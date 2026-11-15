//! The real backend against a synthetic fake install in a temp folder (issue #15).
//!
//! The tree is shaped like an install inside a Wine prefix (as in
//! `crates/starorg-cli/tests/common`), without symlinks so it runs on Windows too. The stock
//! text and GameData come from a test `SourceFactory`, so no Data.p4k is read and no worker
//! runs. App folders are under the temp folder; discovery is pinned to the fake install.
//! Names, ids and text are invented.

use starorg_codex_app::contract::Backend;
use starorg_codex_app::contract::real::UPDATE_JSON;
use starorg_codex_app::contract::real::{
    DialogSlot, Dialogs, FileFilter, RealBackend, RealConfig, SourceFactory, StarOrgDeps,
};
use starorg_codex_app::contract::types::*;
use starorg_codex_app::update::{ReleaseFetch, Updater};
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{Channel, DiscoveryEnv, DiscoveryRequest};
use starorg_core::model::{Blueprint, GameData, Ship, ShipLoc};
use starorg_core::source::{Budget, GameSource, Loaded, Origin, SourceError};
use starorg_net::releases::Reply;
use starorg_net::starorg::mock::{MockServer, MockState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const BOM: &[u8] = b"\xEF\xBB\xBF";

fn stock_bytes() -> Vec<u8> {
    let mut v = BOM.to_vec();
    v.extend_from_slice(b"; synthetic stock-shaped file, filler text only\r\n");
    v.extend_from_slice(b"Frontend_PU_Version=Star Citizen Alpha 4.10\r\n");
    v.extend_from_slice(b"item_NameA=Filler Alpha\r\n");
    v.extend_from_slice(b"item_DescA,P=Filler desc ~mission(Location) <EM4>open tail\r\n");
    v.extend_from_slice(b"item_NameRifle=Fake Rifle Kestrel\r\n");
    v.extend_from_slice(b"fmt_key=Deals %i filler\r\n");
    v.extend_from_slice(
        b"vehicle_NameSYN_Alpha=Synthetic Alpha\r\nvehicle_NameSYN_Alpha_short=Alpha\r\n",
    );
    v.extend_from_slice(b"vehicle_NameSYN_Bravo,P=Synthetic Bravo\r\n");
    for i in 0..60 {
        v.extend_from_slice(format!("filler_key_{i:03}=Filler value {i}\r\n").as_bytes());
    }
    v
}

/// The loc-reference pass of the fake build: a rifle entity names `item_NameRifle`.
fn test_keyrefs() -> starorg_core::groups::KeyRefs {
    let mut b = starorg_core::groups::KeyRefsBuilder::new();
    b.add(
        starorg_core::groups::RefSource {
            struct_type: "EntityClassDefinition".into(),
            item_type: Some("WeaponPersonal".into()),
            size: Some(2),
            manufacturer: Some("KSAR".into()),
            folder: "entities/scitem/weapons".into(),
            ..Default::default()
        },
        ["@item_NameRifle"],
    );
    b.finish()
}

/// Stock text from memory and one blueprint whose product is named by `item_NameRifle`.
/// `.0`: milliseconds each load takes (a slow first read of Data.p4k); `.1` counts loads.
struct TestSource(u64, Arc<std::sync::atomic::AtomicUsize>);

impl GameSource for TestSource {
    fn load(&self, _: &Channel, _: &Budget) -> Result<Loaded, SourceError> {
        self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(self.0));
        Ok(Loaded {
            base_ini: stock_bytes(),
            game_data: GameData {
                blueprints: vec![Blueprint {
                    record_name: "CraftingBlueprintRecord.bp_fake_rifle".into(),
                    id: "bp-fake-rifle".into(),
                    produces_name_key: Some("item_NameRifle".into()),
                    ..Blueprint::default()
                }],
                ships: ["SYN_Alpha", "SYN_Alpha_PU_AI", "SYN_Bravo"]
                    .iter()
                    .map(|r| Ship {
                        record_name: format!("EntityClassDefinition.{r}"),
                        loc: ShipLoc {
                            name: Some(format!("@vehicle_Name{}", &r[..9])),
                            ..ShipLoc::default()
                        },
                        manufacturer: Some("syn".into()),
                        ..Ship::default()
                    })
                    .collect(),
                ..GameData::default()
            },
            keyrefs: Some(test_keyrefs()),
            origin: Origin::LocalExtraction {
                starbreaker_commit: "test".into(),
            },
            warnings: Vec::new(),
        })
    }
}

#[derive(Clone, Default)]
struct TestSources(u64, Arc<std::sync::atomic::AtomicUsize>);

impl SourceFactory for TestSources {
    fn source(&self, _: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        Ok(Box::new(TestSource(self.0, self.1.clone())))
    }
}

struct Fake {
    _t: tempfile::TempDir,
    base: PathBuf,
    lib: PathBuf,
    dirs: AppDirs,
    backend: RealBackend,
    pick: Arc<Mutex<Option<PathBuf>>>,
    /// The folder the last file picker was asked to open in (`None` = Downloads).
    pick_dir: Arc<Mutex<Option<Option<PathBuf>>>>,
}

fn manifest(branch: &str, change: u64, version: &str) -> String {
    format!(
        r#"{{"Data":{{"Branch":"{branch}","RequestedP4ChangeNum":"{change}","Version":"{version}"}}}}"#
    )
}

const LOGIN: &str = "<2026-03-02T10:00:00.000Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 300003 - accountId 900 - name Test_Pilot - state STATE_CURRENT [Team_GameServices][Login]\n";

impl Fake {
    /// StarOrg Tools at a closed loopback port: no test ever reaches the real servers.
    fn new() -> Self {
        Self::with_starorg("http://127.0.0.1:9")
    }

    fn with_starorg(starorg_base: &str) -> Self {
        Self::with(starorg_base, TestSources::default())
    }

    fn with(starorg_base: &str, sources: TestSources) -> Self {
        let t = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(t.path()).unwrap();
        let prefix = base.join("prefix");
        std::fs::create_dir_all(prefix.join("drive_c/users/Public")).unwrap();
        std::fs::write(prefix.join("system.reg"), "").unwrap();
        let lib = prefix.join("drive_c/Program Files/Roberts Space Industries/StarCitizen");
        for (ch, branch, change, version) in [
            ("LIVE", "sc-alpha-4.10.0", 13165946, "4.10.200.58746"),
            ("PTU", "sc-alpha-5.0.0-ptu", 13189690, "5.0.0.1"),
        ] {
            let d = lib.join(ch);
            std::fs::create_dir_all(d.join("Data")).unwrap();
            std::fs::write(d.join("Data.p4k"), b"fake archive").unwrap();
            std::fs::write(
                d.join("build_manifest.id"),
                manifest(branch, change, version),
            )
            .unwrap();
            std::fs::write(
                d.join("USER.cfg"),
                "g_language = english\r\nr_width = 2560\r\n",
            )
            .unwrap();
        }
        std::fs::write(
            lib.join("LIVE/Game.log"),
            format!("<2026-03-02T09:59:00.000Z> Log started\n{LOGIN}"),
        )
        .unwrap();
        let dirs = AppDirs::under(&base.join("app"));
        let mut ctx = Ctx::new(dirs.clone(), "0.0.0-test", "test");
        // The synthetic stock file is small; the real minimum is for real game files.
        ctx.min_stock_keys = 10;
        ctx.p4k_settle = std::time::Duration::from_millis(5);
        let slot = DialogSlot::default();
        let pick = Arc::new(Mutex::new(None));
        let pick_dir = Arc::new(Mutex::new(None));
        slot.set(SharedPick(pick.clone(), pick_dir.clone()));
        let backend = RealBackend::new(RealConfig {
            ctx,
            discovery: DiscoveryRequest {
                game_dir: Some(lib.clone()),
                prefix: None,
            },
            env: DiscoveryEnv::from_process(),
            source: Arc::new(sources),
            dialogs: slot,
        })
        // Never the network in tests: the update check gets a scripted GitHub.
        .with_updater(Updater::with_fetch(
            dirs.data.join(UPDATE_JSON),
            Box::new(FakeGitHub::default()),
        ))
        .with_starorg(StarOrgDeps::local(
            starorg_base,
            Arc::new(starorg_net::starorg::mock::fake_browser),
        ));
        Self {
            _t: t,
            base,
            lib,
            dirs,
            backend,
            pick,
            pick_dir,
        }
    }

    fn ini(&self, ch: &str) -> PathBuf {
        self.lib
            .join(ch)
            .join("Data/Localization/english/global.ini")
    }

    fn install_id(&self) -> String {
        self.backend.overview().unwrap().installs[0].id.clone()
    }

    fn live(&self) -> ChannelRef {
        ChannelRef {
            install: self.install_id(),
            channel: "LIVE".into(),
        }
    }

    fn setup_live(&self) -> SetupResult {
        self.backend
            .setup(SetupRequest {
                install: self.install_id(),
                channels: vec![
                    ChannelConsent {
                        channel: "LIVE".into(),
                        consent: true,
                        switch_language: false,
                    },
                    ChannelConsent {
                        channel: "PTU".into(),
                        consent: false,
                        switch_language: false,
                    },
                ],
                data_source: DataSource::Local,
                preset: Preset::Recommended,
                fix_known_bugs: true,
            })
            .unwrap()
    }

    fn apply(&self, dry_run: bool) -> (ApplyReport, Vec<JobProgress>) {
        let seen = Mutex::new(Vec::new());
        let r = self
            .backend
            .apply(
                ApplyRequest {
                    target: self.live(),
                    dry_run,
                },
                &|p| seen.lock().unwrap().push(p),
            )
            .unwrap();
        (r, seen.into_inner().unwrap())
    }
}

/// `Dialogs` over a shared cell, so a test can change the answer.
struct SharedPick(
    Arc<Mutex<Option<PathBuf>>>,
    Arc<Mutex<Option<Option<PathBuf>>>>,
);

impl Dialogs for SharedPick {
    fn pick_folder(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn pick_file(&self, _: &str, _: FileFilter) -> Option<PathBuf> {
        self.0.lock().unwrap().clone()
    }
    fn pick_file_in(
        &self,
        t: &str,
        f: FileFilter,
        dir: Option<&std::path::Path>,
    ) -> Option<PathBuf> {
        *self.1.lock().unwrap() = Some(dir.map(std::path::Path::to_path_buf));
        self.pick_file(t, f)
    }
    fn save_file(&self, _: &str, _: &str, _: FileFilter) -> Option<PathBuf> {
        None
    }
}

/// Apply refuses to run elevated (spec 11), and GitHub's Windows runners are administrators.
/// These tests turn on the test-hooks override (starorg-core's `test-hooks` feature, a
/// dev-dependency only) so the whole apply flow still runs there. Returns `false`: kept as a
/// function so each apply test states that it depends on the override.
fn elevated() -> bool {
    starorg_core::fsutil::test_allow_elevated(true);
    false
}

#[test]
fn first_run_lists_the_fake_install_unregistered() {
    let f = Fake::new();
    let o = f.backend.overview().unwrap();
    assert!(o.first_run);
    assert_eq!(o.installs.len(), 1);
    let i = &o.installs[0];
    assert!(!i.saved);
    assert!(i.id.starts_with("found-"), "{}", i.id);
    assert_eq!(i.found_by, FoundBy::GameDir);
    let names: Vec<&str> = i.channels.iter().map(|c| c.channel.as_str()).collect();
    assert_eq!(names, ["LIVE", "PTU"]);
    let live = &i.channels[0];
    assert_eq!(live.build.change, Some(13165946));
    assert_eq!(live.build.game_version.as_deref(), Some("4.10.0"));
    assert_eq!(live.status, ApplyState::NotApplied);
    assert_eq!(live.language, Language::English);
    assert!(!live.set_up);
    // Nothing is written by looking.
    assert!(!f.dirs.config.join("config.toml").exists());
}

#[test]
fn setup_plan_apply_status_remove_round_trip() {
    if elevated() {
        return;
    }
    let f = Fake::new();
    let stock = stock_bytes();

    // Review step: a dry-run build; no game file is written.
    let plan = f
        .backend
        .plan_setup(SetupRequest {
            install: f.install_id(),
            channels: vec![ChannelConsent {
                channel: "LIVE".into(),
                consent: true,
                switch_language: false,
            }],
            data_source: DataSource::Local,
            preset: Preset::Recommended,
            fix_known_bugs: true,
        })
        .unwrap();
    assert_eq!(plan.channels.len(), 1, "{plan:?}");
    assert!(plan.channels[0].target.ends_with("global.ini"));
    assert_eq!(plan.channels[0].user_cfg_line, None, "line already right");
    assert!(!f.ini("LIVE").exists());

    let s = f.setup_live();
    assert_eq!(s.set_up, ["LIVE"]);
    assert_eq!(s.skipped, ["PTU"]);
    let o = f.backend.overview().unwrap();
    assert!(!o.first_run);
    assert_eq!(o.default_channel.as_deref(), Some("LIVE"));
    assert_eq!(o.active_install.as_deref(), Some(s.install.as_str()));

    // Dry run: everything up to the write.
    let (r, _) = f.apply(true);
    assert_eq!(r.result, ApplyResult::DryRun, "{r:?}");
    assert!(!f.ini("LIVE").exists());

    // Apply: progress in order, the stock bytes (no generator has data here), Applied.
    let (r, events) = f.apply(false);
    assert_eq!(r.result, ApplyResult::Applied, "{r:?}");
    assert!(
        r.backup
            .as_deref()
            .is_some_and(|b| b.ends_with("-pre-existing"))
    );
    assert_eq!(std::fs::read(f.ini("LIVE")).unwrap(), stock);
    let phases: Vec<(u32, JobPhase)> = events.iter().map(|e| (e.step, e.phase)).collect();
    assert_eq!(
        phases,
        [
            (1, JobPhase::Running),
            (2, JobPhase::Running),
            (3, JobPhase::Done)
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| !e.cancellable && e.kind == JobKind::Apply)
    );
    let st = f.backend.channel_status(f.live()).unwrap();
    assert_eq!(st.status, ApplyState::Applied);
    assert!(st.set_up);
    assert_eq!(st.cache, CacheState::Fresh);
    assert_eq!(
        st.applied_build.as_ref().and_then(|b| b.change),
        Some(13165946)
    );
    assert_eq!(
        st.last_apply.as_ref().map(|l| l.result),
        Some(ApplyResult::Applied)
    );
    // Again: nothing changed, the backup of identical bytes is not repeated.
    let backups = f.backend.list_backups(f.live()).unwrap();
    assert_eq!(backups.backups.len(), 1);
    assert!(backups.offers.remove);

    // Strings: the table is the merged output.
    let page = f
        .backend
        .query_strings(StringsQuery {
            target: f.live(),
            search: "filler alpha".into(),
            sources: vec![],
            categories: vec![],
            changed_only: false,
            sort: StringSort::Key,
            offset: 0,
            limit: 50,
            groups: vec![],
            manufacturers: vec![],
            sizes: vec![],
            group_by: GroupBy::Type,
        })
        .unwrap();
    // The name key, and the description key through its owner (Strings owners).
    assert_eq!(page.total, 2);
    assert_eq!(page.rows[0].key, "item_DescA,P");
    assert_eq!(page.rows[1].key, "item_NameA");
    let owner = |r: &StringRow| r.owner.as_deref().cloned().unwrap();
    assert_eq!(
        (owner(&page.rows[0]).name, owner(&page.rows[0]).field),
        ("Filler Alpha".to_string(), Some(OwnerField::Description))
    );
    assert_eq!(owner(&page.rows[1]).kind, OwnerKind::Named);
    // Groups (addendum 8): the app-started extraction ran the loc-reference pass.
    assert_eq!(page.group_state, GroupState::Exact);
    assert_eq!(page.rows[1].group, "items/misc");
    let rifle_query = |f: &Fake| StringsQuery {
        target: f.live(),
        search: String::new(),
        sources: vec![],
        categories: vec![],
        changed_only: false,
        sort: StringSort::Group,
        offset: 0,
        limit: 1000,
        groups: vec!["fps-weapons".into()],
        manufacturers: vec![],
        sizes: vec![2],
        group_by: GroupBy::Size,
    };
    let rifle = f.backend.query_strings(rifle_query(&f)).unwrap();
    assert_eq!(rifle.total, 1, "{:?}", rifle.rows);
    let r = &rifle.rows[0];
    assert_eq!(
        (r.key.as_str(), r.group.as_str()),
        ("item_NameRifle", "fps-weapons/weapons")
    );
    assert_eq!(r.size, Some(2));
    // KSAR has no manufacturer_NameKSAR in the fake stock: no manufacturer facet.
    assert_eq!(r.manufacturer, None);
    assert!(rifle.groups.iter().any(|g| g.id == "2" && g.count == 1));
    // Named by its own text; no GameData record in the fake build.
    assert_eq!(owner(r).name, "Fake Rifle Kestrel", "{:?}", r.owner);
    // A key nobody owns: the readable key.
    let filler = f
        .backend
        .query_strings(StringsQuery {
            search: "filler_key_007".into(),
            sort: StringSort::Owner,
            group_by: GroupBy::None,
            groups: vec![],
            sizes: vec![],
            ..rifle_query(&f)
        })
        .unwrap();
    let o = owner(&filler.rows[0]);
    assert_eq!(
        (o.kind, o.name.as_str()),
        (OwnerKind::Key, "filler · key 007")
    );
    assert_eq!(rifle.groups.last().unwrap().id, "none");
    let fmt = f
        .backend
        .query_strings(StringsQuery {
            target: f.live(),
            search: "fmt_key".into(),
            sources: vec![],
            categories: vec![],
            changed_only: false,
            sort: StringSort::Key,
            offset: 0,
            limit: 5,
            groups: vec![],
            manufacturers: vec![],
            sizes: vec![],
            group_by: GroupBy::None,
        })
        .unwrap();
    assert_eq!(fmt.rows[0].group, "other");
    assert!(fmt.groups.is_empty());
    assert_eq!(
        f.backend.read_string_groups(f.live()).unwrap(),
        GroupState::Exact
    );

    // An override is checked with M1's rules, then makes the channel stale.
    let bad = f
        .backend
        .set_override(OverrideRequest {
            target: f.live(),
            key: "fmt_key".into(),
            value: "Deals filler".into(),
        })
        .unwrap();
    assert!(!bad.accepted);
    assert_eq!(bad.rule, Some(OverrideRule::FormatSpecifiers));
    let ok = f
        .backend
        .set_override(OverrideRequest {
            target: f.live(),
            key: "item_namea".into(),
            value: "Filler Alpha Prime".into(),
        })
        .unwrap();
    assert!(ok.accepted, "{ok:?}");
    let d = ok.detail.unwrap();
    assert_eq!(d.row.result, "Filler Alpha Prime");
    assert_eq!(d.row.source, StringSource::Edited);
    assert!(matches!(
        f.backend.channel_status(f.live()).unwrap().status,
        ApplyState::Stale { ref reasons } if reasons.iter().any(|r| r == "overrides")
    ));
    assert_eq!(
        f.backend.list_overrides(f.live()).unwrap()[0].key,
        "item_NameA"
    );
    f.backend
        .reset_override(KeyRequest {
            target: f.live(),
            key: "item_NameA".into(),
        })
        .unwrap();
    assert_eq!(
        f.backend.channel_status(f.live()).unwrap().status,
        ApplyState::Applied
    );

    // Profile: M2 fields round-trip through profile.toml.
    let mut p = f.backend.get_profile(f.live()).unwrap();
    assert_eq!(p.preset, Some(Preset::Recommended));
    p.ships = false;
    p.tags.components.enclosing = TagEnclosing::Round;
    p.lines_off = vec!["components.health".into()];
    let saved = f
        .backend
        .save_profile(SaveProfileRequest {
            target: f.live(),
            profile: p.clone(),
        })
        .unwrap();
    assert_eq!(saved.preset, None);
    assert!(!saved.ships);
    assert_eq!(saved.tags.components.enclosing, TagEnclosing::Round);
    assert_eq!(saved.lines_off, ["components.health"]);
    let mut bad_header = saved.clone();
    bad_header.header = "<EM4>stats".into();
    let e = f
        .backend
        .save_profile(SaveProfileRequest {
            target: f.live(),
            profile: bad_header,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert_eq!(
        f.backend.get_profile(f.live()).unwrap(),
        saved,
        "refused save kept the old one"
    );

    // Remove: back to stock (no file), status Not applied.
    let r = f
        .backend
        .remove_changes(RemoveRequest {
            target: f.live(),
            allow_foreign: false,
        })
        .unwrap();
    assert_eq!(r.result, ApplyResult::Removed, "{r:?}");
    assert!(!f.ini("LIVE").exists());
    assert_eq!(
        f.backend.channel_status(f.live()).unwrap().status,
        ApplyState::NotApplied
    );
}

#[test]
fn apply_while_the_channel_lock_is_held_is_skipped_with_a_reason() {
    if elevated() {
        return;
    }
    let f = Fake::new();
    let s = f.setup_live();
    let _held = starorg_core::lock::try_lock(&f.dirs.channel_lock(&s.install, "LIVE")).unwrap();
    let (r, events) = f.apply(false);
    assert_eq!(r.result, ApplyResult::Skipped);
    assert!(
        r.reason
            .as_deref()
            .unwrap()
            .contains("another StarOrg Codex run"),
        "{r:?}"
    );
    assert!(!f.ini("LIVE").exists());
    assert_eq!(events.last().unwrap().phase, JobPhase::Done);
}

/// The wizard's Enhancements step on a first run (Windows report: an empty preview column). The
/// install is not saved and its game data not read, so the preview is an empty list -- not an
/// error -- and the UI must say why. The Review dry run (plan_setup) reads the data; after it the
/// same request has items, still under the `found-...` id the wizard holds.
#[test]
fn preview_is_empty_until_the_dry_run_reads_the_game_data() {
    if elevated() {
        return;
    }
    let f = Fake::new();
    let found = f.install_id();
    assert!(found.starts_with("found-"), "{found}");
    let req = || PreviewRequest {
        target: ChannelRef {
            install: found.clone(),
            channel: "LIVE".into(),
        },
        profile: None,
        preset: Some(Preset::Recommended),
    };
    let cache = |f: &Fake| f.backend.overview().unwrap().installs[0].channels[0].cache;
    assert_eq!(f.backend.build_preview(req()).unwrap(), Vec::new());
    assert_eq!(
        cache(&f),
        CacheState::Missing,
        "empty because nothing was read"
    );
    assert!(
        !f.dirs.config.join("config.toml").exists(),
        "a preview saves nothing"
    );

    f.backend
        .plan_setup(SetupRequest {
            install: found.clone(),
            channels: vec![ChannelConsent {
                channel: "LIVE".into(),
                consent: true,
                switch_language: false,
            }],
            data_source: DataSource::Local,
            preset: Preset::Recommended,
            fix_known_bugs: true,
        })
        .unwrap();
    assert_eq!(
        cache(&f),
        CacheState::Fresh,
        "the dry run read and kept the game data"
    );
    // The same request now builds from the read data (the fake build has no stat rows, so the
    // list itself may stay empty; the UI tells "not read" from "nothing to show" by the cache).
    f.backend.build_preview(req()).unwrap();
    assert!(!f.ini("LIVE").exists(), "nothing written to the game");
}

/// Review asks again while the wizard's first read still runs (a choice changed, or Enhancements'
/// "Read game data now" was still going): the second plan_setup waits for the first read and
/// uses it, instead of skipping with "another extraction is running" and zero counts.
#[test]
fn concurrent_plan_setup_waits_for_the_running_read() {
    if elevated() {
        return;
    }
    let src = TestSources(400, Arc::default());
    let f = Arc::new(Fake::with("http://127.0.0.1:9", src.clone()));
    let req = {
        let id = f.install_id();
        move || SetupRequest {
            install: id.clone(),
            channels: vec![ChannelConsent {
                channel: "LIVE".into(),
                consent: true,
                switch_language: false,
            }],
            data_source: DataSource::Local,
            preset: Preset::Recommended,
            fix_known_bugs: true,
        }
    };
    let runs: Vec<_> = (0..3)
        .map(|_| {
            let (f, r) = (f.clone(), req.clone());
            std::thread::spawn(move || f.backend.plan_setup(r()).unwrap())
        })
        .collect();
    for h in runs {
        let plan = h.join().unwrap();
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
        assert_eq!(plan.channels.len(), 1);
    }
    assert_eq!(
        src.1.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "one read, shared"
    );
}

/// Another process (a launch hook, the CLI) holds the extraction lock: plan_setup says Busy (the
/// wizard retries) rather than an Ok plan with zero counts.
#[test]
fn plan_setup_while_another_process_reads_is_busy() {
    if elevated() {
        return;
    }
    let f = Fake::new();
    let held = starorg_core::lock::try_lock(&f.dirs.extract_lock()).unwrap();
    let r = f.backend.plan_setup(SetupRequest {
        install: f.install_id(),
        channels: vec![ChannelConsent {
            channel: "LIVE".into(),
            consent: true,
            switch_language: false,
        }],
        data_source: DataSource::Local,
        preset: Preset::Recommended,
        fix_known_bugs: true,
    });
    assert_eq!(r.unwrap_err().code, ErrorCode::Busy);
    drop(held);
}

#[test]
fn apply_before_setup_needs_setup_and_writes_nothing() {
    let f = Fake::new();
    let (r, _) = f.apply(false);
    assert_eq!(r.result, ApplyResult::NeedsSetup, "{r:?}");
    assert!(!f.ini("LIVE").exists());
}

const SC_EXPORT: &str = r#"{"version": 1, "exportedAt": "2026-03-03T12:00:00Z", "blueprints": [
    {"name": "[L] Fake Rifle Kestrel", "completed": true, "favorite": true},
    {"name": "Unknown Thing", "completed": true, "favorite": false},
    {"name": "Only Tracked", "completed": false, "favorite": false}
  ]}"#;

#[test]
fn smart_citizen_import_dry_run_reports_and_writes_nothing() {
    let f = Fake::new();
    let s = f.setup_live();
    let export = f.base.join("owned-blueprints.json");
    std::fs::write(&export, SC_EXPORT).unwrap();
    let req = |dry_run| ImportRequest {
        target: f.live(),
        character: "300003".into(),
        dry_run,
        reuse_last: false,
        replace: false,
    };

    // No file chosen: cancelled, nothing read.
    let e = f.backend.import_smart_citizen(req(true)).unwrap_err();
    assert_eq!(e.code, ErrorCode::Cancelled);

    *f.pick.lock().unwrap() = Some(export.clone());
    let r = f.backend.import_smart_citizen(req(true)).unwrap();
    assert!(r.dry_run);
    assert_eq!(r.file, "owned-blueprints.json");
    assert_eq!(r.total, 3);
    assert_eq!(r.not_completed, 1);
    assert_eq!(r.imported.len(), 1, "{r:?}");
    assert_eq!(r.unmatched, ["Unknown Thing"]);
    assert!(!r.empty_catalog);
    let set = f
        .dirs
        .channel_dir(&s.install, "LIVE")
        .join("blueprints/300003.json");
    assert!(!set.exists(), "dry run wrote {}", set.display());

    // For real: the character owns it now.
    let r = f.backend.import_smart_citizen(req(false)).unwrap();
    assert_eq!(r.imported.len(), 1);
    let owned = f
        .backend
        .owned_blueprints(CharacterRequest {
            target: f.live(),
            character: "300003".into(),
        })
        .unwrap();
    assert_eq!(owned.len(), 1);
    assert!(owned[0].favorite);
    assert!(owned[0].sources.iter().any(|s| s == "import:smart-citizen"));
    let chars = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(chars.characters.len(), 1);
    assert_eq!(chars.characters[0].owned, 1);
    assert_eq!(chars.active.mode, CharacterMode::Auto);
    assert_eq!(chars.active.resolved.as_deref(), Some("300003"));

    // The finder: the build's catalogue (cached by the import) with the owner, and every
    // other channel without a catalogue until its build is read. Ownership stays per channel.
    let all = f
        .backend
        .blueprint_finder(BlueprintFinderRequest {
            install: f.live().install,
            channels: vec![],
        })
        .unwrap();
    let live = all.channels.iter().find(|c| c.channel == "LIVE").unwrap();
    assert_eq!((live.catalog, live.characters, live.owned), (Some(1), 1, 1));
    assert!(
        all.channels
            .iter()
            .filter(|c| c.channel != "LIVE")
            .all(|c| c.catalog.is_none() && c.owned == 0)
    );
    let rifle = all
        .rows
        .iter()
        .find(|r| r.name == "Fake Rifle Kestrel")
        .unwrap();
    assert_eq!(rifle.record.as_deref(), Some("bp_fake_rifle"));
    // Reading creates nothing for channels without blueprint data.
    for c in all.channels.iter().filter(|c| c.channel != "LIVE") {
        let d = f
            .dirs
            .channel_dir(&s.install, &c.channel)
            .join("blueprints");
        assert!(!d.exists(), "finder created {}", d.display());
    }
    assert_eq!(rifle.channels, ["LIVE"]);
    assert_eq!(rifle.owners.len(), 1);
    assert_eq!(
        (
            rifle.owners[0].channel.as_str(),
            rifle.owners[0].character.as_str()
        ),
        ("LIVE", "300003")
    );
    assert!(rifle.owners[0].favorite);
    // The catalogue is kept per build; a second call answers from memory, same rows.
    let again = f
        .backend
        .blueprint_finder(BlueprintFinderRequest {
            install: f.live().install,
            channels: vec!["LIVE".into()],
        })
        .unwrap();
    assert_eq!(again.rows, all.rows);
}

/// The store v0.2.0 left after a character reset: the same account and name under an old
/// geid holding the import, the new geid (the newest session) empty. On first load the
/// store is backed up and merged once, Activity says so, the channel shows stale for the
/// owned blueprints, and a re-import reads the remembered file with no picker.
#[test]
fn an_old_store_with_a_reset_character_is_repaired_once() {
    if elevated() {
        return;
    }
    let f = Fake::new();
    let s = f.setup_live();
    let export = f.base.join("owned-blueprints.json");
    std::fs::write(&export, SC_EXPORT).unwrap();
    let bp = f.dirs.channel_dir(&s.install, "LIVE").join("blueprints");
    std::fs::create_dir_all(&bp).unwrap();
    std::fs::write(
        bp.join("characters.json"),
        r#"{"characters": {
            "100001": {"account_id": "900", "name": "Test_Pilot", "last_seen": "2026-01-01T00:00:00.000Z"},
            "300003": {"account_id": "900", "name": "Test_Pilot\r", "last_seen": "2026-03-02T10:00:00.000Z"}}}"#,
    )
    .unwrap();
    std::fs::write(
        bp.join("100001.json"),
        r#"{"schema": 1, "geid": "100001", "entries": {"fake rifle kestrel": {"name": "Fake Rifle Kestrel",
            "ids": ["bp_rifle"], "sources": [{"kind": "import:smart-citizen", "time": "2026-01-05T00:00:00Z"}]}}}"#,
    )
    .unwrap();
    let (r, _) = f.apply(false);
    assert_eq!(r.result, ApplyResult::Applied, "{r:?}");

    let v = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(v.characters.len(), 1, "{v:?}");
    assert_eq!(v.characters[0].id, "300003");
    assert_eq!(v.characters[0].name, "Test_Pilot");
    assert_eq!(v.characters[0].owned, 1);
    assert_eq!(v.notice, None);
    let backups: Vec<_> = std::fs::read_dir(bp.join("backups")).unwrap().collect();
    assert_eq!(backups.len(), 1);
    let log = f.backend.activity(20).unwrap();
    let line = log
        .iter()
        .find(|e| {
            e.message
                .contains("Owned blueprints updated for this version")
        })
        .expect("one activity line");
    assert!(
        line.message.contains("Merged 2 characters into 1"),
        "{}",
        line.message
    );
    // Auto already fell back to the old geid (it owned the import), so the apply above
    // marked it; the merge keeps the same set: nothing to re-apply.
    assert_eq!(
        f.backend.channel_status(f.live()).unwrap().status,
        ApplyState::Applied
    );

    // Once: a second load changes nothing more.
    f.backend.list_characters(f.live()).unwrap();
    assert_eq!(std::fs::read_dir(bp.join("backups")).unwrap().count(), 1);
    let again = f.backend.activity(50).unwrap();
    assert_eq!(
        again
            .iter()
            .filter(|e| e.message.contains("Owned blueprints updated"))
            .count(),
        1
    );

    // reuse_last reads the file the last real import read (import.json); before any import,
    // or when that file is gone, it opens the picker.
    let req = |dry_run, reuse_last, replace| ImportRequest {
        target: f.live(),
        character: "100001".into(),
        dry_run,
        reuse_last,
        replace,
    };
    let e = f
        .backend
        .import_smart_citizen(req(true, true, false))
        .unwrap_err();
    assert_eq!(
        e.code,
        ErrorCode::Cancelled,
        "nothing imported yet: the picker, cancelled"
    );
    assert_eq!(
        *f.pick_dir.lock().unwrap(),
        Some(None),
        "no import yet: the picker opens in Downloads, never another tool's folder"
    );
    *f.pick.lock().unwrap() = Some(export.clone());
    let r = f
        .backend
        .import_smart_citizen(req(true, true, false))
        .unwrap();
    assert_eq!(r.known, ["[L] Fake Rifle Kestrel"], "{r:?}");
    // A dry run remembers nothing; the real import does.
    assert_eq!(
        f.backend.list_characters(f.live()).unwrap().last_import,
        None
    );
    f.backend
        .import_smart_citizen(req(false, false, false))
        .unwrap();
    *f.pick.lock().unwrap() = None;
    assert_eq!(
        f.backend
            .list_characters(f.live())
            .unwrap()
            .last_import
            .as_deref(),
        Some("owned-blueprints.json")
    );
    std::fs::write(&export, r#"{"version": 1, "blueprints": []}"#).unwrap();
    let r = f
        .backend
        .import_smart_citizen(req(true, true, true))
        .unwrap();
    assert_eq!(r.removed, ["Fake Rifle Kestrel"], "{r:?}");
    let r = f
        .backend
        .import_smart_citizen(req(false, true, true))
        .unwrap();
    assert_eq!(r.removed.len(), 1);
    // The owned set changed: the channel offers a re-apply.
    assert!(matches!(
        f.backend.channel_status(f.live()).unwrap().status,
        ApplyState::Stale { ref reasons } if reasons.iter().any(|r| r == "owned blueprints")
    ));
    // The remembered file moved away: the picker again (here: cancelled).
    let moved = f.base.join("moved.json");
    std::fs::rename(&export, &moved).unwrap();
    let e = f
        .backend
        .import_smart_citizen(req(true, true, false))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Cancelled);
    *f.pick.lock().unwrap() = Some(moved.clone());
    let r = f
        .backend
        .import_smart_citizen(req(true, true, false))
        .unwrap();
    assert_eq!(r.file, "moved.json");
    // The picker opened in the folder of the last import (ours, from import.json).
    // Compare the plain form: on Windows the app strips the `\\?\` prefix (fsutil::canonicalize).
    let plain = starorg_core::fsutil::canonicalize(&f.base).unwrap();
    assert_eq!(*f.pick_dir.lock().unwrap(), Some(Some(plain)));
    assert_eq!(
        f.backend.list_characters(f.live()).unwrap().characters[0].owned,
        0
    );

    // Re-scan: dry run first, then for real; the log has no blueprints, nothing to change.
    let r = f
        .backend
        .rescan_logs(RescanRequest {
            target: f.live(),
            dry_run: true,
            replace: false,
        })
        .unwrap();
    assert!(r.dry_run && r.files_read == 1, "{r:?}");
    assert!(
        r.characters
            .iter()
            .all(|c| c.added.is_empty() && c.removed.is_empty())
    );
}

/// A reset that also changed the account (nothing proves one character): no merge, but
/// auto uses the character that owns something and says so; a setting naming a merged geid
/// shows the kept one.
#[test]
fn auto_falls_back_and_settings_follow_merges() {
    if elevated() {
        return;
    }
    let f = Fake::new();
    let s = f.setup_live();
    let data = f.dirs.channel_dir(&s.install, "LIVE");
    let bp = data.join("blueprints");
    std::fs::create_dir_all(&bp).unwrap();
    std::fs::write(
        bp.join("characters.json"),
        r#"{"repair": 1, "characters": {
            "100001": {"account_id": "800", "name": "Test_Pilot", "last_seen": "2026-01-01T00:00:00.000Z"}}}"#,
    )
    .unwrap();
    std::fs::write(
        bp.join("100001.json"),
        r#"{"schema": 1, "geid": "100001", "entries": {"fake rifle kestrel": {"name": "Fake Rifle Kestrel",
            "sources": [{"kind": "import:smart-citizen", "time": "2026-01-05T00:00:00Z"}]}}}"#,
    )
    .unwrap();
    // The log's login (geid 300003, account 900) is the newest session and owns nothing.
    let v = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(v.characters.len(), 2, "{v:?}");
    assert_eq!(v.active.resolved.as_deref(), Some("100001"));
    assert_eq!(v.active.fallback_from.as_deref(), Some("300003"));
    let n = v.notice.unwrap();
    assert!(
        n.contains("on different accounts") && n.contains("Any of my characters"),
        "{n}"
    );
    let (r, _) = f.apply(false);
    assert_eq!(r.result, ApplyResult::Applied, "{r:?}");
    assert_eq!(
        f.backend.channel_status(f.live()).unwrap().status,
        ApplyState::Applied
    );

    // Without account ids the notice does not claim different accounts.
    std::fs::write(
        bp.join("characters.json"),
        r#"{"repair": 1, "characters": {
            "100001": {"name": "Test_Pilot", "last_seen": "2026-01-01T00:00:00.000Z"},
            "300003": {"account_id": "900", "name": "Test_Pilot", "last_seen": "2026-03-02T10:00:00.000Z"}}}"#,
    )
    .unwrap();
    let n = f.backend.list_characters(f.live()).unwrap().notice.unwrap();
    assert!(
        !n.contains("different accounts") && n.contains("do not show"),
        "{n}"
    );

    // A manual pick of a merged geid shows (and is saved as) the kept one.
    std::fs::write(
        bp.join("characters.json"),
        r#"{"repair": 1, "aliases": {"100001": "300003"}, "characters": {
            "300003": {"account_id": "900", "name": "Test_Pilot", "last_seen": "2026-03-02T10:00:00.000Z"}}}"#,
    )
    .unwrap();
    let mut st = starorg_core::state::ChannelState::load(&data).unwrap();
    st.active_character = Some("100001".into());
    st.save(&data).unwrap();
    let v = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(v.active.mode, CharacterMode::Manual);
    assert_eq!(v.active.character.as_deref(), Some("300003"));
    assert_eq!(
        starorg_core::state::ChannelState::load(&data)
            .unwrap()
            .active_character
            .as_deref(),
        Some("300003")
    );

    // Another character's set unreadable (truncated) and an odd id the repair kept: the
    // view, the status and the apply still work; the apply says what was not read.
    std::fs::write(
        bp.join("characters.json"),
        r#"{"repair": 1, "characters": {
            "100001": {"name": "Old_Pilot", "last_seen": "2026-01-01T00:00:00.000Z"},
            "ab-c": {"name": "Odd_Pilot", "last_seen": "2026-02-01T00:00:00.000Z"},
            "300003": {"account_id": "900", "name": "Test_Pilot", "last_seen": "2026-03-02T10:00:00.000Z"}}}"#,
    )
    .unwrap();
    std::fs::write(bp.join("100001.json"), b"{\"schema\": 1, \"entries\": {").unwrap();
    let mut st = starorg_core::state::ChannelState::load(&data).unwrap();
    st.active_character = None;
    st.save(&data).unwrap();
    let v = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(v.characters.len(), 3);
    assert_eq!(v.active.resolved.as_deref(), Some("300003"));
    f.backend.channel_status(f.live()).unwrap();
    let (r, _) = f.apply(false);
    let unread: Vec<_> = r
        .warnings
        .iter()
        .filter(|w| w.kind == "owned-unread")
        .collect();
    assert_eq!(unread.len(), 2, "{:?}", r.warnings);
}

fn bp_line(t: &str, name: &str) -> String {
    format!(
        "<{t}> [Notice] <SHUDEvent_OnNotification> Added notification \"Received Blueprint: {name}: \" [7] to queue. New queue size: 1 [Team_CoreGameplayFeatures][Missions][Comms]\n"
    )
}

/// A StarOrg stand-in whose lookup knows the fake build's rifle by name.
fn starorg_server() -> MockServer {
    let mut st = MockState::default();
    st.lookup
        .push(serde_json::json!({"id": 10, "k": "bp_fake_rifle", "n": "Fake Rifle Kestrel"}));
    MockServer::with(st)
}

fn set_sync(f: &Fake, on: bool, edit: impl FnOnce(&mut StarOrgSettings)) -> Settings {
    let mut s = f.backend.get_settings().unwrap();
    s.network.starorg = on;
    edit(&mut s.starorg);
    f.backend.save_settings(s).unwrap()
}

/// Test_Pilot received the rifle and one blueprint StarOrg does not know.
fn starorg_fake(m: &MockServer) -> Fake {
    let f = Fake::with_starorg(&m.base);
    std::fs::write(
        f.lib.join("LIVE/Game.log"),
        format!(
            "<2026-03-02T09:59:00.000Z> Log started\n{LOGIN}{}{}",
            bp_line("2026-03-02T10:05:00.000Z", "Fake Rifle Kestrel"),
            bp_line("2026-03-02T10:06:00.000Z", "Unknown Test Visor"),
        ),
    )
    .unwrap();
    f.setup_live();
    let chars = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(chars.characters[0].owned, 2);
    f
}

#[test]
fn starorg_needs_the_network_switch_and_sends_nothing_without_it() {
    let m = starorg_server();
    let f = starorg_fake(&m);
    let st = f.backend.starorg_status("LIVE".into()).unwrap();
    assert!(!st.enabled && !st.connected);
    assert_eq!(st.host, "starorg.tools");
    assert_eq!(st.push_count, 2);
    for e in [
        f.backend.starorg_login("LIVE".into()).unwrap_err(),
        f.backend.starorg_push("LIVE".into()).unwrap_err(),
        f.backend.starorg_sync("LIVE".into()).unwrap_err(),
    ] {
        assert_eq!(e.code, ErrorCode::NetworkOff);
    }
    assert!(m.requests().is_empty());
}

#[test]
fn starorg_sign_in_pull_push_hangar_and_sign_out_against_a_local_server() {
    let m = starorg_server();
    let f = starorg_fake(&m);
    set_sync(&f, true, |_| {});
    let st = f.backend.starorg_login("LIVE".into()).unwrap();
    assert!(st.connected && !st.expired);
    assert_eq!(st.user.as_deref(), Some("test_pilot_discord"));
    assert_eq!(st.guilds.len(), 2);
    // The first org with blueprint tracking is picked.
    assert_eq!(st.guild.as_deref(), Some("Synthetic Org"));
    assert_eq!(st.token_store, Some(TokenStoreKind::Keyring));
    assert!(st.expires.is_some());
    // No token in the app's own files.
    let app_json = std::fs::read_to_string(f.dirs.config.join("app.json")).unwrap();
    assert!(app_json.contains("test_pilot_discord"));
    assert!(!app_json.contains("mock-"), "{app_json}");

    // Each kind needs its switch: nothing is sent while it is off.
    let before = m.requests().len();
    assert_eq!(
        f.backend.starorg_push("LIVE".into()).unwrap_err().code,
        ErrorCode::Invalid
    );
    let pull = StarOrgPullRequest {
        channel: "LIVE".into(),
        character: "300003".into(),
    };
    assert_eq!(
        f.backend.starorg_pull(pull.clone()).unwrap_err().code,
        ErrorCode::Invalid
    );
    let hangar = FleetImportRequest {
        target: f.live(),
        source: FleetSource::StarOrg,
        dry_run: true,
        merge: true,
    };
    assert_eq!(
        f.backend.fleet_import(hangar.clone()).unwrap_err().code,
        ErrorCode::Invalid
    );
    assert_eq!(m.requests().len(), before);

    set_sync(&f, true, |s| {
        s.push_blueprints = true;
        s.pull_blueprints = true;
        s.pull_fleet = true;
    });
    let r = f.backend.starorg_push("LIVE".into()).unwrap();
    assert_eq!(r.sent, 1);
    assert_eq!(r.added, ["Fake Rifle Kestrel"]);
    assert_eq!(r.unmatched, ["Unknown Test Visor"]);
    let add = m
        .requests()
        .into_iter()
        .find(|q| q.method == "POST" && q.path.ends_with("/my-blueprints"))
        .unwrap();
    assert!(add.path.contains("/guilds/100000000000000001/"));
    assert!(add.body.contains("\"blueprint_ids\":[10]"), "{}", add.body);
    assert!(
        add.body.contains("2026-03-02T10:05:00.000Z"),
        "{}",
        add.body
    );
    // Character names and ids never leave the computer.
    assert!(
        m.requests()
            .iter()
            .all(|q| !q.body.contains("Test_Pilot") && !q.body.contains("300003"))
    );

    let r = f.backend.starorg_pull(pull).unwrap();
    assert_eq!(r.direction, SyncDirection::Pull);
    // The server's list: the seeded medical gun and the rifle just pushed.
    assert_eq!(
        r.added.len() + r.unmatched.len() + r.already as usize,
        2,
        "{r:?}"
    );
    assert!(r.already >= 1, "the rifle was already known locally: {r:?}");

    let fleet = f.backend.fleet_import(hangar).unwrap();
    assert_eq!(fleet.file.as_deref(), Some("StarOrg Tools hangar"));
    assert_eq!(fleet.total, 3);

    let run = f.backend.starorg_sync("LIVE".into()).unwrap();
    assert_eq!(run.reports.len(), 2, "{run:?}");
    assert!(run.fleet.is_some() && run.skipped.is_empty(), "{run:?}");
    let st = f.backend.starorg_status("LIVE".into()).unwrap();
    assert!(st.last_sync.is_some());

    // Another org; one without blueprint tracking is a 404 from the server.
    f.backend
        .set_starorg_guild(StarOrgGuildRequest {
            channel: "LIVE".into(),
            guild: "100000000000000002".into(),
        })
        .unwrap();
    assert_eq!(
        f.backend.starorg_push("LIVE".into()).unwrap_err().code,
        ErrorCode::NotFound
    );

    // Diagnostics name neither the StarOrg user nor the orgs from the sync log lines.
    let d = f.backend.export_diagnostics().unwrap();
    for name in ["test_pilot_discord", "Synthetic Org", "100000000000000001"] {
        assert!(!d.text.contains(name), "{name} in diagnostics");
    }
    assert!(
        d.text.contains("<org>"),
        "the sync lines are there, redacted"
    );

    // Sign out revokes on the server and forgets the token.
    let st = f.backend.starorg_logout("LIVE".into()).unwrap();
    assert!(!st.connected && st.user.is_none());
    assert_eq!(m.state().revoked, 1);
    assert_eq!(m.valid_tokens(), 0);
}

#[test]
fn starorg_switching_environment_signs_out_and_a_revoked_token_asks_again() {
    let m = starorg_server();
    let f = starorg_fake(&m);
    set_sync(&f, true, |s| s.push_blueprints = true);
    f.backend.starorg_login("LIVE".into()).unwrap();
    assert_eq!(m.valid_tokens(), 1);
    // The server's kill switch: refused with "not enabled", the token stays.
    m.state().enabled = false;
    let e = f.backend.starorg_push("LIVE".into()).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotImplemented, "{e:?}");
    m.state().enabled = true;
    assert!(f.backend.starorg_status("LIVE".into()).unwrap().connected);

    let s = set_sync(&f, true, |s| s.environment = StarOrgEnv::Dev);
    assert_eq!(s.starorg.environment, StarOrgEnv::Dev);
    let st = f.backend.starorg_status("LIVE".into()).unwrap();
    assert_eq!(st.environment, StarOrgEnv::Dev);
    assert_eq!(st.host, "dev.starorg.tools");
    assert!(!st.connected && st.user.is_none());
    assert_eq!(
        m.state().revoked,
        1,
        "the production token was revoked on the way out"
    );
    // The environment is in config.toml for the CLI and hooks.
    let toml = std::fs::read_to_string(f.dirs.config_toml()).unwrap();
    assert!(toml.contains("environment = \"dev\""), "{toml}");

    // Signed in on dev, then revoked elsewhere ("Sign out everywhere"): sign in again.
    f.backend.starorg_login("LIVE".into()).unwrap();
    m.revoke_all();
    let e = f.backend.starorg_push("LIVE".into()).unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid, "{e:?}");
    let st = f.backend.starorg_status("LIVE".into()).unwrap();
    assert!(!st.connected && st.expired, "{st:?}");
}

#[test]
fn starorg_sign_out_with_the_switch_off_says_it_did_not_revoke() {
    let m = starorg_server();
    let f = starorg_fake(&m);
    set_sync(&f, true, |_| {});
    f.backend.starorg_login("LIVE".into()).unwrap();
    set_sync(&f, false, |_| {});
    let before = m.requests().len();
    let st = f.backend.starorg_logout("LIVE".into()).unwrap();
    assert!(!st.connected && st.user.is_none());
    // Nothing sent: the token is gone here but still valid on the server, and the log says so.
    assert_eq!(m.requests().len(), before);
    assert_eq!(m.state().revoked, 0);
    assert_eq!(m.valid_tokens(), 1);
    let log = f.backend.activity(50).unwrap();
    assert!(
        log.iter().any(|e| e.message.contains("not revoked there")),
        "{log:?}"
    );
}

#[test]
fn starorg_environment_changed_outside_the_app_signs_the_old_one_out() {
    let m = starorg_server();
    let f = starorg_fake(&m);
    set_sync(&f, true, |_| {});
    f.backend.starorg_login("LIVE".into()).unwrap();
    // `starorg-codex sync set --environment dev`: config.toml only, no token touched.
    let mut cfg = starorg_core::config::Config::load(&f.dirs).unwrap();
    cfg.sync.environment = starorg_core::starorg::Environment::Dev;
    cfg.save(&f.dirs).unwrap();
    let st = f.backend.starorg_status("LIVE".into()).unwrap();
    assert_eq!(st.environment, StarOrgEnv::Dev);
    assert!(!st.connected);
    assert_eq!(m.state().revoked, 1, "the production token was revoked");
    // Switching back finds no stale production sign-in.
    let mut cfg = starorg_core::config::Config::load(&f.dirs).unwrap();
    cfg.sync.environment = starorg_core::starorg::Environment::Production;
    cfg.save(&f.dirs).unwrap();
    let st = f.backend.starorg_status("LIVE".into()).unwrap();
    assert!(!st.connected && st.user.is_none(), "{st:?}");
}

#[test]
fn starorg_a_token_refused_on_the_first_call_is_not_saved_as_signed_in() {
    let m = starorg_server();
    let f = starorg_fake(&m);
    set_sync(&f, true, |_| {});
    m.state().dead_tokens = true;
    let e = f.backend.starorg_login("LIVE".into()).unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid, "{e:?}");
    let st = f.backend.starorg_status("LIVE".into()).unwrap();
    assert!(!st.connected && st.token_store.is_none(), "{st:?}");
    let app_json = std::fs::read_to_string(f.dirs.config.join("app.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&app_json).unwrap();
    assert_ne!(
        v["starorg"]["production"]["signed_in"],
        serde_json::json!(true),
        "{app_json}"
    );
}

#[test]
fn game_log_import_off_reads_no_log() {
    let f = Fake::new();
    f.setup_live();
    let mut s = f.backend.get_settings().unwrap();
    assert!(s.starorg.import_game_log, "on by default");
    s.starorg.import_game_log = false;
    f.backend.save_settings(s).unwrap();
    let chars = f.backend.list_characters(f.live()).unwrap();
    assert!(chars.characters.is_empty(), "{chars:?}");
    let scan = f.dirs.data.join("channels");
    let has_scan = walk(&scan).iter().any(|p| p.ends_with("scan.json"));
    assert!(!has_scan, "no scan state written");
    let mut s = f.backend.get_settings().unwrap();
    s.starorg.import_game_log = true;
    f.backend.save_settings(s).unwrap();
    let chars = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(chars.characters[0].name, "Test_Pilot");
}

fn walk(d: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn starorg_auto_sync_runs_after_exit_and_on_the_hook_note() {
    let m = starorg_server();
    let f = starorg_fake(&m);
    set_sync(&f, true, |s| s.push_blueprints = true);
    f.backend.starorg_login("LIVE".into()).unwrap();
    // Auto off: nothing.
    assert!(f.backend.auto_tick_with(&|_, _, _| false).is_empty());
    set_sync(&f, true, |s| {
        s.push_blueprints = true;
        s.auto_sync = true;
    });
    let before = m.requests().len();
    assert!(
        f.backend.auto_tick_with(&|_, _, _| true).is_empty(),
        "running: wait"
    );
    let runs = f.backend.auto_tick_with(&|_, _, _| false);
    assert_eq!(runs.len(), 1, "running -> stopped syncs once");
    assert_eq!(runs[0].channel, "LIVE");
    assert_eq!(runs[0].reports[0].sent, 1);
    assert!(m.requests().len() > before);
    assert!(f.backend.auto_tick_with(&|_, _, _| false).is_empty());
    // The post-exit hook's note.
    starorg_core::starorg::mark_pending(&f.dirs, &["LIVE".into()]).unwrap();
    assert_eq!(f.backend.auto_tick_with(&|_, _, _| false).len(), 1);
    assert!(starorg_core::starorg::read_pending(&f.dirs).is_empty());
}

#[test]
fn diagnostics_replace_character_names_and_ids() {
    let f = Fake::new();
    f.setup_live();
    // Reading characters scans the log into the store.
    let chars = f.backend.list_characters(f.live()).unwrap();
    assert_eq!(chars.characters[0].name, "Test_Pilot");
    f.backend
        .set_active_character(SetActiveCharacter {
            target: f.live(),
            mode: CharacterMode::Manual,
            character: Some("300003".into()),
        })
        .unwrap();
    let d = f.backend.export_diagnostics().unwrap();
    assert!(!d.text.contains("Test_Pilot"));
    assert!(!d.text.contains("300003"));
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        assert!(home.len() < 2 || !d.text.contains(home.as_ref()));
    }
    assert!(d.file_name.ends_with(".txt"));
    // No dialog answer: save is cancelled, nothing written.
    assert_eq!(f.backend.save_diagnostics().unwrap(), None);
}

/// What the real backend shows for this machine: overview and doctor only, which read and
/// never write (no apply, no recovery, no registration). Needs a real install.
#[test]
#[ignore]
fn print_real_overview_read_only() {
    let b = RealBackend::new(RealConfig::from_env().unwrap());
    let o = b.overview().unwrap();
    for i in &o.installs {
        println!("{} {:?} saved={}", i.root, i.found_by, i.saved);
        for c in &i.channels {
            println!(
                "  {} {:?} {:?} set_up={} cache={:?}",
                c.channel, c.build.version, c.status, c.set_up, c.cache
            );
        }
    }
    let d = b.doctor().unwrap();
    println!("doctor: {} sections", d.sections.len());
}

#[test]
fn setup_again_keeps_a_set_up_channels_profile() {
    let f = Fake::new();
    f.setup_live();
    let data = f.dirs.channel_dir(&f.install_id(), "LIVE");
    let stamp = |d: &std::path::Path| {
        starorg_core::state::ChannelState::load(d)
            .unwrap()
            .setup_time
    };
    let first = stamp(&data);
    assert!(first.is_some());
    let mut p = f.backend.get_profile(f.live()).unwrap();
    p.header = "MINE".into();
    p.ships = false;
    f.backend
        .save_profile(SaveProfileRequest {
            target: f.live(),
            profile: p,
        })
        .unwrap();
    // The wizard again (Settings "Run setup again"), LIVE and PTU ticked, a different preset.
    let both = |preset| SetupRequest {
        install: f.install_id(),
        channels: ["LIVE", "PTU"]
            .map(|c| ChannelConsent {
                channel: c.into(),
                consent: true,
                switch_language: false,
            })
            .to_vec(),
        data_source: DataSource::Local,
        preset,
        fix_known_bugs: false,
    };
    let r = f.backend.setup(both(Preset::Minimal)).unwrap();
    assert_eq!(r.set_up, ["LIVE", "PTU"]);
    let after = f.backend.get_profile(f.live()).unwrap();
    assert_eq!(after.header, "MINE");
    assert!(!after.ships);
    assert!(after.patches_disabled.is_empty(), "{after:?}");
    assert_eq!(stamp(&data), first, "setup time kept");
    // PTU was new: it gets the chosen preset with fixes off.
    let ptu = f
        .backend
        .get_profile(ChannelRef {
            install: f.install_id(),
            channel: "PTU".into(),
        })
        .unwrap();
    assert_eq!(ptu.preset, Some(Preset::Minimal));
    assert!(!ptu.patches_disabled.is_empty());
}

#[test]
fn unbuilt_network_switches_stay_off() {
    let f = Fake::new();
    let mut s = f.backend.get_settings().unwrap();
    s.network.check_updates = true;
    s.network.published_dumps = true;
    s.network.starorg = true;
    let saved = f.backend.save_settings(s).unwrap();
    // check_updates is built (docs/updates.md) and stored; published data is not.
    assert!(saved.network.check_updates);
    assert!(f.backend.get_settings().unwrap().network.check_updates);
    assert!(!saved.network.published_dumps);
    assert!(saved.network.starorg);
    let req = SetupRequest {
        install: f.install_id(),
        channels: vec![ChannelConsent {
            channel: "LIVE".into(),
            consent: true,
            switch_language: false,
        }],
        data_source: DataSource::PublishedDumps,
        preset: Preset::Recommended,
        fix_known_bugs: true,
    };
    f.backend.setup(req).unwrap();
    assert!(!f.backend.get_settings().unwrap().network.published_dumps);
}

#[test]
fn unknown_patch_id_is_refused_and_not_saved() {
    let f = Fake::new();
    f.setup_live();
    let e = f
        .backend
        .set_patch_enabled(PatchToggle {
            target: f.live(),
            id: "user-x".into(),
            enabled: false,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFound);
    assert!(
        f.backend
            .get_profile(f.live())
            .unwrap()
            .patches_disabled
            .is_empty()
    );
}

fn profile_has_table(f: &Fake) -> bool {
    std::fs::read_to_string(
        f.dirs
            .channel_dir(&f.install_id(), "LIVE")
            .join("profile.toml"),
    )
    .unwrap()
    .contains("[ship_names]")
}

const FLEET_EXPORT: &str = r#"[
 {"ship_code":"SYN_Alpha","ship_name":"Needle","manufacturer_code":"SYN","name":"Synthetic Alpha","entity_type":"ship","pledge_id":"90000001","pledge_cost":"$1.00 USD"},
 {"ship_code":"SYN_Bravo","ship_name":"Bravo","manufacturer_code":"SYN","name":"Bravo","entity_type":"ship"},
 {"ship_code":"SYN_Zulu","ship_name":"Zulu","manufacturer_code":"SYN","name":"Zulu","entity_type":"ship"}
]"#;

#[test]
fn fleet_import_order_and_ship_names_through_apply() {
    if elevated() {
        return;
    }
    let f = Fake::new();
    f.setup_live();
    let view = |read| {
        f.backend
            .fleet_view(FleetViewRequest {
                target: f.live(),
                read_game_data: read,
            })
            .unwrap()
    };
    let v = view(true);
    assert!(v.catalog_ready);
    let ids: Vec<&str> = v.catalog.iter().map(|h| h.hull.as_str()).collect();
    assert_eq!(ids, ["vehicle_NameSYN_Alpha", "vehicle_NameSYN_Bravo"]);
    assert_eq!(v.catalog[0].npc_shared, 1);
    assert_eq!(v.scope, ShipNamesScope::Shared);
    assert!(!v.settings.enabled);

    // Dry run from the picker writes nothing; the confirm step reuses the picked file.
    let export = f.base.join("fleet.json");
    std::fs::write(&export, FLEET_EXPORT).unwrap();
    *f.pick.lock().unwrap() = Some(export);
    let req = |source, dry_run| FleetImportRequest {
        target: f.live(),
        source,
        dry_run,
        merge: false,
    };
    let r = f
        .backend
        .fleet_import(req(FleetSource::File, true))
        .unwrap();
    assert_eq!(r.file.as_deref(), Some("fleet.json"));
    assert_eq!(r.format, FleetFormat::XplorJson);
    assert_eq!(r.hulls.len(), 2);
    assert_eq!(r.hulls[0].names, ["Needle"]);
    assert_eq!(r.unmatched[0].name, "Zulu");
    assert_eq!(r.added_to_order, 0);
    let fleet_json = f.dirs.data.join("fleet.json");
    let names_toml = f.dirs.data.join("ship-names.toml");
    assert!(!fleet_json.exists() && !names_toml.exists());
    *f.pick.lock().unwrap() = None;
    let r = f
        .backend
        .fleet_import(req(FleetSource::Last, false))
        .unwrap();
    assert_eq!(r.added_to_order, 2);
    let stored = std::fs::read_to_string(&fleet_json).unwrap();
    assert!(
        !stored.contains("90000001") && !stored.contains("USD"),
        "{stored}"
    );
    let v = view(false);
    assert_eq!(v.owned.len(), 2);
    assert_eq!(v.pending[0].name, "Zulu");
    assert_eq!(v.settings.ships.len(), 2);

    // Turn it on with a rename; apply writes prefix + name; reordering makes it stale.
    let mut s = v.settings.clone();
    s.enabled = true;
    s.ships[1].name = Some("Needle".into());
    let save = |s: &ShipNamesSettings, scope| {
        f.backend
            .save_ship_names(SaveShipNamesRequest {
                target: f.live(),
                scope,
                settings: s.clone(),
            })
            .unwrap()
    };
    let v = save(&s, ShipNamesScope::Shared);
    assert!(v.settings.enabled);
    let (r, _) = f.apply(false);
    assert_eq!(r.result, ApplyResult::Applied, "{r:?}");
    let ini = std::fs::read_to_string(f.ini("LIVE")).unwrap();
    assert!(
        ini.contains("vehicle_NameSYN_Alpha=01 Synthetic Alpha\r\n"),
        "{ini}"
    );
    assert!(
        ini.contains("vehicle_NameSYN_Bravo,P=02 Needle\r\n"),
        "{ini}"
    );
    s.ships.reverse();
    save(&s, ShipNamesScope::Shared);
    match f.backend.channel_status(f.live()).unwrap().status {
        ApplyState::Stale { reasons } => assert_eq!(reasons, ["ship names"]),
        other => panic!("{other:?}"),
    }

    // Channel scope: the list lives in the channel's profile; the shared file is kept.
    let mut own = s.clone();
    own.style = NameStyle::Symbol;
    let v = save(&own, ShipNamesScope::Channel);
    assert_eq!(v.scope, ShipNamesScope::Channel);
    assert!(
        v.settings_file.ends_with("profile.toml"),
        "{}",
        v.settings_file
    );
    let profile = std::fs::read_to_string(
        f.dirs
            .channel_dir(&f.install_id(), "LIVE")
            .join("profile.toml"),
    )
    .unwrap();
    assert!(
        profile.contains("use = \"channel\"") && profile.contains("style = \"symbol\""),
        "{profile}"
    );
    assert!(
        std::fs::read_to_string(&names_toml)
            .unwrap()
            .contains("enabled = true")
    );
    assert_eq!(v.shared_settings.style, NameStyle::Numeric);
    let v = save(&v.shared_settings, ShipNamesScope::Shared);
    assert_eq!(
        v.settings.style,
        NameStyle::Numeric,
        "back to the shared list"
    );
    assert!(!profile_has_table(&f), "the channel's own list is dropped");

    // A hand-broken shared file is moved aside before a save, not overwritten.
    let broken = "style = \"roman\"\n# notes kept\n";
    std::fs::write(&names_toml, broken).unwrap();
    save(&s, ShipNamesScope::Shared);
    let kept: Vec<String> = std::fs::read_dir(names_toml.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("ship-names.toml.bad-"))
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(
        std::fs::read_to_string(names_toml.with_file_name(&kept[0])).unwrap(),
        broken
    );
    assert!(
        std::fs::read_to_string(&names_toml)
            .unwrap()
            .contains("enabled = true")
    );

    // Hand match and an override that wins.
    let v = f
        .backend
        .fleet_match(FleetMatchRequest {
            target: f.live(),
            name: "Zulu".into(),
            hull: None,
        })
        .unwrap();
    assert!(v.pending.is_empty());
    assert_eq!(v.not_in_game, ["zulu"]);
    f.backend
        .set_override(OverrideRequest {
            target: f.live(),
            key: "vehicle_NameSYN_Alpha".into(),
            value: "My Alpha".into(),
        })
        .unwrap();
    let v = view(false);
    assert_eq!(v.overrides.len(), 1);
    assert_eq!(v.overrides[0].long.as_deref(), Some("My Alpha"));
    let bad = f.backend.fleet_match(FleetMatchRequest {
        target: f.live(),
        name: "Zulu".into(),
        hull: Some("vehicle_NameNope".into()),
    });
    assert_eq!(bad.unwrap_err().code, ErrorCode::Invalid);
    let v = f.backend.fleet_clear(f.live()).unwrap();
    assert!(v.owned.is_empty() && !v.settings.ships.is_empty());
}

/// GitHub's release list, scripted; counts requests.
#[derive(Default)]
struct FakeGitHub {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl ReleaseFetch for FakeGitHub {
    fn fetch(&self, repo: &str) -> Result<Reply, String> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(repo, "voidput/starorg-codex");
        Ok(Reply {
            status: 200,
            rate_limit_remaining: Some(59),
            body: br#"[{"tag_name":"v999.0.0","name":"Future","body":"* <b>new</b>","draft":false,"prerelease":false}]"#.to_vec(),
        })
    }
}

#[test]
fn update_check_asks_only_with_the_switch_on_and_keeps_its_state() {
    let Fake {
        _t, dirs, backend, ..
    } = Fake::new();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let backend = backend.with_updater(Updater::with_fetch(
        dirs.data.join(UPDATE_JSON),
        Box::new(FakeGitHub {
            calls: calls.clone(),
        }),
    ));
    let n = || calls.load(std::sync::atomic::Ordering::SeqCst);

    // Off: the start-up check is a no-op and "Check now" refuses.
    let st = backend
        .check_for_updates(UpdateCheckRequest { force: false })
        .unwrap();
    assert!(!st.enabled);
    assert_eq!(st.check, UpdateCheck::NotChecked);
    assert_eq!(
        backend
            .check_for_updates(UpdateCheckRequest { force: true })
            .unwrap_err()
            .code,
        ErrorCode::NetworkOff
    );
    assert_eq!(n(), 0);
    assert!(!dirs.data.join(UPDATE_JSON).exists());

    let mut s = backend.get_settings().unwrap();
    s.network.check_updates = true;
    backend.save_settings(s).unwrap();
    let st = backend
        .check_for_updates(UpdateCheckRequest { force: false })
        .unwrap();
    let UpdateCheck::Available { release } = &st.check else {
        panic!("{:?}", st.check)
    };
    assert_eq!(release.version, "999.0.0");
    assert_eq!(release.notes[0].text, "new");
    assert_eq!(st.current_version, env!("CARGO_PKG_VERSION"));
    // Tests are not a bundle: download only.
    assert_eq!(st.install, UpdateInstall::DownloadOnly);
    assert_eq!(n(), 1);
    // Within 24 h: no second request; the state is a file in the data folder.
    backend
        .check_for_updates(UpdateCheckRequest { force: false })
        .unwrap();
    assert_eq!(n(), 1);
    assert!(dirs.data.join(UPDATE_JSON).is_file());
    // Download-only installs refuse in-app install with their reason; no window, no browser.
    assert_eq!(
        backend.install_update().unwrap_err().code,
        ErrorCode::Invalid
    );
    assert_eq!(
        backend.open_release_page().unwrap_err().code,
        ErrorCode::NotImplemented
    );
    assert!(
        backend
            .activity(20)
            .unwrap()
            .iter()
            .any(|e| e.message == "Update check: 999.0.0 is available")
    );
}

/// Settings -> Experimental -> Local MCP server, on the real backend: off at start, then a
/// loopback server for the fake install's channels that needs its token, then off again.
#[test]
fn local_mcp_server_is_off_until_switched_on_and_needs_its_token() {
    use std::io::{Read, Write};
    let f = Fake::new();
    let off = f.backend.mcp_status().unwrap();
    assert!(!off.running && off.url.is_none() && off.token.is_none());

    let on = f
        .backend
        .set_mcp_enabled(McpToggle { enabled: true })
        .unwrap();
    assert!(on.running, "{:?}", on.error);
    assert_eq!(on.channels, ["LIVE", "PTU"]);
    let url = on.url.clone().unwrap();
    let token = on.token.clone().unwrap();
    let addr = url
        .trim_start_matches("http://")
        .trim_end_matches("/mcp")
        .to_string();
    assert!(addr.starts_with("127.0.0.1:"));
    let ask = |auth: Option<&str>| -> (u16, String) {
        let mut s = std::net::TcpStream::connect(&addr).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status","arguments":{}}}"#;
        let a = auth
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        write!(
            s,
            "POST /mcp HTTP/1.1\r\n{a}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let status = out.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, out)
    };
    assert_eq!(ask(None).0, 401);
    let (status, out) = ask(Some(&token));
    assert_eq!(status, 200);
    assert!(out.contains("LIVE") && out.contains("PTU"));
    // No dataset was extracted to answer `status`.
    assert!(!f.dirs.cache.join("mcp").join("LIVE").exists());
    let st = f.backend.mcp_status().unwrap();
    assert_eq!(st.requests.len(), 2);
    assert!(
        f.backend
            .activity(10)
            .unwrap()
            .iter()
            .any(|e| e.job == Some(JobKind::Mcp))
    );
    let off = f
        .backend
        .set_mcp_enabled(McpToggle { enabled: false })
        .unwrap();
    assert!(!off.running);
    assert!(std::net::TcpStream::connect(&addr).is_err());
}

/// Strings owners on the user's real LIVE install, read only (the game folder is only read;
/// the cache entry must already exist). Prints time and memory of the first Strings query
/// (game data load, groups, owners, table) and of a warm one, and a few real rows. Run with
/// `--ignored --nocapture`.
#[test]
#[ignore]
fn measure_strings_owners_on_real_live() {
    fn status(field: &str) -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with(field))
                    .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
            })
            .unwrap_or(0)
    }
    let b = RealBackend::new(RealConfig::from_env().unwrap());
    let o = b.overview().unwrap();
    let (install, _) = o
        .installs
        .iter()
        .find_map(|i| {
            i.channels
                .iter()
                .find(|c| c.channel == "LIVE")
                .map(|c| (i.id.clone(), c.clone()))
        })
        .expect("a LIVE channel");
    let target = ChannelRef {
        install,
        channel: "LIVE".into(),
    };
    let q = |search: &str, sort: StringSort| StringsQuery {
        target: target.clone(),
        search: search.into(),
        sources: vec![],
        categories: vec![],
        changed_only: false,
        sort,
        offset: 0,
        limit: 200,
        groups: vec![],
        manufacturers: vec![],
        sizes: vec![],
        group_by: GroupBy::Type,
    };
    let rss0 = status("VmRSS:");
    let t = std::time::Instant::now();
    let first = b.query_strings(q("", StringSort::Group)).unwrap();
    let cold = t.elapsed();
    let rss1 = status("VmRSS:");
    let t = std::time::Instant::now();
    b.query_strings(q("", StringSort::Key)).unwrap();
    let warm_key = t.elapsed();
    let t = std::time::Instant::now();
    let by_owner = b.query_strings(q("", StringSort::Owner)).unwrap();
    let warm_sort = t.elapsed();
    let t = std::time::Instant::now();
    let gladius = b.query_strings(q("gladius", StringSort::Owner)).unwrap();
    let warm_search = t.elapsed();
    println!(
        "keys {}: cold query {cold:?} (RSS {} -> {} MiB, peak {} MiB), key sort {warm_key:?}, owner sort {warm_sort:?}, search {warm_search:?}",
        first.all,
        rss0 / 1024,
        rss1 / 1024,
        status("VmHWM:") / 1024
    );
    println!(
        "first by owner: {:?}",
        by_owner.rows.first().map(|r| (&r.key, &r.owner))
    );
    println!("gladius: {} rows", gladius.total);
    for key in [
        "item_DescCOOL_AEGS_S01_Bracer",
        "vehicle_NameAEGS_Gladius",
        "vehicle_focus_combat",
        "manufacturer_DescAEGS",
        "Pyro1_L2_03_desc",
        "item_Desc_fta_medium_helmet_01_04_10",
        "hud_law_TrespassAreaRevokedAuthorisation",
        "item_NameBEHR_LaserCannon_S1",
        "item_NameGMISL_S03_EM_FSKI_Thunderbolt",
        "item_DescGMNI_rifle_ballistic_01_grey_red01",
        "item_Descgmni_rifle_ballistic_01_mag",
    ] {
        let p = b.query_strings(q(key, StringSort::Key)).unwrap();
        if let Some(r) = p.rows.iter().find(|r| r.key == key) {
            let o = r.owner.as_deref().unwrap();
            println!(
                "{key} -> {} | {:?} | {:?} | {:?} | {} owners | group {}",
                o.name, o.field, o.kind, o.detail, r.owners, r.group_label
            );
        }
    }
}

/// The start-up folder move (0.3.0 rename) is told in Activity: one line per outcome.
#[test]
fn folder_migration_is_one_activity_line_per_outcome() {
    use starorg_core::legacy::{Event, Report};
    let f = Fake::new();
    let r = Report {
        events: vec![
            Event::Moved {
                old: "/h/.config/starorg-strings".into(),
                new: "/h/.config/starorg-codex".into(),
            },
            Event::Moved {
                old: "/h/.local/share/starorg-strings".into(),
                new: "/h/.local/share/starorg-codex".into(),
            },
            Event::BothExist {
                old: "/h/.cache/starorg-strings".into(),
                new: "/h/.cache/starorg-codex".into(),
            },
        ],
    };
    f.backend.log_folder_migration(&r);
    let log = f.backend.activity(10).unwrap();
    let moved: Vec<_> = log
        .iter()
        .filter(|e| e.message.starts_with("App folders moved"))
        .collect();
    assert_eq!(moved.len(), 1, "{log:?}");
    assert_eq!(moved[0].level, LogLevel::Info);
    assert!(moved[0].message.contains("/h/.local/share/starorg-codex"));
    assert!(log.iter().any(
        |e| e.message.contains("left alone") && e.message.contains("/h/.cache/starorg-strings")
    ));
    f.backend.log_folder_migration(&Report {
        events: vec![Event::Failed {
            old: "/h/x-old".into(),
            new: "/h/x-new".into(),
            error: "rename: busy".into(),
        }],
    });
    let log = f.backend.activity(10).unwrap();
    assert!(
        log.iter()
            .any(|e| e.level == LogLevel::Warn && e.message.contains("rename: busy")),
        "{log:?}"
    );
}

/// A schema-1 `profile.toml` is rewritten once when the channel is listed: one Activity
/// line that names only the setting whose value changed, a backup, a sparse file. A file
/// that cannot be read warns once, not on every refresh.
#[test]
fn schema1_profile_migrates_once_with_one_activity_line() {
    let f = Fake::new();
    f.setup_live();
    let data = f.dirs.channel_dir(&f.install_id(), "LIVE");
    let path = data.join("profile.toml");
    let old = "preset = \"recommended\"\nschema = 1\n[enhance]\nbranding = true\nships = true\ncolour_test = false\n[enhance.mission]\nmax_list = 25\nowned_em = 0\ntitle_em = true\ntitle_rep_em = 2\n[enhance.tags.ship_weapons.styles]\ndamage = \"short\"\n[patches]\ndisabled = []\n";
    std::fs::write(&path, old).unwrap();
    let lines = |needle: &str| -> Vec<String> {
        f.backend
            .activity(200)
            .unwrap()
            .into_iter()
            .map(|e| e.message)
            .filter(|m| m.contains(needle))
            .collect()
    };
    f.backend.overview().unwrap();
    f.backend.overview().unwrap();
    let got = lines("Profile updated");
    assert_eq!(
        got,
        [
            "Profile updated: owned_em now follows the current default (it was saved at the old one); removed 5 saved defaults (no change); a backup is in profile.toml.bak-schema1"
        ],
    );
    assert_eq!(
        std::fs::read_to_string(data.join("profile.toml.bak-schema1")).unwrap(),
        old
    );
    let now: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let expect: toml::Table =
        toml::from_str("branding = true\n[mission]\ntitle_rep_em = 2\n").unwrap();
    assert_eq!(now["schema"].as_integer(), Some(3));
    assert_eq!(now["enhance"].as_table().unwrap(), &expect);
    let p = f.backend.get_profile(f.live()).unwrap();
    assert_eq!((p.mission.owned_em, p.mission.title_rep_em), (4, 2));
    assert!(p.branding);

    // Unreadable: one warning however often the channel is listed.
    std::fs::write(&path, "schema = 1\n[enhance\n").unwrap();
    f.backend.overview().unwrap();
    f.backend.overview().unwrap();
    assert_eq!(lines("could not be updated to the new format").len(), 1);
    assert_eq!(lines("Profile updated").len(), 1);
}
