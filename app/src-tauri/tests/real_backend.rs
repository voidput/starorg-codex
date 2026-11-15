//! The real backend against a synthetic fake install in a temp folder (issue #15).
//!
//! The tree is shaped like an install inside a Wine prefix (as in
//! `crates/starorg-cli/tests/common`), without symlinks so it runs on Windows too. The stock
//! text and GameData come from a test `SourceFactory`, so no Data.p4k is read and no worker
//! runs. App folders are under the temp folder; discovery is pinned to the fake install.
//! Names, ids and text are invented.

use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{Channel, DiscoveryEnv, DiscoveryRequest};
use starorg_core::model::{Blueprint, GameData};
use starorg_core::source::{Budget, GameSource, Loaded, Origin, SourceError};
use starorg_net::releases::Reply;
use starorg_strings_app::contract::Backend;
use starorg_strings_app::contract::real::UPDATE_JSON;
use starorg_strings_app::contract::real::{
    DialogSlot, Dialogs, FileFilter, RealBackend, RealConfig, SourceFactory,
};
use starorg_strings_app::contract::types::*;
use starorg_strings_app::update::{ReleaseFetch, Updater};
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
struct TestSource;

impl GameSource for TestSource {
    fn load(&self, _: &Channel, _: &Budget) -> Result<Loaded, SourceError> {
        Ok(Loaded {
            base_ini: stock_bytes(),
            game_data: GameData {
                blueprints: vec![Blueprint {
                    record_name: "CraftingBlueprintRecord.bp_fake_rifle".into(),
                    id: "bp-fake-rifle".into(),
                    produces_name_key: Some("item_NameRifle".into()),
                    ..Blueprint::default()
                }],
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

struct TestSources;

impl SourceFactory for TestSources {
    fn source(&self, _: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        Ok(Box::new(TestSource))
    }
}

struct Fake {
    _t: tempfile::TempDir,
    base: PathBuf,
    lib: PathBuf,
    dirs: AppDirs,
    backend: RealBackend,
    pick: Arc<Mutex<Option<PathBuf>>>,
}

fn manifest(branch: &str, change: u64, version: &str) -> String {
    format!(
        r#"{{"Data":{{"Branch":"{branch}","RequestedP4ChangeNum":"{change}","Version":"{version}"}}}}"#
    )
}

const LOGIN: &str = "<2026-03-02T10:00:00.000Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 300003 - accountId 900 - name Test_Pilot - state STATE_CURRENT [Team_GameServices][Login]\n";

impl Fake {
    fn new() -> Self {
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
        slot.set(SharedPick(pick.clone()));
        let backend = RealBackend::new(RealConfig {
            ctx,
            discovery: DiscoveryRequest {
                game_dir: Some(lib.clone()),
                prefix: None,
            },
            env: DiscoveryEnv::from_process(),
            source: Arc::new(TestSources),
            dialogs: slot,
        })
        // Never the network in tests: the update check gets a scripted GitHub.
        .with_updater(Updater::with_fetch(
            dirs.data.join(UPDATE_JSON),
            Box::new(FakeGitHub::default()),
        ));
        Self {
            _t: t,
            base,
            lib,
            dirs,
            backend,
            pick,
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
struct SharedPick(Arc<Mutex<Option<PathBuf>>>);

impl Dialogs for SharedPick {
    fn pick_folder(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn pick_file(&self, _: &str, _: FileFilter) -> Option<PathBuf> {
        self.0.lock().unwrap().clone()
    }
    fn save_file(&self, _: &str, _: &str, _: FileFilter) -> Option<PathBuf> {
        None
    }
}

/// Apply refuses to run elevated (spec 11); Windows CI runners are administrators.
fn elevated() -> bool {
    if starorg_core::fsutil::is_elevated() {
        eprintln!("running elevated: apply refuses by design; test skipped");
        return true;
    }
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
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].key, "item_NameA");
    // Groups (addendum 8): the app-started extraction ran the loc-reference pass.
    assert_eq!(page.group_state, GroupState::Exact);
    assert_eq!(page.rows[0].group, "items/misc");
    let rifle = f
        .backend
        .query_strings(StringsQuery {
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
        })
        .unwrap();
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
            .contains("another StarOrg Strings run"),
        "{r:?}"
    );
    assert!(!f.ini("LIVE").exists());
    assert_eq!(events.last().unwrap().phase, JobPhase::Done);
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
}

#[test]
fn starorg_sync_is_refused_not_faked() {
    let f = Fake::new();
    let e = f.backend.starorg_push("LIVE".into()).unwrap_err();
    assert_eq!(e.code, ErrorCode::NetworkOff);
    let mut s = f.backend.get_settings().unwrap();
    s.network.starorg = true;
    f.backend.save_settings(s).unwrap();
    let e = f.backend.starorg_login("LIVE".into()).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotImplemented);
    assert!(!f.backend.starorg_status("LIVE".into()).unwrap().connected);
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

/// GitHub's release list, scripted; counts requests.
#[derive(Default)]
struct FakeGitHub {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl ReleaseFetch for FakeGitHub {
    fn fetch(&self, repo: &str) -> Result<Reply, String> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(repo, "voidput/starorg-strings");
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
