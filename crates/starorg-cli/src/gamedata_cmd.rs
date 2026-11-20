//! `gamedata`: extract the `GameData` of a channel from its `Data.p4k` and write it as JSON
//! to a file. No network. Touches nothing else; never writes into the install or its Wine
//! prefix.

use crate::commands::{
    canonical_lenient, fail, refuse_protected, run_discovery, worker_exe, write_atomic,
};
use crate::{GlobalOpts, exit};
use serde_json::json;
use starorg_core::source::local::LocalExtraction;
use starorg_core::source::{Budget, GameSource, SourceError};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

pub struct Args<'a> {
    pub channel: &'a str,
    pub out: &'a Path,
    pub timeout: Option<u64>,
}

pub fn run(g: &GlobalOpts, a: &Args) -> ExitCode {
    let started = Instant::now();
    let d = run_discovery(g);
    let Some(install) = d.default_install() else {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found (try --game-dir or --prefix)",
        );
    };
    let Some(ch) = install.channel(a.channel) else {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!(
                "channel {} not found in {}",
                a.channel,
                install.root.display()
            ),
        );
    };
    let out_abs = canonical_lenient(a.out);
    if let Some(code) = refuse_protected(g, &out_abs, install) {
        return code;
    }
    let worker = match worker_exe() {
        Ok(w) => w,
        Err(e) => {
            return fail(
                g,
                exit::ERROR,
                "error",
                &format!("cannot find own binary: {e}"),
            );
        }
    };
    let src = LocalExtraction::new(worker);
    let budget = a
        .timeout
        .map(|s| Budget::with_timeout(Duration::from_secs(s)))
        .unwrap_or_else(Budget::unlimited);
    let loaded = match src.load(ch, &budget) {
        Ok(l) => l,
        Err(SourceError::Timeout(t)) => {
            return fail(
                g,
                exit::SKIPPED,
                "timeout",
                &format!(
                    "out of time after {:.1} s; nothing written",
                    t.as_secs_f64()
                ),
            );
        }
        Err(e) => return fail(g, exit::ERROR, "extraction", &e.to_string()),
    };
    let mut gd = loaded.game_data;
    gd.build = ch.build.clone();
    let bytes = match serde_json::to_vec_pretty(&gd) {
        Ok(b) => b,
        Err(e) => return fail(g, exit::ERROR, "error", &e.to_string()),
    };
    if let Err(e) = write_atomic(a.out, &bytes) {
        return fail(
            g,
            exit::ERROR,
            "io",
            &format!("cannot write {}: {e}", a.out.display()),
        );
    }
    let counts = gd.counts();
    let code = if loaded.warnings.is_empty() {
        exit::OK
    } else {
        exit::WARNINGS
    };
    if g.json {
        println!(
            "{}",
            json!({
                "schema": 1,
                "channel": ch.name,
                "result": "written",
                "out": out_abs,
                "origin": loaded.origin,
                "counts": counts,
                "warnings": loaded.warnings,
                "duration_ms": started.elapsed().as_millis() as u64,
            })
        );
    } else if !g.quiet {
        for w in &loaded.warnings {
            eprintln!("warning: {}: {}", w.kind, w.detail);
        }
        println!(
            "Wrote {} for {} in {:.1} s: {} ships, {} components, {} ship / {} FPS weapons, {} missiles, {} contracts, {} blueprints",
            out_abs.display(),
            ch.name,
            started.elapsed().as_secs_f64(),
            counts.ships,
            counts.components,
            counts.ship_weapons,
            counts.fps_weapons,
            counts.missiles,
            counts.contracts,
            counts.blueprints
        );
    }
    ExitCode::from(code)
}
