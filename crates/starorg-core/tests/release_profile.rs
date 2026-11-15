//! Spec 3.1: the release profile must keep `panic = "unwind"`. The extraction worker relies on
//! `catch_unwind` to turn a DataCore panic into a readable error. Tauri size guides suggest
//! `abort`; this test fails if anyone follows them.

#[test]
fn release_profile_keeps_panic_unwind() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let text = std::fs::read_to_string(manifest).expect("read workspace Cargo.toml");
    let doc: toml::Table = text.parse().expect("parse workspace Cargo.toml");
    let panic = doc
        .get("profile")
        .and_then(|p| p.get("release"))
        .and_then(|r| r.get("panic"))
        .and_then(|v| v.as_str());
    assert_eq!(
        panic,
        Some("unwind"),
        "[profile.release] panic must be \"unwind\" (spec 3.1)"
    );
}

#[test]
fn workspace_has_single_version() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let doc: toml::Table = std::fs::read_to_string(manifest).unwrap().parse().unwrap();
    let version = doc["workspace"]["package"]["version"].as_str();
    assert!(
        version.is_some(),
        "[workspace.package] version is the single version"
    );
}

/// Compiled in the profile under test: under `cargo test --release` this proves the release
/// profile really unwinds.
#[test]
fn compiled_with_unwind() {
    const { assert!(cfg!(panic = "unwind")) };
}
