//! Process tests for the extraction worker (spec 13, "Process tests").
//!
//! The panic test must pass in release mode too:
//! `cargo test --release -p starorg-cli --test worker`.
//! In release the profile keeps `panic = "unwind"` (spec 3.1); with `abort` the worker would
//! die by SIGABRT and these tests would see a signal instead of a reported error.

use starorg_core::discovery::Channel;
use starorg_core::source::local::LocalExtraction;
use starorg_core::source::{Budget, GameSource, SourceError};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_starorg-codex");

fn fake_channel(dir: &Path) -> Channel {
    Channel {
        name: "LIVE".into(),
        dir: dir.to_path_buf(),
        p4k: dir.join("Data.p4k"),
        build: None,
        build_error: None,
    }
}

fn source(root: &Path, extra: &[&str]) -> LocalExtraction {
    LocalExtraction {
        worker: PathBuf::from(BIN),
        work_root: Some(root.join("work")),
        ini_only: false,
        keyrefs: false,
        extra_args: extra.iter().map(|s| s.to_string()).collect(),
    }
}

fn work_is_empty(root: &Path) -> bool {
    std::fs::read_dir(root.join("work"))
        .map(|mut d| d.next().is_none())
        .unwrap_or(true)
}

#[test]
fn worker_panic_is_reported_as_an_error_not_a_crash() {
    const { assert!(cfg!(panic = "unwind")) };
    let t = tempfile::tempdir().unwrap();
    let out = t.path().join("out");
    let o = Command::new(BIN)
        .args(["extract-worker", "--p4k", "/nonexistent/Data.p4k", "--out"])
        .arg(&out)
        .arg("--test-panic")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1), "exit code 1, not a signal: {o:?}");
    let stdout = String::from_utf8_lossy(&o.stdout);
    let report: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(report["ok"], false);
    let err = report["error"].as_str().unwrap();
    assert!(err.contains("panic during extraction"), "{err}");
    assert!(
        err.contains("test panic inside the extraction worker"),
        "{err}"
    );
    assert!(!out.exists(), "no partial output");

    // Through the parent: a readable error, nothing left in the work dir.
    let src = source(t.path(), &["--test-panic"]);
    match src.load(&fake_channel(t.path()), &Budget::unlimited()) {
        Err(SourceError::Worker(msg)) => assert!(msg.contains("test panic"), "{msg}"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(work_is_empty(t.path()));
}

#[test]
fn worker_killed_on_timeout_leaves_nothing() {
    let t = tempfile::tempdir().unwrap();
    let src = source(t.path(), &["--test-hang"]);
    let started = Instant::now();
    let r = src.load(
        &fake_channel(t.path()),
        &Budget::with_timeout(Duration::from_millis(300)),
    );
    assert!(matches!(r, Err(SourceError::Timeout(_))), "{r:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(work_is_empty(t.path()));
}

#[test]
fn worker_error_message_reaches_the_parent() {
    let t = tempfile::tempdir().unwrap();
    let src = source(t.path(), &[]);
    match src.load(&fake_channel(t.path()), &Budget::unlimited()) {
        Err(SourceError::Worker(msg)) => assert!(msg.contains("cannot open archive"), "{msg}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn missing_worker_binary() {
    let t = tempfile::tempdir().unwrap();
    let src = LocalExtraction::new("/nonexistent/starorg-codex");
    match src.load(&fake_channel(t.path()), &Budget::unlimited()) {
        Err(SourceError::Worker(msg)) => assert!(msg.contains("cannot start"), "{msg}"),
        other => panic!("unexpected {other:?}"),
    }
}
