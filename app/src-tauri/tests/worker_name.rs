//! The app starts its workers (extract-worker, dev-worker, asset-worker, mcp-extract-worker)
//! as `std::env::current_exe()`, so they follow the binary whatever it is called. Since 0.3.0
//! it is `starorg-codex-app` (`starorg-codex-app.exe`); these run it under that name, as the
//! packages install it, and check every worker subcommand is handled without a window.

use std::path::{Path, PathBuf};
use std::process::Command;

const APP: &str = env!("CARGO_BIN_EXE_starorg-codex-app");

fn installed_copy(dir: &Path) -> PathBuf {
    let exe = dir.join(format!("starorg-codex-app{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(APP, &exe).unwrap();
    exe
}

#[test]
fn binary_has_the_new_name() {
    let name = Path::new(APP)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        name,
        format!("starorg-codex-app{}", std::env::consts::EXE_SUFFIX)
    );
}

#[test]
fn every_worker_subcommand_runs_under_the_new_name() {
    let t = tempfile::tempdir().unwrap();
    let exe = installed_copy(t.path());
    for (sub, args) in [
        ("extract-worker", vec!["--bogus"]),
        ("dev-worker", vec!["--bogus"]),
        ("asset-worker", vec!["--bogus"]),
        ("mcp-extract-worker", vec!["--bogus"]),
    ] {
        let o = Command::new(&exe)
            .arg(sub)
            .args(&args)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(2),
            "{sub}: a usage error, no window: {o:?}"
        );
    }
    // A real extraction attempt: the worker answers with its JSON report.
    let out = t.path().join("out");
    let o = Command::new(&exe)
        .args(["extract-worker", "--p4k", "/nonexistent/Data.p4k", "--out"])
        .arg(&out)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1), "{o:?}");
    let report: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&o.stdout).trim()).unwrap();
    assert!(report.is_object(), "{report}");
}
