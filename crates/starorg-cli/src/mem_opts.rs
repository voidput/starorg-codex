//! `--memory-limit <GB>` / `STARORG_MEMORY_LIMIT_GB` of `dev` and `mcp` (addendum 7.17).
//!
//! Default: the app's setting (`config.toml` `[dev] memory_limit_gb`), else 5 GB or half the
//! RAM when that is less; `0` = no limit. `dev` runs in this process, so it holds the cap
//! itself (`RLIMIT_DATA` on Linux and the 90% watchdog, `starorg_core::memlimit::arm`);
//! `mcp` puts it on each extraction worker.

use crate::commands::fail;
use crate::{GlobalOpts, exit};
use serde_json::json;
use starorg_core::config::Config;
use starorg_core::memlimit;
use std::process::ExitCode;
use std::sync::OnceLock;

static LIMIT: OnceLock<Option<u64>> = OnceLock::new();

/// The limit in bytes for this run (`None` = no limit), from the flag (clap also reads the
/// environment variable into it), the setting or the default.
pub fn resolve(g: &GlobalOpts, flag: Option<&str>) -> Result<Option<u64>, ExitCode> {
    let cfg = starorg_core::appdirs::AppDirs::from_env()
        .and_then(|d| Config::load(&d).ok())
        .unwrap_or_default();
    let l = memlimit::effective_limit(flag, None, &cfg, memlimit::meminfo().total)
        .map_err(|e| fail(g, exit::USAGE, "usage", &e))?;
    let _ = LIMIT.set(l);
    Ok(l)
}

/// The limit [`resolve`] found (`None` before it ran or with no limit).
pub fn current() -> Option<u64> {
    LIMIT.get().copied().flatten()
}

/// Hold this process to `limit`: at 90% it prints the error (as `fail` would) and exits 1.
/// `told`: the limit came from the flag or the environment (then a status line says so).
pub fn arm_here(g: &GlobalOpts, limit: Option<u64>, told: bool) {
    let Some(limit) = limit else { return };
    let json_out = g.json;
    let a = memlimit::arm(
        limit,
        i32::from(exit::ERROR),
        Box::new(move |m| {
            if json_out {
                println!(
                    "{}",
                    json!({ "schema": crate::commands::JSON_SCHEMA, "error": { "kind": "memory-limit", "message": m } })
                );
            }
            eprintln!("starorg-strings: {m}");
        }),
    );
    if told && !g.quiet && !g.json {
        eprintln!(
            "starorg-strings: memory limit {} ({}; --memory-limit to change, 0 = none)",
            memlimit::human(limit),
            a.hard.label()
        );
    }
}
