//! `starorg-strings mcp [--channel NAME]`: the experimental local MCP server over stdio.
//!
//! stdout carries the protocol only; status lines go to stderr (none with `--quiet`).
//! Datasets are extracted on first use by this same binary started as
//! `mcp-extract-worker` (one dataset per process) and cached under the app cache folder.

use crate::commands::fail;
use crate::{GlobalOpts, exit};
use starorg_core::appdirs::AppDirs;
use starorg_core::discovery::DiscoveryRequest;
use starorg_mcp::worker::ProcessExtractor;
use std::process::ExitCode;
use std::sync::Arc;

pub fn run(g: &GlobalOpts, channel: Option<&str>) -> ExitCode {
    let Some(dirs) = AppDirs::from_env() else {
        return fail(
            g,
            exit::ERROR,
            "error",
            "cannot find the home directory for the app folders",
        );
    };
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            return fail(
                g,
                exit::ERROR,
                "error",
                &format!("cannot find own binary: {e}"),
            );
        }
    };
    let req = DiscoveryRequest {
        game_dir: g.game_dir.clone(),
        prefix: g.prefix.clone(),
    };
    let mut cfg = starorg_mcp::config_from_env(&req, &dirs, Arc::new(ProcessExtractor::new(exe)));
    if let Some(ch) = channel {
        if !cfg.channels.iter().any(|c| c.name.eq_ignore_ascii_case(ch)) {
            let have: Vec<&str> = cfg.channels.iter().map(|c| c.name.as_str()).collect();
            return fail(
                g,
                exit::NOT_FOUND,
                "not-found",
                &format!("channel {ch} not found (have: {})", have.join(", ")),
            );
        }
        cfg.default_channel = Some(ch.to_string());
    }
    let quiet = g.quiet;
    if !quiet {
        let names: Vec<&str> = cfg.channels.iter().map(|c| c.name.as_str()).collect();
        eprintln!(
            "starorg-strings mcp: experimental, read-only; channels {} (cache {})",
            if names.is_empty() {
                "none found".to_string()
            } else {
                names.join(", ")
            },
            cfg.cache_dir.display()
        );
    }
    let notify: starorg_mcp::store::Notify = Arc::new(move |m: String| {
        if !quiet {
            eprintln!("{m}");
        }
    });
    let server = starorg_mcp::server(cfg, Some(notify));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match starorg_mcp::stdio::serve(&server, stdin.lock(), stdout.lock(), |_| {}) {
        Ok(()) => ExitCode::from(exit::OK),
        Err(e) => {
            eprintln!("starorg-strings mcp: {e}");
            ExitCode::from(exit::ERROR)
        }
    }
}
