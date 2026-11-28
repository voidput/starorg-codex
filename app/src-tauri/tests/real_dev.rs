//! Developer mode on the real backend (addendum 7.11): the real `dev-worker` process (this
//! package's binary) on a synthetic DCB built with StarBreaker's DcbBuilder, a fake install
//! in a temp folder, and native dialogs replaced by fixed answers. Read only; nothing here
//! touches a game install.

use starorg_codex_app::contract::Backend;
use starorg_codex_app::contract::dev::*;
use starorg_codex_app::contract::real::{
    DevSpawner, DialogSlot, Dialogs, FileFilter, RealBackend, RealConfig, SourceFactory,
};
use starorg_codex_app::contract::types::*;
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{Channel, DiscoveryEnv, DiscoveryRequest};
use starorg_core::source::dump::{DumpTransport, ReleaseInfo, TreeEntry, git_blob_sha1};
use starorg_core::source::{GameSource, SourceError};
use starorg_data::starbreaker::synthetic;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct NoSource;
impl SourceFactory for NoSource {
    fn source(&self, _: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        Err("no extraction in these tests".into())
    }
}

#[derive(Default)]
struct Answers {
    folder: Option<PathBuf>,
    save: Option<PathBuf>,
}

struct FixedDialogs(Arc<Mutex<Answers>>);
impl Dialogs for FixedDialogs {
    fn pick_folder(&self, _: &str) -> Option<PathBuf> {
        self.0.lock().unwrap().folder.clone()
    }
    fn pick_file(&self, _: &str, _: FileFilter) -> Option<PathBuf> {
        None
    }
    fn save_file(&self, _: &str, _: &str, _: FileFilter) -> Option<PathBuf> {
        self.0.lock().unwrap().save.clone()
    }
}

/// The app binary as `dev-worker --dcb <synthetic> --ini <synthetic>`.
struct Synthetic {
    dcb: PathBuf,
    ini: PathBuf,
    starts: Arc<Mutex<u32>>,
}
impl DevSpawner for Synthetic {
    fn command(&self, _: &Channel) -> Result<Command, String> {
        *self.starts.lock().unwrap() += 1;
        let mut c = Command::new(env!("CARGO_BIN_EXE_starorg-codex-app"));
        c.arg("dev-worker")
            .arg("--dcb")
            .arg(&self.dcb)
            .arg("--ini")
            .arg(&self.ini);
        Ok(c)
    }
}

/// A worker that cannot start (its DCB is missing).
struct Broken;
impl DevSpawner for Broken {
    fn command(&self, _: &Channel) -> Result<Command, String> {
        let mut c = Command::new(env!("CARGO_BIN_EXE_starorg-codex-app"));
        c.args(["dev-worker", "--dcb"])
            .arg(Path::new("/nonexistent/Game2.dcb"));
        Ok(c)
    }
}

/// The dumps repo in memory: two LIVE builds and a PTU one; counts every request.
struct FakeDumps {
    calls: AtomicUsize,
    tamper: Mutex<Option<String>>,
    /// After this many file downloads, turn the switch off in app.json (a user flipping it
    /// while a download runs).
    off_after: Mutex<Option<(usize, PathBuf)>>,
    files_served: AtomicUsize,
}

const OLD: &str = "4.10.197.2878";
const NEW: &str = "4.10.200.58746";

fn dump_body(tag: &str) -> String {
    let (n, w2) = if tag == NEW { (3, 25) } else { (2, 20) };
    let rows: Vec<String> = (1..=n)
        .map(|i| {
            let dps = if i == 2 { w2 } else { 10 * i };
            format!(r#"{{"name":"W{i}","dps":{dps}}}"#)
        })
        .collect();
    format!(
        r#"{{"_source":"syn","_extracted":"{tag}","_build":"{tag}","shipWeapons":[{}]}}"#,
        rows.join(",")
    )
}

fn commit_of(tag: &str) -> String {
    format!("{:0>40}", tag.replace('.', ""))
}

impl FakeDumps {
    fn files(&self, tag: &str) -> BTreeMap<String, Vec<u8>> {
        let mut m = BTreeMap::new();
        m.insert(
            "data/game-weapons.json".to_string(),
            dump_body(tag).into_bytes(),
        );
        m.insert(
            "data/game-law.json".to_string(),
            format!(r#"{{"_build":"{tag}","laws":[{{"id":"a"}}]}}"#).into_bytes(),
        );
        m.insert("README.md".to_string(), b"readme".to_vec());
        m
    }
    fn tag_of(commit: &str) -> &'static str {
        if commit == commit_of(NEW) { NEW } else { OLD }
    }
}

impl DumpTransport for FakeDumps {
    fn releases(&self) -> Result<Vec<ReleaseInfo>, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let r = |tag: &str, title: &str, ch: &str, date: &str| ReleaseInfo {
            tag: tag.into(),
            title: title.into(),
            body: format!("- Channel: {ch}\n- Branch: `sc-alpha-4.10.0`"),
            published: Some(date.into()),
        };
        Ok(vec![
            r(
                "5.0.201.1",
                "5.0 PTU (5.0.201.1)",
                "PTU",
                "2026-11-15T00:00:00Z",
            ),
            r(
                NEW,
                "4.10.0 LIVE (4.10.200.58746)",
                "LIVE",
                "2026-11-12T18:20:43Z",
            ),
            r(
                OLD,
                "4.10.2 LIVE (4.10.197.2878)",
                "LIVE",
                "2026-11-01T14:36:39Z",
            ),
        ])
    }
    fn release(&self, tag: &str) -> Result<Option<ReleaseInfo>, SourceError> {
        let all = self.releases()?;
        Ok(all.into_iter().find(|r| r.tag == tag))
    }
    fn tag_commit(&self, tag: &str) -> Result<String, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(commit_of(tag))
    }
    fn tree(&self, commit: &str) -> Result<Vec<TreeEntry>, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .files(Self::tag_of(commit))
            .into_iter()
            .map(|(p, b)| TreeEntry {
                sha: git_blob_sha1(&b),
                size: b.len() as u64,
                path: p,
            })
            .collect())
    }
    fn file(&self, commit: &str, path: &str, _: u64) -> Result<Vec<u8>, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let served = self.files_served.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some((n, app_json)) = self.off_after.lock().unwrap().clone()
            && served >= n
        {
            let mut v: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&app_json).unwrap()).unwrap();
            v["network"]["published_dumps"] = false.into();
            std::fs::write(&app_json, serde_json::to_vec(&v).unwrap()).unwrap();
        }
        let mut b = self.files(Self::tag_of(commit))[path].clone();
        if self.tamper.lock().unwrap().as_deref() == Some(path) {
            b[0] = b' ';
        }
        Ok(b)
    }
}

struct Fake {
    _t: tempfile::TempDir,
    base: PathBuf,
    backend: RealBackend,
    answers: Arc<Mutex<Answers>>,
    starts: Arc<Mutex<u32>>,
    dumps: Arc<FakeDumps>,
}

fn fake(broken: bool) -> Fake {
    let t = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(t.path()).unwrap();
    let lib = base.join("games/StarCitizen");
    let d = lib.join("LIVE");
    std::fs::create_dir_all(d.join("Data")).unwrap();
    std::fs::write(d.join("Data.p4k"), b"fake archive").unwrap();
    std::fs::write(
        d.join("build_manifest.id"),
        r#"{"Data":{"Branch":"sc-alpha-4.10.0","RequestedP4ChangeNum":"13165946","Version":"4.10.200.58746"}}"#,
    )
    .unwrap();
    let dcb = base.join("work/Game2.dcb");
    let ini = base.join("work/global.ini");
    std::fs::create_dir_all(dcb.parent().unwrap()).unwrap();
    std::fs::write(&dcb, synthetic::build(3, 600, 5)).unwrap();
    std::fs::write(&ini, b"item_NameSYN_Item_001=Synthetic Item One\r\n").unwrap();
    let dirs = AppDirs::under(&base.join("app"));
    let ctx = Ctx::new(dirs, "0.0.0-test", "test");
    let answers = Arc::new(Mutex::new(Answers::default()));
    let slot = DialogSlot::default();
    slot.set(FixedDialogs(answers.clone()));
    let starts = Arc::new(Mutex::new(0));
    let spawner: Arc<dyn DevSpawner> = if broken {
        Arc::new(Broken)
    } else {
        Arc::new(Synthetic {
            dcb,
            ini,
            starts: starts.clone(),
        })
    };
    let backend = RealBackend::new(RealConfig {
        ctx,
        discovery: DiscoveryRequest {
            game_dir: Some(lib),
            prefix: None,
        },
        env: DiscoveryEnv::from_process(),
        source: Arc::new(NoSource),
        dialogs: slot,
    })
    .with_dev_spawner(spawner);
    let dumps = Arc::new(FakeDumps {
        calls: AtomicUsize::new(0),
        tamper: Mutex::new(None),
        off_after: Mutex::new(None),
        files_served: AtomicUsize::new(0),
    });
    let backend = backend.with_dump_transport(dumps.clone());
    Fake {
        _t: t,
        base,
        backend,
        answers,
        starts,
        dumps,
    }
}

impl Fake {
    fn live(&self) -> ChannelRef {
        ChannelRef {
            install: self.backend.overview().unwrap().installs[0].id.clone(),
            channel: "LIVE".into(),
        }
    }

    fn dev_mode(&self, on: bool) {
        let mut s = self.backend.get_settings().unwrap();
        s.advanced.developer_mode = on;
        self.backend.save_settings(s).unwrap();
    }
}

fn noop(_: JobProgress) {}

#[test]
fn developer_mode_is_off_by_default_and_refuses_every_call() {
    let f = fake(false);
    assert!(!f.backend.get_settings().unwrap().advanced.developer_mode);
    let e = f.backend.dev_status(f.live()).unwrap_err();
    assert_eq!(e.code, ErrorCode::DevDisabled);
    assert_eq!(
        f.backend.dev_open(f.live(), &noop).unwrap_err().code,
        ErrorCode::DevDisabled
    );
    assert_eq!(*f.starts.lock().unwrap(), 0, "no worker started");
    // Older app.json without the field loads as off.
    f.dev_mode(true);
    assert!(f.backend.get_settings().unwrap().advanced.developer_mode);
}

#[test]
fn the_worker_starts_answers_exports_and_stops() {
    let f = fake(false);
    f.dev_mode(true);
    let live = f.live();
    assert_eq!(
        f.backend.dev_status(live.clone()).unwrap().state,
        WorkerState::Stopped
    );
    let seen = Mutex::new(Vec::new());
    let s = f
        .backend
        .dev_open(live.clone(), &|p| seen.lock().unwrap().push(p))
        .unwrap();
    assert_eq!(s.state, WorkerState::Ready);
    assert_eq!(s.records, 608);
    assert_eq!(s.main, 604);
    let phases: Vec<JobPhase> = seen.lock().unwrap().iter().map(|p| p.phase).collect();
    assert_eq!(phases.last(), Some(&JobPhase::Done));
    assert!(seen.lock().unwrap().iter().all(|p| p.kind == JobKind::Dev));
    // Tree, search, record with references and loc text.
    let types = f
        .backend
        .dev_tree(DevTreeRequest {
            fold_variants: false,
            player_only: false,
            target: live.clone(),
            root: TreeRoot::Type,
            node: String::new(),
            offset: 0,
            limit: 500,
            main_only: true,
        })
        .unwrap();
    assert_eq!(types.total, 3);
    let items = f
        .backend
        .dev_tree(DevTreeRequest {
            fold_variants: false,
            player_only: false,
            target: live.clone(),
            root: TreeRoot::Type,
            node: format!("t:{}", synthetic::ITEM_TYPE),
            offset: 500,
            limit: 500,
            main_only: true,
        })
        .unwrap();
    assert_eq!((items.total, items.nodes.len()), (600, 100));
    let hits = f
        .backend
        .dev_search(DevSearchRequest {
            target: live.clone(),
            query: "syn_item_001".into(),
            limit: 10,
        })
        .unwrap();
    assert_eq!(hits.hits[0].name, "SYN_Item_001");
    let r = f
        .backend
        .dev_record(DevRecordRequest {
            target: live.clone(),
            what: hits.hits[0].guid.clone(),
            pointer: String::new(),
            depth: None,
        })
        .unwrap();
    assert!(r.json.contains("\"_ref\""));
    assert_eq!(r.loc[0].text, "Synthetic Item One");
    // Export: cancelled dialog writes nothing; a chosen path gets the array.
    let req = DevExportTypeRequest {
        target: live.clone(),
        type_name: synthetic::ITEM_TYPE.into(),
        main_only: true,
    };
    assert!(
        f.backend
            .dev_export_type(req.clone(), &noop)
            .unwrap()
            .cancelled
    );
    let out = f.base.join("export/items.json");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    f.answers.lock().unwrap().save = Some(out.clone());
    let progress = Mutex::new(Vec::new());
    let done = f
        .backend
        .dev_export_type(req, &|p| progress.lock().unwrap().push(p))
        .unwrap();
    assert_eq!(done.records, 600);
    let arr: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(arr.len(), 600);
    assert!(progress.lock().unwrap().iter().any(|p| p.cancellable));
    let one = f.base.join("export/one.json");
    f.answers.lock().unwrap().save = Some(one.clone());
    let e = f
        .backend
        .dev_export_record(DevExportRecordRequest {
            target: live.clone(),
            what: "SYN_Item_002".into(),
        })
        .unwrap();
    assert_eq!(e.records, 1);
    assert!(one.is_file());
    // The same worker serves later calls; closing stops it, the next call starts a new one.
    assert_eq!(*f.starts.lock().unwrap(), 1);
    let s = f.backend.dev_close(live.clone()).unwrap();
    assert_eq!(s.state, WorkerState::Stopped);
    f.backend.dev_types(live.clone()).unwrap();
    assert_eq!(*f.starts.lock().unwrap(), 2);
    // Turning Developer mode off stops the worker and refuses calls.
    f.dev_mode(false);
    assert_eq!(
        f.backend.dev_types(live.clone()).unwrap_err().code,
        ErrorCode::DevDisabled
    );
    f.dev_mode(true);
    assert_eq!(
        f.backend.dev_status(live).unwrap().state,
        WorkerState::Stopped
    );
}

#[test]
fn a_worker_that_cannot_start_is_a_failed_status_not_a_crash() {
    let f = fake(true);
    f.dev_mode(true);
    let e = f.backend.dev_open(f.live(), &noop).unwrap_err();
    assert_eq!(e.code, ErrorCode::Internal);
    let s = f.backend.dev_status(f.live()).unwrap();
    assert_eq!(s.state, WorkerState::Failed);
    assert!(s.error.unwrap().contains("Game2.dcb"));
}

#[test]
fn folder_datasets_rows_and_diff() {
    let f = fake(false);
    f.dev_mode(true);
    let live = f.live();
    let a = f.base.join("data-a");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(
        a.join("game-weapons.json"),
        r#"{"_source":"syn","_extracted":"1","shipWeapons":[{"name":"W1","dps":10},{"name":"W2","dps":20}],"summary":{"shipWeapons":2}}"#,
    )
    .unwrap();
    std::fs::write(a.join("notes.txt"), "not json").unwrap();
    let b = f.base.join("data-b");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(
        b.join("game-weapons.json"),
        r#"{"_source":"syn","_extracted":"2","shipWeapons":[{"name":"W1","dps":12},{"name":"W3","dps":5}]}"#,
    )
    .unwrap();
    // A cancelled folder dialog adds nothing.
    let before = f.backend.dev_sources(live.clone()).unwrap();
    assert!(before.iter().all(|s| s.kind != SourceKind::Folder));
    let build = before.iter().find(|s| s.kind == SourceKind::Build).unwrap();
    assert_eq!(build.state, SourceState::Missing, "no extraction yet");
    assert!(before.iter().all(|s| s.kind != SourceKind::Dump));
    assert_eq!(
        f.backend.dev_add_source(live.clone()).unwrap().len(),
        before.len()
    );
    for dir in [&a, &b] {
        f.answers.lock().unwrap().folder = Some(dir.clone());
        f.backend.dev_add_source(live.clone()).unwrap();
    }
    let src = f.backend.dev_sources(live.clone()).unwrap();
    let folders: Vec<&DevSource> = src
        .iter()
        .filter(|s| s.kind == SourceKind::Folder)
        .collect();
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].datasets.len(), 1, "only *.json");
    assert_eq!(folders[0].datasets[0].rows, 2);
    let page = f
        .backend
        .dev_rows(DevRowsRequest {
            target: live.clone(),
            source: folders[0].id.clone(),
            dataset: "game-weapons.json".into(),
            table: "shipWeapons".into(),
            columns: vec![],
            search: "w2".into(),
            filters: vec![],
            sort: None,
            offset: 0,
            limit: 50,
            group: None,
        })
        .unwrap();
    assert_eq!(page.total, 1);
    let diff = f
        .backend
        .dev_diff(
            DevDiffRequest {
                target: live.clone(),
                a: folders[0].id.clone(),
                b: folders[1].id.clone(),
            },
            &noop,
        )
        .unwrap();
    let t = &diff.tables[0];
    assert_eq!((t.added, t.removed, t.changed), (1, 1, 1));
    let recs = f
        .backend
        .dev_diff_records(DevDiffRecordsRequest {
            target: live.clone(),
            dataset: "game-weapons.json".into(),
            table: "shipWeapons".into(),
            state: Some(RecordState::Changed),
            field: None,
            identity: None,
            query: None,
            offset: 0,
            limit: 10,
            group: None,
        })
        .unwrap();
    // The Diff tree (addendum 7.15): one dataset, so the table is the top node.
    let tree = f
        .backend
        .dev_diff_groups(DevDiffGroupsRequest {
            target: live.clone(),
            by: DiffGroupBy::Auto,
            open: vec![],
        })
        .unwrap();
    let top = &tree.nodes[0];
    assert_eq!(top.label, "shipWeapons");
    let c = top.diff.unwrap();
    assert_eq!((c.added, c.removed, c.changed), (1, 1, 1));
    let fd = f
        .backend
        .dev_diff_record(DevDiffRecordRequest {
            target: live.clone(),
            dataset: "game-weapons.json".into(),
            table: "shipWeapons".into(),
            id: recs.records[0].id.clone(),
            identity: None,
        })
        .unwrap();
    assert_eq!(fd.changes[0].delta, Some(2.0));
    // Removing a folder forgets it (and the comparison).
    let id = folders[1].id.clone();
    let after = f
        .backend
        .dev_remove_source(DevSourceRef {
            target: live.clone(),
            source: id,
        })
        .unwrap();
    assert_eq!(
        after
            .iter()
            .filter(|s| s.kind == SourceKind::Folder)
            .count(),
        1
    );
    assert_eq!(
        f.backend.dev_diff_summary(live).unwrap_err().code,
        ErrorCode::NotFound
    );
}

/// A build folder of the dataset cache with `game-weapons.json` of `tag`; `measured` are
/// the datasets its `measurements.json` says were extracted.
fn cache_build(dir: &Path, tag: &str, measured: &[&str]) {
    use starorg_mcp::worker::Measurement;
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("game-weapons.json"), dump_body(tag)).unwrap();
    // As an extraction by this build's code stamps it (an unstamped file is not served).
    starorg_mcp::stamps::write(dir, "game-weapons", starorg_mcp::stamps::LOCAL, None).unwrap();
    let m: BTreeMap<&str, Measurement> = measured
        .iter()
        .map(|d| {
            (
                *d,
                Measurement {
                    dataset: d.to_string(),
                    seconds: 1.0,
                    peak_rss_bytes: None,
                    bytes: 1,
                    warnings: 0,
                    sha256: None,
                },
            )
        })
        .collect();
    std::fs::write(
        dir.join("measurements.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
}

#[test]
fn older_cached_builds_are_read_only_sources_next_to_the_current_one() {
    let f = fake(false);
    f.dev_mode(true);
    let live = f.live();
    let p4k = f.base.join("games/StarCitizen/LIVE/Data.p4k");
    let mcp = AppDirs::under(&f.base.join("app")).cache.join("mcp/LIVE");
    let fp = starorg_mcp::store::Store::fingerprint(&p4k);
    // Two cached builds: the current one (this archive) and the one read before it.
    let current = mcp.join(format!("{NEW}-{fp}"));
    let older = mcp.join(format!("{OLD}-0a1b2c3d"));
    cache_build(&current, NEW, &["game-weapons"]);
    cache_build(&older, OLD, &["game-weapons"]);
    let src = f.backend.dev_sources(live.clone()).unwrap();
    let cached: Vec<&DevSource> = src
        .iter()
        .filter(|s| s.kind == SourceKind::Cached)
        .collect();
    assert_eq!(cached.len(), 1, "the current build is not listed twice");
    let old_id = format!("cached:LIVE:{OLD}-0a1b2c3d");
    assert_eq!(src[0].id, "build:LIVE");
    assert_eq!(src[1].id, old_id, "right after its channel's build");
    let c = cached[0];
    assert_eq!(c.label, format!("LIVE {OLD} (cached)"));
    assert_eq!(c.build.as_deref(), Some(OLD));
    assert_eq!(c.state, SourceState::Ready, "{:?}", c.detail);
    assert_eq!(c.datasets[0].file, "game-weapons.json");
    assert_eq!(c.datasets[0].rows, 2);
    // The current build carries what the cache holds for it, with no GameData read yet.
    let build = &src[0];
    assert_eq!(build.state, SourceState::Ready);
    assert_eq!(
        build
            .datasets
            .iter()
            .map(|d| d.file.as_str())
            .collect::<Vec<_>>(),
        ["game-weapons.json"]
    );
    // Previous build (A) against the current one (B), read from the cache only.
    let diff = f
        .backend
        .dev_diff(
            DevDiffRequest {
                target: live.clone(),
                a: old_id.clone(),
                b: "build:LIVE".into(),
            },
            &noop,
        )
        .unwrap();
    let t = diff
        .tables
        .iter()
        .find(|t| t.table == "shipWeapons")
        .unwrap();
    assert_eq!((t.added, t.removed, t.changed), (1, 0, 1));
    let page = f
        .backend
        .dev_rows(DevRowsRequest {
            target: live.clone(),
            source: old_id.clone(),
            dataset: "game-weapons.json".into(),
            table: "shipWeapons".into(),
            columns: vec![],
            search: String::new(),
            filters: vec![],
            sort: None,
            offset: 0,
            limit: 50,
            group: None,
        })
        .unwrap();
    assert_eq!(page.total, 2);
    // Nothing was extracted: still just the two folders, each with only what it had (and
    // the reading locks beside them, never inside).
    let mut names: Vec<String> = std::fs::read_dir(&mcp)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let folders: Vec<&String> = names
        .iter()
        .filter(|n| !n.ends_with(starorg_mcp::store::READING_LOCK))
        .collect();
    assert_eq!(
        folders,
        [&format!("{OLD}-0a1b2c3d"), &format!("{NEW}-{fp}")]
    );
    for dir in [&current, &older] {
        let mut inside: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        inside.sort();
        assert_eq!(
            inside,
            [
                "game-weapons.json",
                "game-weapons.stamp",
                "measurements.json"
            ],
            "{dir:?}"
        );
    }
    assert!(!current.join("gamedata.json").exists());
    // An incomplete older build is an error that says why; it is listed after the newer.
    let cut = mcp.join("4.10.190.1-deadbeef");
    cache_build(&cut, OLD, &["game-weapons", "game-law"]);
    let src = f.backend.dev_sources(live.clone()).unwrap();
    let cached: Vec<&DevSource> = src
        .iter()
        .filter(|s| s.kind == SourceKind::Cached)
        .collect();
    assert_eq!(
        cached.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        [old_id.as_str(), "cached:LIVE:4.10.190.1-deadbeef"],
        "newest first"
    );
    assert_eq!(cached[1].state, SourceState::Error);
    let why = cached[1].detail.as_deref().unwrap();
    assert!(why.contains("game-law.json is missing"), "{why}");
    // A build that is no longer in the cache (or never was) is not found, not a path.
    for gone in [
        "cached:LIVE:4.0.0.1-00000000",
        "cached:LIVE:../../x",
        "cached:LIVE",
    ] {
        let e = f
            .backend
            .dev_dataset(DevDatasetRef {
                target: live.clone(),
                source: gone.into(),
                dataset: "game-weapons.json".into(),
            })
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::NotFound, "{gone}");
    }
}

#[test]
fn the_worker_serves_sections_list_owner_and_compare() {
    use starorg_codex_app::contract::dev::{
        DevCompareRequest, DevListRequest, DevOwnerRequest, ListGroup, ListQuery,
    };
    let f = fake(false);
    f.dev_mode(true);
    let live = f.live();
    let s = f.backend.dev_open(live.clone(), &noop).unwrap();
    // `--dcb` input: built at start, nothing checked against an archive.
    let st = s.sections.expect("section stats");
    assert!(!st.verified);
    let root = f
        .backend
        .dev_tree(DevTreeRequest {
            fold_variants: false,
            player_only: true,
            target: live.clone(),
            root: TreeRoot::Section,
            node: String::new(),
            offset: 0,
            limit: 500,
            main_only: true,
        })
        .unwrap();
    assert!(root.nodes.iter().any(|n| n.label == "Other"));
    let page = f
        .backend
        .dev_list(DevListRequest {
            target: live.clone(),
            query: ListQuery {
                query: "syn_item_00".into(),
                group: ListGroup::Section,
                limit: 5,
                ..Default::default()
            },
        })
        .unwrap();
    assert_eq!(page.items.len(), 5);
    assert_eq!(page.groups.len(), 1);
    assert!(page.total >= 10);
    let owners = f
        .backend
        .dev_owner(DevOwnerRequest {
            target: live.clone(),
            path: r"Data\Objects\none.cga".into(),
        })
        .unwrap();
    assert!(owners.is_empty());
    // Comparing with itself clears.
    let c = f
        .backend
        .dev_compare(DevCompareRequest {
            target: live.clone(),
            other: Some("LIVE".into()),
        })
        .unwrap();
    assert!(c.is_none());
    assert!(f.backend.dev_status(live).unwrap().compared.is_none());
}

fn dump_req(live: &ChannelRef, tag: &str) -> DevDumpRequest {
    DevDumpRequest {
        target: live.clone(),
        tag: tag.into(),
    }
}

fn dumps_switch(f: &Fake, on: bool) {
    let mut s = f.backend.get_settings().unwrap();
    s.network.published_dumps = on;
    f.backend.save_settings(s).unwrap();
}

#[test]
fn published_live_builds_download_verify_and_diff() {
    let f = fake(false);
    f.dev_mode(true);
    let live = f.live();
    // Switch off: no request at all, nothing listed, the download refused.
    assert!(
        f.backend
            .dev_dump_releases(live.clone())
            .unwrap()
            .releases
            .is_empty()
    );
    assert_eq!(
        f.backend
            .dev_download_dump(dump_req(&live, OLD), &noop)
            .unwrap_err()
            .code,
        ErrorCode::NetworkOff
    );
    assert_eq!(f.dumps.calls.load(Ordering::SeqCst), 0);
    dumps_switch(&f, true);
    // On: only LIVE, newest first.
    let rel = f.backend.dev_dump_releases(live.clone()).unwrap().releases;
    assert_eq!(
        rel.iter().map(|r| r.tag.as_str()).collect::<Vec<_>>(),
        [NEW, OLD]
    );
    assert!(rel.iter().all(|r| !r.cached && r.channel == "LIVE"));
    assert_eq!(rel[0].branch.as_deref(), Some("sc-alpha-4.10.0"));
    // PTU or unknown tags are refused; a bad tag never reaches the network.
    for (tag, code) in [
        ("5.0.201.1", ErrorCode::NotFound),
        ("9.9.9.9", ErrorCode::NotFound),
        ("../x", ErrorCode::Invalid),
    ] {
        assert_eq!(
            f.backend
                .dev_download_dump(dump_req(&live, tag), &noop)
                .unwrap_err()
                .code,
            code,
            "{tag}"
        );
    }
    // A download whose file does not match its blob id is refused and not cached.
    *f.dumps.tamper.lock().unwrap() = Some("data/game-weapons.json".into());
    let seen = Mutex::new(Vec::new());
    let e = f
        .backend
        .dev_download_dump(dump_req(&live, OLD), &|p| seen.lock().unwrap().push(p))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid, "{e:?}");
    assert_eq!(seen.lock().unwrap().last().unwrap().phase, JobPhase::Failed);
    assert!(
        f.backend
            .dev_dump_releases(live.clone())
            .unwrap()
            .releases
            .iter()
            .all(|r| !r.cached)
    );
    *f.dumps.tamper.lock().unwrap() = None;
    // Both builds download as jobs with progress.
    let seen = Mutex::new(Vec::new());
    f.backend
        .dev_download_dump(dump_req(&live, OLD), &|p| seen.lock().unwrap().push(p))
        .unwrap();
    let seen = seen.into_inner().unwrap();
    assert!(seen.iter().all(|p| p.kind == JobKind::Dev));
    assert_eq!(seen.last().unwrap().phase, JobPhase::Done);
    assert!(
        seen.iter()
            .any(|p| p.label.starts_with("Downloading game-law.json"))
    );
    let src = f
        .backend
        .dev_download_dump(dump_req(&live, NEW), &noop)
        .unwrap();
    let dumps: Vec<&DevSource> = src.iter().filter(|s| s.kind == SourceKind::Dump).collect();
    assert_eq!(dumps.len(), 2);
    assert_eq!(dumps[0].id, format!("dump:{NEW}"), "newest first");
    assert_eq!(dumps[0].label, format!("LIVE {NEW}"));
    assert_eq!(dumps[0].build.as_deref(), Some(NEW));
    assert_eq!(dumps[0].state, SourceState::Ready);
    let names: Vec<&str> = dumps[0].datasets.iter().map(|d| d.file.as_str()).collect();
    assert_eq!(
        names,
        ["game-law.json", "game-weapons.json"],
        "only data/*.json"
    );
    assert_eq!(dumps[0].datasets[1].rows, 3);
    // Cached under the app cache.
    assert!(f.dump_dir(NEW).is_dir());
    // Off again: no request, the downloaded builds listed and usable.
    dumps_switch(&f, false);
    let calls = f.dumps.calls.load(Ordering::SeqCst);
    let rel = f.backend.dev_dump_releases(live.clone()).unwrap().releases;
    assert_eq!(rel.len(), 2);
    assert!(rel.iter().all(|r| r.cached));
    let sum = f
        .backend
        .dev_diff(
            DevDiffRequest {
                target: live.clone(),
                a: format!("dump:{OLD}"),
                b: format!("dump:{NEW}"),
            },
            &noop,
        )
        .unwrap();
    let w = sum
        .tables
        .iter()
        .find(|t| t.dataset == "game-weapons.json")
        .unwrap();
    assert_eq!((w.added, w.removed, w.changed), (1, 0, 1), "{w:?}");
    let page = f
        .backend
        .dev_rows(DevRowsRequest {
            target: live.clone(),
            source: format!("dump:{NEW}"),
            dataset: "game-weapons.json".into(),
            table: "shipWeapons".into(),
            columns: vec![],
            search: String::new(),
            filters: vec![],
            sort: None,
            offset: 0,
            limit: 10,
            group: None,
        })
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(
        f.dumps.calls.load(Ordering::SeqCst),
        calls,
        "no request while off"
    );
    // A cached file changed on disk fails its checksum when read again.
    let entry = std::fs::read_dir(f.dump_dir(OLD))
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    std::fs::write(entry.join("game-law.json"), br#"{"_build":"x","laws":[]}"#).unwrap();
    let e = f
        .backend
        .dev_dataset(DevDatasetRef {
            target: live.clone(),
            source: format!("dump:{OLD}"),
            dataset: "game-law.json".into(),
        })
        .unwrap_err();
    assert!(e.message.contains("checksum"), "{e:?}");
    assert!(!e.message.contains("archive check failed"), "{e:?}");
    let src = f.backend.dev_sources(live.clone()).unwrap();
    let old = src.iter().find(|s| s.id == format!("dump:{OLD}")).unwrap();
    assert_eq!(old.state, SourceState::Error);
    assert!(
        old.detail.as_deref().unwrap().contains("checksum"),
        "{:?}",
        old.detail
    );
    // A missing file is an error too, not Ready with no tables.
    std::fs::remove_file(
        f.dump_dir(NEW)
            .read_dir()
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path()
            .join("game-law.json"),
    )
    .unwrap();
    let src = f.backend.dev_sources(live.clone()).unwrap();
    let new = src.iter().find(|s| s.id == format!("dump:{NEW}")).unwrap();
    assert_eq!(new.state, SourceState::Error, "{:?}", new.detail);
    // Remove deletes the cache entry.
    let src = f
        .backend
        .dev_remove_source(DevSourceRef {
            target: live.clone(),
            source: format!("dump:{OLD}"),
        })
        .unwrap();
    assert!(src.iter().all(|s| s.id != format!("dump:{OLD}")));
    assert!(!f.dump_dir(OLD).exists());
    assert_eq!(
        f.backend
            .dev_remove_source(DevSourceRef {
                target: live.clone(),
                source: format!("dump:{OLD}"),
            })
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

impl Fake {
    fn dump_dir(&self, tag: &str) -> PathBuf {
        AppDirs::under(&self.base.join("app"))
            .cache
            .join("dumps")
            .join(tag)
    }
}

/// Real network smoke test: both published LIVE builds through the real backend path
/// (GitHub API + raw downloads, verified and cached), then a diff old -> new. Prints sizes,
/// times and the per-table counts.
#[test]
#[ignore = "network: downloads two published LIVE builds (about 50 MB)"]
fn real_network_smoke_download_and_diff() {
    let f = fake(false);
    f.dev_mode(true);
    f.backend
        .set_dump_transport(Arc::new(starorg_net::GitHubTransport::new()));
    dumps_switch(&f, true);
    let live = f.live();
    let t0 = std::time::Instant::now();
    let rel = f.backend.dev_dump_releases(live.clone()).unwrap().releases;
    eprintln!("releases ({} ms):", t0.elapsed().as_millis());
    for r in &rel {
        eprintln!(
            "  {} | {} | {:?} | {:?}",
            r.tag, r.title, r.branch, r.published
        );
    }
    for tag in [OLD, NEW] {
        assert!(rel.iter().any(|r| r.tag == tag), "{tag} listed");
        let t = std::time::Instant::now();
        let src = f
            .backend
            .dev_download_dump(dump_req(&live, tag), &noop)
            .unwrap();
        let s = src.iter().find(|s| s.id == format!("dump:{tag}")).unwrap();
        let bytes: u64 = s.datasets.iter().map(|d| d.bytes).sum();
        let rows: u32 = s.datasets.iter().map(|d| d.rows).sum();
        eprintln!(
            "downloaded {tag}: {} datasets, {:.1} MB, {rows} rows, {:.1} s (incl. verify + parse)",
            s.datasets.len(),
            bytes as f64 / 1e6,
            t.elapsed().as_secs_f64()
        );
        assert_eq!(s.state, SourceState::Ready, "{:?}", s.detail);
    }
    let t = std::time::Instant::now();
    let src = f.backend.dev_sources(live.clone()).unwrap();
    eprintln!(
        "dev_sources with both cached: {} ms ({} sources)",
        t.elapsed().as_millis(),
        src.len()
    );
    let t = std::time::Instant::now();
    let sum = f
        .backend
        .dev_diff(
            DevDiffRequest {
                target: live.clone(),
                a: format!("dump:{OLD}"),
                b: format!("dump:{NEW}"),
            },
            &noop,
        )
        .unwrap();
    eprintln!(
        "diff {OLD} -> {NEW}: {} tables, {:.1} s (backend says {} ms)",
        sum.tables.len(),
        t.elapsed().as_secs_f64(),
        sum.ms
    );
    let (mut a, mut r, mut c) = (0, 0, 0);
    let mut changed_tables = 0;
    for tb in &sum.tables {
        a += tb.added;
        r += tb.removed;
        c += tb.changed;
        if tb.weight() > 0 {
            changed_tables += 1;
            eprintln!(
                "  {}/{}: +{} -{} ~{} ={}",
                tb.dataset, tb.table, tb.added, tb.removed, tb.changed, tb.same
            );
        }
    }
    eprintln!(
        "totals: {changed_tables} of {} tables differ; +{a} -{r} ~{c}",
        sum.tables.len()
    );
    assert!(changed_tables > 0);
}

/// GitHub unreachable or rate limited.
struct Down;
impl DumpTransport for Down {
    fn releases(&self) -> Result<Vec<ReleaseInfo>, SourceError> {
        Err(SourceError::Unavailable(
            "https://api.github.com/...: HTTP 403".into(),
        ))
    }
    fn release(&self, _: &str) -> Result<Option<ReleaseInfo>, SourceError> {
        self.releases().map(|_| None)
    }
    fn tag_commit(&self, _: &str) -> Result<String, SourceError> {
        Err(SourceError::Unavailable("offline".into()))
    }
    fn tree(&self, _: &str) -> Result<Vec<TreeEntry>, SourceError> {
        Err(SourceError::Unavailable("offline".into()))
    }
    fn file(&self, _: &str, _: &str, _: u64) -> Result<Vec<u8>, SourceError> {
        Err(SourceError::Unavailable("offline".into()))
    }
}

#[test]
fn release_list_is_remembered_until_refresh() {
    let f = fake(false);
    f.dev_mode(true);
    let live = f.live();
    dumps_switch(&f, true);
    let calls = || f.dumps.calls.load(Ordering::SeqCst);
    let first = f.backend.dev_dump_releases(live.clone()).unwrap();
    let n = calls();
    assert!(n > 0);
    // A later visit reuses the list: no request, same answer.
    assert_eq!(f.backend.dev_dump_releases(live.clone()).unwrap(), first);
    assert_eq!(calls(), n, "remembered");
    // A download still shows at once as downloaded (read from disk, not the list).
    f.backend
        .dev_download_dump(dump_req(&live, OLD), &noop)
        .unwrap();
    let after = calls();
    let rel = f.backend.dev_dump_releases(live.clone()).unwrap().releases;
    assert_eq!(calls(), after);
    assert!(rel.iter().any(|r| r.tag == OLD && r.cached));
    assert!(rel.iter().any(|r| r.tag == NEW && !r.cached));
    // Refresh asks again.
    let fresh = f.backend.dev_refresh_dump_releases(live.clone()).unwrap();
    assert_eq!(calls(), after + 1);
    assert_eq!(fresh.releases, rel);
    // Off: no request, even on refresh.
    dumps_switch(&f, false);
    f.backend.dev_refresh_dump_releases(live.clone()).unwrap();
    assert_eq!(calls(), after + 1);
    // A failed request is not remembered.
    dumps_switch(&f, true);
    f.backend.set_dump_transport(Arc::new(Down));
    assert!(
        f.backend
            .dev_dump_releases(live.clone())
            .unwrap()
            .error
            .is_some()
    );
    f.backend.set_dump_transport(f.dumps.clone());
    let r = f.backend.dev_dump_releases(live.clone()).unwrap();
    assert!(r.error.is_none());
    assert_eq!(calls(), after + 2);
}

#[test]
fn offline_keeps_downloaded_builds_listed_and_off_stops_a_download() {
    let f = fake(false);
    f.dev_mode(true);
    let live = f.live();
    dumps_switch(&f, true);
    f.backend
        .dev_download_dump(dump_req(&live, OLD), &noop)
        .unwrap();
    // GitHub down: the downloaded build is still listed, with the reason, not an error.
    f.backend.set_dump_transport(Arc::new(Down));
    let r = f.backend.dev_dump_releases(live.clone()).unwrap();
    assert_eq!(
        r.releases
            .iter()
            .map(|x| x.tag.as_str())
            .collect::<Vec<_>>(),
        [OLD]
    );
    assert!(r.releases[0].cached);
    let why = r.error.unwrap();
    assert!(
        why.contains("Could not reach GitHub") && why.contains("403"),
        "{why}"
    );
    assert!(!why.contains("source unavailable"), "{why}");
    // A download then fails as a network problem, not "not found".
    let e = f
        .backend
        .dev_download_dump(dump_req(&live, NEW), &noop)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Internal, "{e:?}");
    assert!(e.message.starts_with("Could not reach GitHub"), "{e:?}");
    // The source still reads.
    assert!(
        f.backend
            .dev_dataset(DevDatasetRef {
                target: live.clone(),
                source: format!("dump:{OLD}"),
                dataset: "game-weapons.json".into(),
            })
            .is_ok()
    );
    // Turned off while downloading: no further request, nothing cached, network-off.
    f.backend.set_dump_transport(f.dumps.clone());
    let app_json = AppDirs::under(&f.base.join("app")).config.join("app.json");
    *f.dumps.off_after.lock().unwrap() =
        Some((f.dumps.files_served.load(Ordering::SeqCst) + 1, app_json));
    let before = f.dumps.files_served.load(Ordering::SeqCst);
    let seen = Mutex::new(Vec::new());
    let e = f
        .backend
        .dev_download_dump(dump_req(&live, NEW), &|p| seen.lock().unwrap().push(p))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NetworkOff, "{e:?}");
    assert_eq!(
        f.dumps.files_served.load(Ordering::SeqCst),
        before + 1,
        "stopped after one file"
    );
    assert_eq!(seen.lock().unwrap().last().unwrap().phase, JobPhase::Failed);
    assert!(!f.dump_dir(NEW).exists() || std::fs::read_dir(f.dump_dir(NEW)).unwrap().count() == 0);
}

#[test]
fn download_progress_is_asking_then_one_step_per_file_then_checking() {
    let f = fake(false);
    f.dev_mode(true);
    dumps_switch(&f, true);
    let live = f.live();
    let seen = Mutex::new(Vec::new());
    f.backend
        .dev_download_dump(dump_req(&live, OLD), &|p| seen.lock().unwrap().push(p))
        .unwrap();
    let got: Vec<(u32, u32, String)> = seen
        .into_inner()
        .unwrap()
        .into_iter()
        .map(|p| (p.step, p.steps, p.label))
        .collect();
    let l = format!("LIVE {OLD}");
    assert_eq!(
        got,
        [
            (1, 1, format!("Asking GitHub for {l}")),
            (1, 3, "Downloading game-law.json (1 of 2)".to_string()),
            (2, 3, "Downloading game-weapons.json (2 of 2)".to_string()),
            (3, 3, format!("Checking {l}")),
            (3, 3, format!("Downloaded {l}")),
        ]
    );
}
