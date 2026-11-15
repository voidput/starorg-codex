//! `channels` and `build`.

use crate::{GlobalOpts, exit};
use serde_json::json;
use starorg_core::discovery::{
    Channel, Discovery, DiscoveryEnv, DiscoveryRequest, Install, discover,
};
use starorg_core::ini::{Edits, StockIni};
use starorg_core::source::local::LocalExtraction;
use starorg_core::source::{Budget, GameSource, SourceError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// JSON output schema version (fields are only ever added within a major version).
const JSON_SCHEMA: u32 = 1;

pub(crate) fn run_discovery(g: &GlobalOpts) -> Discovery {
    let req = DiscoveryRequest {
        game_dir: g.game_dir.clone(),
        prefix: g.prefix.clone(),
    };
    discover(&req, &DiscoveryEnv::from_process())
}

pub(crate) fn fail(g: &GlobalOpts, code: u8, kind: &str, message: &str) -> ExitCode {
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "error": { "kind": kind, "message": message } })
        );
    }
    eprintln!("starorg-strings: {message}");
    ExitCode::from(code)
}

fn channel_json(c: &Channel) -> serde_json::Value {
    let b = c.build.as_ref();
    json!({
        "channel": c.name,
        "dir": c.dir,
        "branch": b.map(|b| b.branch.as_str()),
        "game_version": b.and_then(|b| b.game_version).map(|v| v.to_string()),
        "change": b.and_then(|b| b.change),
        "version": b.map(|b| b.version.as_str()),
        "build_error": c.build_error,
    })
}

fn install_json(i: &Install) -> serde_json::Value {
    json!({
        "root": i.root,
        "prefix": i.prefix,
        "found_by": i.found_by,
        "channels": i.channels.iter().map(channel_json).collect::<Vec<_>>(),
    })
}

pub fn channels(g: &GlobalOpts) -> ExitCode {
    let d = run_discovery(g);
    if d.installs.is_empty() {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found (try --game-dir or --prefix)",
        );
    }
    if g.json {
        let installs: Vec<_> = d.installs.iter().map(install_json).collect();
        println!("{}", json!({ "schema": JSON_SCHEMA, "installs": installs }));
        return ExitCode::from(exit::OK);
    }
    if g.quiet {
        return ExitCode::from(exit::OK);
    }
    for (n, i) in d.installs.iter().enumerate() {
        let default = if n == 0 { " (default)" } else { "" };
        println!("Install{default}: {}", i.root.display());
        print!("  found by: {}", i.found_by.describe());
        match &i.prefix {
            Some(p) => println!(", Wine prefix {}", p.display()),
            None => println!(),
        }
        println!(
            "  {:<14} {:<26} {:<8} {:<10} VERSION",
            "CHANNEL", "BRANCH", "GAME", "CHANGE"
        );
        for c in &i.channels {
            match &c.build {
                Some(b) => println!(
                    "  {:<14} {:<26} {:<8} {:<10} {}",
                    c.name,
                    b.branch,
                    b.game_version
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                    b.change
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                    b.version
                ),
                None => println!(
                    "  {:<14} (no build info: {})",
                    c.name,
                    c.build_error.as_deref().unwrap_or("unknown")
                ),
            }
        }
    }
    ExitCode::from(exit::OK)
}

/// Canonical form of a path that may not exist yet: canonicalise the deepest existing
/// ancestor and append the rest.
pub(crate) fn canonical_lenient(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut existing = abs.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(c) = std::fs::canonicalize(existing) {
            let mut out = c;
            for r in rest.iter().rev() {
                out.push(r);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return abs,
        }
    }
}

/// Exit with a usage error when `out_abs` lies inside the install or the whole Wine prefix
/// it sits in (commands that write output never write there).
pub(crate) fn refuse_protected(
    g: &GlobalOpts,
    out_abs: &Path,
    install: &Install,
) -> Option<ExitCode> {
    let hit = install
        .protected_roots()
        .into_iter()
        .find(|r| out_abs.starts_with(r))?;
    Some(fail(
        g,
        exit::USAGE,
        "usage",
        &format!(
            "--out {} is inside {}; output is never written into the game install or its Wine prefix",
            out_abs.display(),
            hit.display()
        ),
    ))
}

/// Write `bytes` to `path` atomically: temp file in the same directory, fsync, rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    if let Some(d) = dir {
        std::fs::create_dir_all(d)?;
    }
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("output path has no file name"))?;
    let tmp = path.with_file_name(format!(
        ".{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub(crate) fn worker_exe() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

pub fn build(g: &GlobalOpts, channel: &str, out: &Path, timeout: Option<u64>) -> ExitCode {
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
    let Some(ch) = install.channel(channel) else {
        let names: Vec<_> = install.channels.iter().map(|c| c.name.as_str()).collect();
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!(
                "channel {channel} not found in {} (have: {})",
                install.root.display(),
                names.join(", ")
            ),
        );
    };

    // `build` writes only the given file, never into the game or its Wine prefix.
    let out_abs = canonical_lenient(out);
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
    let budget = match timeout {
        Some(s) => Budget::with_timeout(Duration::from_secs(s)),
        None => Budget::unlimited(),
    };
    if !g.quiet && !g.json {
        eprintln!("Extracting {} from {} ...", ch.name, ch.p4k.display());
    }
    let loaded = match LocalExtraction::new(worker).load(ch, &budget) {
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

    // No enhancements yet (M0): the output is the stock file, rendered by the writer.
    let stock = StockIni::parse(loaded.base_ini);
    let edits = Edits::new();
    let bytes = stock.render(&edits);
    if bytes != stock.as_bytes() {
        return fail(
            g,
            exit::ERROR,
            "writer",
            "writer bug: unchanged render differs from stock",
        );
    }
    if let Err(e) = write_atomic(out, &bytes) {
        return fail(
            g,
            exit::ERROR,
            "io",
            &format!("cannot write {}: {e}", out.display()),
        );
    }

    let code = if loaded.warnings.is_empty() {
        exit::OK
    } else {
        exit::WARNINGS
    };
    let b = ch.build.as_ref();
    if g.json {
        println!(
            "{}",
            json!({
                "schema": JSON_SCHEMA,
                "channel": ch.name,
                "result": "built",
                "build": {
                    "branch": b.map(|b| b.branch.as_str()),
                    "game_version": b.and_then(|b| b.game_version).map(|v| v.to_string()),
                    "change": b.and_then(|b| b.change),
                    "version": b.map(|b| b.version.as_str()),
                },
                "out": out_abs,
                "bytes": bytes.len(),
                "keys": stock.key_count(),
                "counts": { "changed": edits.len() },
                "origin": loaded.origin,
                "warnings": loaded.warnings,
                "duration_ms": started.elapsed().as_millis() as u64,
            })
        );
    } else if !g.quiet {
        for w in &loaded.warnings {
            eprintln!("warning: {}: {}", w.kind, w.detail);
        }
        println!(
            "Wrote {} ({} bytes, {} keys, {} changed) for {} {} in {:.1} s",
            out_abs.display(),
            bytes.len(),
            stock.key_count(),
            edits.len(),
            ch.name,
            b.map(|b| b.version.as_str()).unwrap_or("unknown build"),
            started.elapsed().as_secs_f64()
        );
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_lenient_handles_missing_tail() {
        let t = tempfile::tempdir().unwrap();
        let real = std::fs::canonicalize(t.path()).unwrap();
        assert_eq!(
            canonical_lenient(&t.path().join("a/b/c.ini")),
            real.join("a/b/c.ini")
        );
    }

    #[test]
    fn write_atomic_creates_parent_and_leaves_no_temp() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("sub/x.ini");
        write_atomic(&p, b"abc").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"abc");
        let names: Vec<_> = std::fs::read_dir(t.path().join("sub"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec!["x.ini"]);
    }
}
