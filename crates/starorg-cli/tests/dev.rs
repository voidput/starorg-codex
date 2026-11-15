//! `dev record|export-type|types` (addendum 7.9) on a synthetic DCB (the `--test-dcb` hook),
//! plus the real-archive refusals that need no game data.

use starorg_data::starbreaker::synthetic;
use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_starorg-strings");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env_remove("WINEPREFIX")
        .output()
        .unwrap()
}

fn dcb(dir: &Path) -> String {
    let p = dir.join("Game2.dcb");
    std::fs::write(&p, synthetic::build(2, 30, 3)).unwrap();
    p.display().to_string()
}

#[test]
fn types_lists_counts() {
    let t = tempfile::tempdir().unwrap();
    let d = dcb(t.path());
    let o = run(&["dev", "types", "--test-dcb", &d, "--json"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["records"], 35);
    let items = v["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == synthetic::ITEM_TYPE)
        .unwrap();
    assert_eq!(items["records"], 30);
    let o = run(&["dev", "types", "--test-dcb", &d]);
    assert!(String::from_utf8_lossy(&o.stdout).contains(synthetic::TAG_TYPE));
}

#[test]
fn record_prints_starbreaker_json_by_name_guid_or_path() {
    let t = tempfile::tempdir().unwrap();
    let d = dcb(t.path());
    for what in [
        "SYN_Item_003".to_string(),
        synthetic::guid_text(1, 3),
        synthetic::item_path(3),
    ] {
        let o = run(&["dev", "record", &what, "--test-dcb", &d]);
        assert_eq!(o.status.code(), Some(0), "{what}: {o:?}");
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["_RecordName_"], "SYN_Item_003");
        // References as StarBreaker writes them: relative paths.
        assert!(
            v["_RecordValue_"]["Maker"]
                .as_str()
                .unwrap()
                .starts_with("file://")
        );
    }
    let o = run(&[
        "dev",
        "record",
        "SYN_Item_003",
        "--compact",
        "--test-dcb",
        &d,
    ]);
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim().lines().count(), 1);
    let o = run(&["dev", "record", "nothing", "--test-dcb", &d]);
    assert_eq!(o.status.code(), Some(4));
}

#[test]
fn export_type_writes_an_array_or_ndjson() {
    let t = tempfile::tempdir().unwrap();
    let d = dcb(t.path());
    let out = t.path().join("items.json");
    let o = run(&[
        "dev",
        "export-type",
        synthetic::ITEM_TYPE,
        "--out",
        out.to_str().unwrap(),
        "--test-dcb",
        &d,
        "--json",
    ]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let arr: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(arr.len(), 30);
    let nd = t.path().join("items.ndjson");
    let o = run(&[
        "dev",
        "export-type",
        synthetic::ITEM_TYPE,
        "--out",
        nd.to_str().unwrap(),
        "--ndjson",
        "--test-dcb",
        &d,
    ]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&nd).unwrap().lines().count(), 30);
    let o = run(&[
        "dev",
        "export-type",
        "Nope",
        "--out",
        out.to_str().unwrap(),
        "--test-dcb",
        &d,
    ]);
    assert_eq!(o.status.code(), Some(4));
}

#[test]
fn a_missing_channel_or_archive_fails_cleanly() {
    let t = tempfile::tempdir().unwrap();
    let o = run(&[
        "dev",
        "types",
        "--game-dir",
        t.path().to_str().unwrap(),
        "--channel",
        "LIVE",
    ]);
    assert_eq!(o.status.code(), Some(4), "{o:?}");
    let o = run(&["dev", "types", "--test-dcb", "/nonexistent/Game2.dcb"]);
    assert_eq!(o.status.code(), Some(1));
}
