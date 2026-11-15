//! `starorg-codex version` (docs/updates.md).

use crate::commands::JSON_SCHEMA;
use crate::{GlobalOpts, exit};
use serde_json::json;
use std::process::ExitCode;

/// Build details for `version`: compile-time facts only (no network, no files read).
pub fn version_info() -> serde_json::Value {
    json!({
        "schema": JSON_SCHEMA,
        "name": "starorg-codex",
        "version": env!("CARGO_PKG_VERSION"),
        "target": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "features": {
            "published_dumps": cfg!(feature = "published-dumps"),
        },
        "starbreaker_commit": starorg_data::starbreaker::STARBREAKER_COMMIT,
        "repository": env!("CARGO_PKG_REPOSITORY"),
    })
}

/// `starorg-codex version`: prints the version and build details. It never checks for
/// updates (the CLI has no HTTP client; spec 11): new releases are on the release page.
pub fn version(g: &GlobalOpts) -> ExitCode {
    let v = version_info();
    if g.json {
        println!("{v}");
    } else if !g.quiet {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        println!("starorg-codex {}", s("version"));
        println!("  target:      {} ({})", s("target"), s("profile"));
        println!(
            "  features:    published-dumps {}",
            if cfg!(feature = "published-dumps") {
                "on"
            } else {
                "off"
            }
        );
        println!("  starbreaker: {}", s("starbreaker_commit"));
        println!("  releases:    {}/releases", s("repository"));
    }
    ExitCode::from(exit::OK)
}
