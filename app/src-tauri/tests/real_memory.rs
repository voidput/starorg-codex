//! The memory limit on the real backend (addendum 7.18): the setting round trip through
//! `config.toml`, and the real `dev-worker` (this package's binary, on a synthetic DCB)
//! stopped by a tiny limit and reported as such instead of a plain crash.

use starorg_codex_app::contract::Backend;
use starorg_codex_app::contract::dev::*;
use starorg_codex_app::contract::real::{
    DevSpawner, DialogSlot, RealBackend, RealConfig, SourceFactory,
};
use starorg_codex_app::contract::types::*;
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{Channel, DiscoveryEnv, DiscoveryRequest};
use starorg_core::source::GameSource;
use starorg_data::starbreaker::synthetic;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

struct NoSource;
impl SourceFactory for NoSource {
    fn source(&self, _: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        Err("no extraction in these tests".into())
    }
}

struct Synthetic {
    dcb: PathBuf,
    ini: PathBuf,
}
impl DevSpawner for Synthetic {
    fn command(&self, _: &Channel) -> Result<Command, String> {
        let mut c = Command::new(env!("CARGO_BIN_EXE_starorg-codex-app"));
        c.arg("dev-worker")
            .arg("--dcb")
            .arg(&self.dcb)
            .arg("--ini")
            .arg(&self.ini);
        Ok(c)
    }
}

struct Fake {
    _t: tempfile::TempDir,
    dirs: AppDirs,
    backend: RealBackend,
}

fn fake() -> Fake {
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
    let backend = RealBackend::new(RealConfig {
        ctx: Ctx::new(dirs.clone(), "0.0.0-test", "test"),
        discovery: DiscoveryRequest {
            game_dir: Some(lib),
            prefix: None,
        },
        env: DiscoveryEnv::from_process(),
        source: Arc::new(NoSource),
        dialogs: DialogSlot::default(),
    })
    .with_dev_spawner(Arc::new(Synthetic { dcb, ini }));
    let mut s = backend.get_settings().unwrap();
    s.advanced.developer_mode = true;
    backend.save_settings(s).unwrap();
    Fake {
        _t: t,
        dirs,
        backend,
    }
}

impl Fake {
    fn live(&self) -> ChannelRef {
        ChannelRef {
            install: self.backend.overview().unwrap().installs[0].id.clone(),
            channel: "LIVE".into(),
        }
    }
}

fn noop(_: JobProgress) {}

#[test]
fn the_limit_is_saved_in_config_toml_and_clamped() {
    let f = fake();
    let s = f.backend.memory_status().unwrap();
    assert!(s.is_default);
    assert!(s.min_gb == 1.5 && s.step_gb == 0.5 && s.max_gb >= s.min_gb);
    assert!(s.default_gb <= 5.0 && s.limit_gb == s.default_gb);
    assert!(s.estimates.windows(2).all(|w| w[0].bytes <= w[1].bytes));
    let s = f
        .backend
        .set_memory_limit(MemoryLimitRequest { gb: Some(0.3) })
        .unwrap();
    assert_eq!(s.limit_gb, 1.5, "clamped to the slider");
    let text = std::fs::read_to_string(f.dirs.config_toml()).unwrap();
    assert!(
        text.contains("[dev]") && text.contains("memory_limit_gb = 1.5"),
        "{text}"
    );
    let s = f
        .backend
        .set_memory_limit(MemoryLimitRequest { gb: None })
        .unwrap();
    assert!(s.is_default);
    let text = std::fs::read_to_string(f.dirs.config_toml()).unwrap_or_default();
    assert!(!text.contains("memory_limit_gb"), "{text}");
}

#[test]
fn the_explorer_opens_under_the_default_limit() {
    let f = fake();
    let s = f.backend.dev_open(f.live(), &noop).unwrap();
    assert_eq!(s.state, WorkerState::Ready);
    f.backend.dev_close(f.live()).unwrap();
}

#[test]
fn a_worker_over_its_limit_is_stopped_and_says_so() {
    let f = fake();
    // Hand-edited below the slider: about 1 MB, which no worker fits in.
    std::fs::create_dir_all(f.dirs.config_toml().parent().unwrap()).unwrap();
    std::fs::write(f.dirs.config_toml(), "[dev]\nmemory_limit_gb = 0.001\n").unwrap();
    assert_eq!(f.backend.memory_status().unwrap().limit_gb, 0.001);
    let e = f.backend.dev_open(f.live(), &noop).unwrap_err();
    assert!(
        e.message.starts_with("Stopped: needed more than 1 MB."),
        "{}",
        e.message
    );
    assert!(e.message.contains("Raise the limit"), "{}", e.message);
    let st = f.backend.dev_status(f.live()).unwrap();
    assert_eq!(st.state, WorkerState::Failed);
    assert!(st.error.unwrap().starts_with("Stopped: needed more than"));
    // Back to a sane limit: it opens.
    f.backend
        .set_memory_limit(MemoryLimitRequest { gb: Some(2.0) })
        .unwrap();
    assert_eq!(
        f.backend.dev_open(f.live(), &noop).unwrap().state,
        WorkerState::Ready
    );
    f.backend.dev_close(f.live()).unwrap();
}
