//! `hook prelaunch|postexit|install|uninstall|status` (addendum §2).
//!
//! `prelaunch` and `postexit` are what the LUG Helper scripts call. They never fail the
//! launch: every outcome is reported and the exit code is 0 (2 only for usage errors). The
//! apply's own code is in the JSON (`exit_code`).

use crate::apply_cmds::{
    apply_channels, ctx, install, lookup, notify, print_reports, register, source, summary,
};
use crate::blueprint_cmds::{print_scan, scan_channel};
use crate::commands::{JSON_SCHEMA, fail, run_discovery};
use crate::{GlobalOpts, HookCmd, HookScripts, exit};
use serde_json::{Value, json};
use starorg_core::apply::{ApplyOpts, Outcome};
use starorg_core::hooks::{self, BlockOpts, BlockState, Kind};
use starorg_core::source::Budget;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

pub fn run(g: &GlobalOpts, cmd: HookCmd) -> ExitCode {
    match cmd {
        HookCmd::Prelaunch { timeout } => launch_hook(g, Kind::Prelaunch, timeout),
        HookCmd::Postexit { timeout } => launch_hook(g, Kind::Postexit, timeout),
        HookCmd::Install { scripts, bin } => edit(g, &scripts, bin.as_deref(), true),
        HookCmd::Uninstall { scripts } => edit(g, &scripts, None, false),
        HookCmd::Status { scripts, bin } => status(g, &scripts, bin.as_deref()),
    }
}

/// Print a non-fatal hook problem and exit 0.
fn soft(g: &GlobalOpts, kind: Kind, msg: &str) -> ExitCode {
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": format!("hook {}", kind.as_str()),
                    "exit_code": exit::ERROR, "error": msg })
        );
    }
    eprintln!("starorg-codex hook {}: {msg} (non-fatal)", kind.as_str());
    ExitCode::from(exit::OK)
}

fn launch_hook(g: &GlobalOpts, kind: Kind, timeout: Option<u64>) -> ExitCode {
    let started = Instant::now();
    let budget = Duration::from_secs(timeout.unwrap_or(kind.default_timeout()));
    let deadline = started + budget;
    let Ok(c) = ctx(g) else {
        return soft(g, kind, "no app folders");
    };
    let Some(i) = run_discovery(g).installs.into_iter().next() else {
        return soft(g, kind, "no Star Citizen install found");
    };
    let Ok(id) = register(g, &c, &i) else {
        return soft(g, kind, "cannot register the install");
    };
    // Post-exit: read new blueprints first (every channel with logs; reading is harmless and
    // the store is ours).
    let mut scans = Vec::new();
    if kind == Kind::Postexit {
        for ch in &i.channels {
            match scan_channel(&c, Some(&id), ch, Some(deadline), true) {
                Ok((store, r)) => {
                    if !g.json && (!r.new_blueprints.is_empty() || !r.warnings.is_empty()) {
                        print_scan(g, &ch.name, &store, &r);
                    }
                    scans.push(json!({ "channel": ch.name, "scan": r }));
                }
                Err(e) => {
                    eprintln!("starorg-codex: {}: blueprint scan skipped: {e}", ch.name);
                    scans.push(json!({ "channel": ch.name, "error": e }));
                }
            }
        }
    }
    // "Sync automatically after game exit": the CLI has no HTTP client, so it leaves the
    // app a note (starorg_core::starorg::PENDING_FILE); the app syncs when it next looks.
    let mut sync_pending = false;
    if kind == Kind::Postexit {
        let prefs = starorg_core::starorg::SyncPrefs::load(&c.dirs);
        if prefs.auto_sync && prefs.any_network() {
            let names: Vec<String> = i.channels.iter().map(|c| c.name.clone()).collect();
            sync_pending = starorg_core::starorg::mark_pending(&c.dirs, &names).is_ok();
        }
    }
    let Ok(src) = source(g, &c) else {
        return soft(g, kind, "cannot find own binary");
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    let opts = ApplyOpts {
        if_stale: true,
        dry_run: false,
        budget: Budget::with_timeout(remaining),
        profile_file: None,
        character: None,
        // Datasets are cached after the first apply of a build; a first extraction that
        // would outrun the hook's budget is stopped and only its lines are left out.
        obtain: crate::obtain_src::provider(g, &i, &c.dirs, Some(remaining)),
    };
    let names: Vec<String> = i.channels.iter().map(|c| c.name.clone()).collect();
    let (reports, skipped) = match apply_channels(g, &c, &i, &id, &names, true, &opts, src.as_ref())
    {
        Ok(x) => x,
        Err(_) => return soft(g, kind, "channel lookup failed"),
    };
    let command = format!("hook {}", kind.as_str());
    let code = if g.json {
        // One object: the apply report plus the scans.
        let code = exit::worst(reports.iter().map(|r| r.code));
        let mut channels: Vec<Value> = reports
            .iter()
            .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
            .collect();
        for (ch, why) in &skipped {
            channels.push(json!({ "channel": ch, "result": "needs-setup", "reason": why }));
        }
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": command, "exit_code": code,
                    "channels": channels, "scans": scans, "sync_pending": sync_pending,
                    "duration_ms": started.elapsed().as_millis() as u64 })
        );
        code
    } else {
        print_reports(g, &command, &reports, &[])
    };
    let applied = reports.iter().any(|r| r.result == Outcome::Applied);
    notify(g, code, &summary(&reports), applied);
    if code != exit::OK && !g.quiet {
        let what = match code {
            exit::SKIPPED => "skipped (busy or out of time)",
            exit::WARNINGS => "applied with warnings",
            exit::NEEDS_DECISION => "needs a decision (see above)",
            _ => "apply failed",
        };
        eprintln!("starorg-codex {command}: {what} (non-fatal)");
    }
    ExitCode::from(exit::OK)
}

/// Where the scripts are and what must never be edited.
struct Places {
    prefix: Option<PathBuf>,
    forbidden: Vec<PathBuf>,
    game_dir: Option<String>,
}

fn places(g: &GlobalOpts) -> Places {
    let d = run_discovery(g);
    let inst = d.installs.into_iter().next();
    let prefix = g
        .prefix
        .clone()
        .or_else(|| {
            std::env::var_os("WINEPREFIX")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| inst.as_ref().and_then(|i| i.prefix.clone()))
        .or_else(|| {
            inst.as_ref()
                .and_then(|i| starorg_core::running::enclosing_prefix(&i.root))
        });
    let mut forbidden = Vec::new();
    if let Some(i) = &inst {
        forbidden.push(i.root.clone());
    }
    if let Some(p) = &prefix {
        forbidden.push(p.join("drive_c"));
    }
    let game_dir = g
        .game_dir
        .as_ref()
        .map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone()))
        .map(|p| p.to_string_lossy().into_owned());
    Places {
        prefix,
        forbidden,
        game_dir,
    }
}

fn script_path(kind: Kind, s: &HookScripts, p: &Places) -> Option<PathBuf> {
    let given = match kind {
        Kind::Prelaunch => s.prelaunch_script.clone(),
        Kind::Postexit => s.postexit_script.clone(),
    };
    given.or_else(|| p.prefix.as_ref().map(|x| x.join(kind.script_name())))
}

fn block_opts(g: &GlobalOpts, bin: Option<&Path>, p: &Places) -> BlockOpts {
    BlockOpts {
        bin: bin.map(|b| {
            std::path::absolute(b)
                .unwrap_or_else(|_| b.to_path_buf())
                .to_string_lossy()
                .into_owned()
        }),
        game_dir: p.game_dir.clone(),
        timeout: None,
        notify: g.notify,
    }
}

fn on_path() -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(hooks::CLI).is_file()))
}

fn edit(g: &GlobalOpts, s: &HookScripts, bin: Option<&Path>, install_: bool) -> ExitCode {
    if cfg!(windows) {
        return fail(
            g,
            exit::NOT_FOUND,
            "unsupported",
            "LUG Helper hooks are Linux only; on Windows run `apply --all --if-stale` yourself",
        );
    }
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let p = places(g);
    let bo = block_opts(g, bin, &p);
    let backup_dir = c.dirs.data.join("hooks").join("backups");
    let mut results = Vec::new();
    let mut codes = Vec::new();
    for kind in Kind::ALL {
        let Some(path) = script_path(kind, s, &p) else {
            codes.push(exit::NOT_FOUND);
            results.push(json!({ "kind": kind, "result": "not-found",
                "reason": "no Wine prefix; give --prefix or --prelaunch-script/--postexit-script" }));
            continue;
        };
        let script = match hooks::locate(kind, &path, &p.forbidden) {
            Ok(x) => x,
            Err((missing, why)) => {
                codes.push(if missing {
                    exit::NOT_FOUND
                } else {
                    exit::ERROR
                });
                results.push(json!({ "kind": kind, "path": path,
                    "result": if missing { "not-found" } else { "refused" }, "reason": why }));
                continue;
            }
        };
        let text = match hooks::read(&script) {
            Ok(t) => t,
            Err(e) => {
                codes.push(exit::ERROR);
                results
                    .push(json!({ "kind": kind, "path": path, "result": "failed", "reason": e }));
                continue;
            }
        };
        let want = hooks::block(kind, &bo);
        let state = hooks::inspect(&text, &want);
        let new = if install_ {
            match state {
                BlockState::Current => None,
                _ => Some(hooks::install(&text, &want)),
            }
        } else {
            match state {
                BlockState::Absent => None,
                _ => Some(hooks::uninstall(&text).map(|o| o.unwrap_or_else(|| text.clone()))),
            }
        };
        let mut res =
            json!({ "kind": kind, "path": path, "real": script.real, "link": script.is_link });
        match new {
            None => {
                res["result"] = json!(if install_ {
                    "already-installed"
                } else {
                    "not-installed"
                });
                codes.push(exit::OK);
            }
            Some(Err(e)) => {
                res["result"] = json!("refused");
                res["reason"] = json!(e);
                codes.push(exit::ERROR);
            }
            Some(Ok(new)) => match hooks::write(&script, &text, &new, &backup_dir) {
                Ok(b) => {
                    res["result"] = json!(match (install_, &state) {
                        (true, BlockState::Outdated) => "updated",
                        (true, _) => "installed",
                        (false, _) => "removed",
                    });
                    res["backup"] = json!(b);
                    codes.push(exit::OK);
                }
                Err(e) => {
                    res["result"] = json!("failed");
                    res["reason"] = json!(e);
                    codes.push(exit::ERROR);
                }
            },
        }
        if install_ {
            let after = hooks::read(&script).unwrap_or_default();
            let manual = hooks::manual_calls(&after);
            if !manual.is_empty() {
                res["warning"] = json!(format!(
                    "also calls starorg-codex outside the block (line {}); it would run twice",
                    manual
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            let others = hooks::other_tools(&after);
            if !others.is_empty() {
                res["other_tools"] = json!(others);
            }
        }
        results.push(res);
    }
    // All missing: 4. Any error: 1. Otherwise 0 (one missing script is reported, not fatal).
    let code = if codes.iter().all(|c| *c == exit::NOT_FOUND) {
        exit::NOT_FOUND
    } else if codes.contains(&exit::ERROR) {
        exit::ERROR
    } else {
        exit::OK
    };
    let command = if install_ {
        "hook install"
    } else {
        "hook uninstall"
    };
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": command, "exit_code": code, "scripts": results })
        );
    } else if !g.quiet {
        for r in &results {
            print_result(r);
        }
        if install_ && bin.is_none() && !on_path() {
            println!(
                "note: starorg-codex is not on PATH; the block does nothing until it is (or use --bin)"
            );
        }
    }
    ExitCode::from(code)
}

fn print_result(r: &Value) {
    let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("");
    let path = r.get("path").and_then(Value::as_str).unwrap_or("(no path)");
    let mut line = format!("{}: {} {path}", s("kind"), s("result"));
    if r.get("link").and_then(Value::as_bool) == Some(true) {
        line.push_str(&format!(" (link to {})", s("real")));
    }
    println!("{line}");
    if !s("reason").is_empty() {
        println!("  {}", s("reason"));
    }
    if !s("backup").is_empty() {
        println!("  backup: {}", s("backup"));
    }
    if !s("warning").is_empty() {
        println!("  warning: {}", s("warning"));
    }
    if let Some(o) = r.get("other_tools").and_then(Value::as_array) {
        let names: Vec<&str> = o.iter().filter_map(Value::as_str).collect();
        println!("  also runs: {}", names.join(", "));
    }
}

fn status(g: &GlobalOpts, s: &HookScripts, bin: Option<&Path>) -> ExitCode {
    let p = places(g);
    let bo = block_opts(g, bin, &p);
    let mut results = Vec::new();
    let mut found = 0;
    for kind in Kind::ALL {
        let Some(path) = script_path(kind, s, &p) else {
            results.push(json!({ "kind": kind, "result": "not-found" }));
            continue;
        };
        let script = match hooks::locate(kind, &path, &p.forbidden) {
            Ok(x) => x,
            Err((missing, why)) => {
                results.push(json!({ "kind": kind, "path": path,
                    "result": if missing { "not-found" } else { "refused" }, "reason": why }));
                continue;
            }
        };
        found += 1;
        let text = hooks::read(&script).unwrap_or_default();
        let st = hooks::inspect(&text, &hooks::block(kind, &bo));
        let result = match &st {
            BlockState::Absent => "not-installed",
            BlockState::Current => "installed",
            BlockState::Outdated => "outdated",
            BlockState::Broken { .. } => "broken",
        };
        let mut res = json!({ "kind": kind, "path": path, "real": script.real,
            "link": script.is_link, "result": result });
        if let BlockState::Broken { why } = st {
            res["reason"] = json!(why);
        } else if st == BlockState::Outdated && hooks::has_legacy_markers(&text) {
            res["reason"] = json!(
                "written by starorg-strings (before 0.3.0); `starorg-codex hook install` replaces it in place"
            );
        }
        let manual = hooks::manual_calls(&text);
        if !manual.is_empty() {
            res["warning"] = json!(format!(
                "calls starorg-codex outside the block (line {})",
                manual
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let others = hooks::other_tools(&text);
        if !others.is_empty() {
            res["other_tools"] = json!(others);
        }
        results.push(res);
    }
    let id_known = ctx(g)
        .ok()
        .zip(install(g).ok())
        .and_then(|(c, i)| lookup(&c, &i))
        .is_some();
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "hook status", "on_path": on_path(),
                    "install_registered": id_known, "scripts": results })
        );
    } else if !g.quiet {
        for r in &results {
            print_result(r);
        }
        println!(
            "starorg-codex on PATH: {}",
            if on_path() { "yes" } else { "no" }
        );
    }
    ExitCode::from(if found == 0 {
        exit::NOT_FOUND
    } else {
        exit::OK
    })
}
