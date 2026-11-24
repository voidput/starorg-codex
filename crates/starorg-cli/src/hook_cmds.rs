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
use crate::obtain_src::Obtain;
use crate::{GlobalOpts, HookCmd, HookScripts, exit};
use serde_json::{Value, json};
use starorg_core::apply::{ApplyOpts, OBTAIN_LATER, Outcome, Report};
use starorg_core::hooks::{self, BlockOpts, BlockState, Kind};
use starorg_core::source::Budget;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

pub fn run(g: &GlobalOpts, cmd: HookCmd) -> ExitCode {
    match cmd {
        HookCmd::Prelaunch { timeout } => launch_hook(g, Kind::Prelaunch, timeout),
        HookCmd::Postexit { timeout } => launch_hook(g, Kind::Postexit, timeout),
        HookCmd::Background { .. } => background_job(g),
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

/// Past the deadline by this much, the watchdog ends the hook whatever it is doing.
pub const HOOK_GRACE: Duration = Duration::from_secs(2);

/// The hook's priority as JSON: what `starorg_core::background` set for this process.
fn priority_json() -> Value {
    use starorg_core::background as bg;
    json!({ "class": bg::process_priority().as_str(), "nice": bg::current_nice(),
            "threads": bg::process_threads() })
}

/// End a hook that is still running at `deadline` + [`HOOK_GRACE`]: the usual soft report
/// (exit 0). Each phase checks the deadline on its own, and the workers stop themselves at
/// it; this catches whatever does not (a long in-process step), never cutting a write short
/// (`starorg_core::background::critical`). What was stopped is done on the next run: every
/// cache is written to a temp name and renamed into place.
fn arm_watchdog(g: &GlobalOpts, kind: Kind, started: Instant, deadline: Instant) {
    let json_out = g.json;
    starorg_core::background::watchdog(deadline, HOOK_GRACE, move || {
        let secs = started.elapsed().as_secs_f64();
        let msg =
            format!("out of time after {secs:.1} s; stopped, the rest is left for the next run");
        if json_out {
            println!(
                "{}",
                json!({ "schema": JSON_SCHEMA, "command": format!("hook {}", kind.as_str()),
                        "exit_code": exit::SKIPPED, "error": msg, "stopped": true,
                        "priority": priority_json(),
                        "duration_ms": started.elapsed().as_millis() as u64 })
            );
        }
        eprintln!("starorg-codex hook {}: {msg} (non-fatal)", kind.as_str());
        std::process::exit(i32::from(exit::OK));
    });
}

fn launch_hook(g: &GlobalOpts, kind: Kind, timeout: Option<u64>) -> ExitCode {
    let started = Instant::now();
    let budget = Duration::from_secs(timeout.unwrap_or(kind.default_timeout()));
    let deadline = started + budget;
    arm_watchdog(g, kind, started, deadline);
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
        // The game text first: WHERE TO GET lines only when already built. Building them
        // takes minutes on a new build: the post-exit background job does it, and the
        // output stays stale ("where to get") until they are in.
        obtain: crate::obtain_src::provider_as(g, &i, &c.dirs, Obtain::Stored),
    };
    let names: Vec<String> = i.channels.iter().map(|c| c.name.clone()).collect();
    let (reports, skipped) = match apply_channels(g, &c, &i, &id, &names, true, &opts, src.as_ref())
    {
        Ok(x) => x,
        Err(_) => return soft(g, kind, "channel lookup failed"),
    };
    // What the budget left for later goes to the background job (after exit only: before
    // launch the game is about to start).
    let background_job = kind == Kind::Postexit
        && reports.iter().any(left_for_later)
        && start_background_job(&c.dirs);
    // The report is written whole and once: the watchdog waits for it, then stands down.
    let reporting = starorg_core::background::critical();
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
                    "background_job": background_job, "priority": priority_json(),
                    "duration_ms": started.elapsed().as_millis() as u64 })
        );
        code
    } else {
        let code = print_reports(g, &command, &reports, &[]);
        if background_job && !g.quiet {
            eprintln!(
                "starorg-codex {command}: the rest continues in the background at low priority"
            );
        }
        code
    };
    starorg_core::background::disarm();
    drop(reporting);
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

/// Whether a hook left work for later in `r`: out of time, or WHERE TO GET lines not built.
fn left_for_later(r: &Report) -> bool {
    let late = |s: &str| s.contains("out of time") || s.contains(OBTAIN_LATER);
    (r.result == Outcome::Skipped && r.reason.as_deref().is_some_and(late))
        || r.warnings
            .iter()
            .any(|w| w.kind == "obtain-hints" && w.detail.contains(OBTAIN_LATER))
}

/// The background job's lock (one at a time) and log, in the cache folder.
const JOB_LOCK: &str = "hook-background.lock";
const JOB_LOG: &str = "hook-background.log";

/// Start `hook background` detached: this binary with this run's arguments (so it finds
/// the same install), at the lowest priority, in a process group of its own so the
/// launcher does not wait for it, output to the log in the cache folder.
fn start_background_job(dirs: &starorg_core::appdirs::AppDirs) -> bool {
    use starorg_core::background::{self as bg, Priority};
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let Some(hook) = args.iter().position(|a| a == "hook") else {
        return false;
    };
    let Some(post) = args[hook..].iter().position(|a| a == "postexit") else {
        return false;
    };
    args[hook + post] = "background".into();
    if std::fs::create_dir_all(&dirs.cache).is_err() {
        return false;
    }
    let Ok(log) = std::fs::File::create(dirs.cache.join(JOB_LOG)) else {
        return false;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    bg::prepare(&mut cmd, Priority::Background, None, true);
    bg::spawn(&mut cmd, Priority::Background).is_ok()
}

/// Whether the game runs for any channel of `i`.
fn game_running(i: &starorg_core::discovery::Install) -> bool {
    i.channels.iter().any(|ch| {
        starorg_core::running::game_running(i.prefix.as_deref(), &ch.name, &ch.dir).is_some()
    })
}

/// `hook background`: re-apply what is stale with no time budget, extracting what is
/// missing (game data of a new build, the WHERE TO GET datasets) at the lowest priority with
/// few threads. One job at a time; skipped while the game runs, and stopped (with its
/// workers) as soon as it starts: everything it writes is renamed into place whole, so the
/// next job or hook picks up from what was finished.
fn background_job(g: &GlobalOpts) -> ExitCode {
    let say = |m: &str| eprintln!("starorg-codex hook background: {m}");
    let Ok(c) = ctx(g) else {
        say("no app folders");
        return ExitCode::from(exit::OK);
    };
    let _ = std::fs::create_dir_all(&c.dirs.cache);
    let Ok(_one) = starorg_core::lock::try_lock(&c.dirs.cache.join(JOB_LOCK)) else {
        say("another background job is running");
        return ExitCode::from(exit::OK);
    };
    let Some(i) = run_discovery(g).installs.into_iter().next() else {
        say("no Star Citizen install found");
        return ExitCode::from(exit::OK);
    };
    if game_running(&i) {
        say("the game is running; left for the next exit");
        return ExitCode::from(exit::OK);
    }
    // Stop the moment the game starts: the whole group, workers included.
    let watched = i.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(5));
            if game_running(&watched) {
                eprintln!("starorg-codex hook background: the game started; stopping");
                starorg_core::background::stop_process_group();
            }
        }
    });
    let Ok(id) = register(g, &c, &i) else {
        say("cannot register the install");
        return ExitCode::from(exit::OK);
    };
    let Ok(src) = source(g, &c) else {
        say("cannot find own binary");
        return ExitCode::from(exit::OK);
    };
    let opts = ApplyOpts {
        if_stale: true,
        dry_run: false,
        budget: Budget::unlimited(),
        profile_file: None,
        character: None,
        obtain: crate::obtain_src::provider_as(g, &i, &c.dirs, Obtain::Full(None)),
    };
    let names: Vec<String> = i.channels.iter().map(|c| c.name.clone()).collect();
    match apply_channels(g, &c, &i, &id, &names, true, &opts, src.as_ref()) {
        Ok((reports, _)) => {
            print_reports(g, "hook background", &reports, &[]);
        }
        Err(_) => say("channel lookup failed"),
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
