//! The CLI links no HTTP client (spec 11; ci.yml's `cli-deps` job checks the full tree with
//! `cargo tree`). This catches it earlier, from the manifests alone: no workspace crate the
//! CLI depends on (normal dependencies, followed through the workspace) names an HTTP crate
//! or `starorg-net`. The prebuilt game data download lives in the app for this reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    "starorg-net",
    "ureq",
    "reqwest",
    "hyper",
    "curl",
    "isahc",
    "attohttpc",
];

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Normal dependencies of a workspace crate (`[dependencies]` and every
/// `[target.*.dependencies]`), by package name: a renamed one (`x = { package = "ureq" }`)
/// counts under both names.
fn deps(krate: &str) -> Vec<String> {
    let text = std::fs::read_to_string(crates_dir().join(krate).join("Cargo.toml")).unwrap();
    let doc: toml::Table = text.parse().unwrap();
    let mut tables: Vec<&toml::Table> = doc
        .get("dependencies")
        .and_then(|d| d.as_table())
        .into_iter()
        .collect();
    for t in doc
        .get("target")
        .and_then(|t| t.as_table())
        .into_iter()
        .flat_map(|t| t.values())
    {
        tables.extend(t.get("dependencies").and_then(|d| d.as_table()));
    }
    let mut out = Vec::new();
    for t in tables {
        for (k, v) in t {
            out.push(k.clone());
            if let Some(p) = v.get("package").and_then(|p| p.as_str()) {
                out.push(p.to_string());
            }
        }
    }
    out
}

#[test]
fn the_cli_reaches_no_http_crate_through_the_workspace() {
    let mut seen = BTreeSet::new();
    let mut todo = vec!["starorg-cli".to_string()];
    while let Some(k) = todo.pop() {
        if !seen.insert(k.clone()) {
            continue;
        }
        for d in deps(&k) {
            assert!(
                !FORBIDDEN.contains(&d.as_str()),
                "{k} depends on {d}: the CLI must not link an HTTP client"
            );
            if crates_dir().join(&d).join("Cargo.toml").is_file() {
                todo.push(d);
            }
        }
    }
    assert!(
        seen.contains("starorg-mcp"),
        "the MCP crate is in the CLI: {seen:?}"
    );
}
