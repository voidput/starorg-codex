//! Developer mode on the real backend (addendum 7.11): the real `dev-worker` process (this
//! package's binary) on a synthetic DCB built with StarBreaker's DcbBuilder, a fake install
//! in a temp folder, and native dialogs replaced by fixed answers. Read only; nothing here
//! touches a game install.

use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{Channel, DiscoveryEnv, DiscoveryRequest};
use starorg_core::source::GameSource;
use starorg_data::starbreaker::synthetic;
use starorg_strings_app::contract::Backend;
use starorg_strings_app::contract::dev::*;
use starorg_strings_app::contract::real::{
    DevSpawner, DialogSlot, Dialogs, FileFilter, RealBackend, RealConfig, SourceFactory,
};
use starorg_strings_app::contract::types::*;
use std::path::{Path, PathBuf};
use std::process::Command;
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
        let mut c = Command::new(env!("CARGO_BIN_EXE_starorg-strings-app"));
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
        let mut c = Command::new(env!("CARGO_BIN_EXE_starorg-strings-app"));
        c.args(["dev-worker", "--dcb"])
            .arg(Path::new("/nonexistent/Game2.dcb"));
        Ok(c)
    }
}

struct Fake {
    _t: tempfile::TempDir,
    base: PathBuf,
    backend: RealBackend,
    answers: Arc<Mutex<Answers>>,
    starts: Arc<Mutex<u32>>,
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
    Fake {
        _t: t,
        base,
        backend,
        answers,
        starts,
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
    let dump = before.iter().find(|s| s.kind == SourceKind::Dump).unwrap();
    assert_eq!(dump.state, SourceState::Off);
    assert_eq!(
        f.backend
            .dev_download_dump(live.clone(), &noop)
            .unwrap_err()
            .code,
        ErrorCode::NetworkOff
    );
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
