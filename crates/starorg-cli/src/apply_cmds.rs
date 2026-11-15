//! `setup`, `apply`, `status`, `remove`, `restore`, `backups`, `patches`, `doctor` (spec 8).

use crate::commands::{JSON_SCHEMA, fail, run_discovery};
use crate::{ChannelSel, GlobalOpts, exit};
use serde_json::{Value, json};
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::{self, ApplyOpts, Ctx, Outcome, Report, State, Target};
use starorg_core::backup;
use starorg_core::cache::{self, KeyInputs};
use starorg_core::config::{self, Config, IdOrigin, Outputs};
use starorg_core::discovery::{Channel, Install};
use starorg_core::fsutil;
use starorg_core::ini::StockIni;
use starorg_core::merge;
use starorg_core::model::GameData;
use starorg_core::patch;
use starorg_core::paths::resolve_ci;
use starorg_core::source::local::LocalExtraction;
use starorg_core::source::{Budget, GameSource, Loaded, Origin, SourceError};
use starorg_core::state::{ChannelState, INTENT_JSON, Profile};
use starorg_core::usercfg;
use starorg_data::starbreaker::STARBREAKER_COMMIT;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

/// Test source: a stock file on disk instead of an extraction (hidden `--test-stock-ini`).
struct FileSource(PathBuf);

impl GameSource for FileSource {
    fn load(&self, _: &Channel, _: &Budget) -> Result<Loaded, SourceError> {
        Ok(Loaded {
            base_ini: std::fs::read(&self.0)?,
            game_data: GameData::default(),
            origin: Origin::LocalExtraction {
                starbreaker_commit: "test-stock-ini".into(),
            },
            warnings: Vec::new(),
        })
    }
}

pub(crate) fn ctx(g: &GlobalOpts) -> Result<Ctx, ExitCode> {
    let Some(dirs) = AppDirs::from_env() else {
        return Err(fail(
            g,
            exit::ERROR,
            "error",
            "cannot find the home directory for the app folders",
        ));
    };
    let mut c = Ctx::new(dirs, env!("CARGO_PKG_VERSION"), STARBREAKER_COMMIT);
    c.use_cache = !g.no_cache && g.test_stock_ini().is_none();
    c.pause_after_write = g.test_pause_after_write_ms().map(Duration::from_millis);
    c.pause_before_cfg = g.test_pause_before_cfg_ms().map(Duration::from_millis);
    Ok(c)
}

pub(crate) fn source(g: &GlobalOpts, c: &Ctx) -> Result<Box<dyn GameSource>, ExitCode> {
    if let Some(p) = g.test_stock_ini() {
        return Ok(Box::new(FileSource(p.to_path_buf())));
    }
    let exe = std::env::current_exe().map_err(|e| {
        fail(
            g,
            exit::ERROR,
            "error",
            &format!("cannot find own binary: {e}"),
        )
    })?;
    let mut s = LocalExtraction::new(exe);
    s.work_root = Some(c.dirs.cache.join("work"));
    Ok(Box::new(s))
}

pub(crate) fn install(g: &GlobalOpts) -> Result<Install, ExitCode> {
    run_discovery(g).installs.into_iter().next().ok_or_else(|| {
        fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found (try --game-dir or --prefix)",
        )
    })
}

pub(crate) fn find_channel<'a>(
    g: &GlobalOpts,
    i: &'a Install,
    name: &str,
) -> Result<&'a Channel, ExitCode> {
    i.channel(name).ok_or_else(|| {
        let names: Vec<_> = i.channels.iter().map(|c| c.name.as_str()).collect();
        fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!(
                "channel {name} not found in {} (have: {})",
                i.root.display(),
                names.join(", ")
            ),
        )
    })
}

/// Register the install (commands that write) and return its id.
pub(crate) fn register(g: &GlobalOpts, c: &Ctx, i: &Install) -> Result<String, ExitCode> {
    let mut cfg =
        Config::load(&c.dirs).map_err(|e| fail(g, exit::ERROR, "config", &e.to_string()))?;
    let outputs = Outputs::load(&c.dirs).unwrap_or_default();
    let (id, origin) = config::register_install(&c.dirs, &mut cfg, &outputs, i)
        .map_err(|e| fail(g, exit::ERROR, "config", &e.to_string()))?;
    if let IdOrigin::Relinked { old_path } = origin
        && !g.quiet
    {
        eprintln!(
            "starorg-strings: re-linked moved install {} -> {}",
            old_path.display(),
            i.root.display()
        );
    }
    Ok(id)
}

/// Id of an install without writing anything.
pub(crate) fn lookup(c: &Ctx, i: &Install) -> Option<String> {
    let cfg = Config::load(&c.dirs).ok()?;
    let outputs = Outputs::load(&c.dirs).unwrap_or_default();
    config::lookup_install(&cfg, &outputs, i).map(|(id, _)| id)
}

pub(crate) fn notify(g: &GlobalOpts, code: u8, summary: &str, applied_something: bool) {
    if !g.notify || cfg!(windows) {
        return;
    }
    let send = matches!(code, 1 | 5 | 6) || (code == 0 && applied_something);
    if !send {
        return;
    }
    let urgency = if code == 1 { "critical" } else { "normal" };
    let r = std::process::Command::new("notify-send")
        .args([
            "--app-name",
            "StarOrg Strings",
            "--urgency",
            urgency,
            "StarOrg Strings",
            summary,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if r.is_err() && !g.quiet {
        eprintln!("starorg-strings: notify-send not available; {summary}");
    }
}

pub(crate) fn print_reports(
    g: &GlobalOpts,
    command: &str,
    reports: &[Report],
    skipped: &[(String, String)],
) -> u8 {
    let code = exit::worst(reports.iter().map(|r| r.code));
    if g.json {
        let mut channels: Vec<Value> = reports
            .iter()
            .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
            .collect();
        for (ch, why) in skipped {
            channels.push(json!({ "channel": ch, "result": "needs-setup", "reason": why }));
        }
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": command, "exit_code": code, "channels": channels })
        );
        return code;
    }
    for r in reports {
        let result = serde_json::to_value(r.result).ok();
        let result = result.as_ref().and_then(Value::as_str).unwrap_or("?");
        if r.code != exit::OK || !g.quiet {
            let mut line = format!("{}: {result}", r.channel);
            if let Some(why) = &r.reason {
                line.push_str(&format!(" ({why})"));
            }
            if r.code == exit::OK || r.code == exit::WARNINGS {
                println!("{line}");
            } else {
                eprintln!("{line}");
            }
        }
        if g.quiet {
            continue;
        }
        if let Some(m) = &r.recovered {
            println!("  recovered: {m}");
        }
        if let Some(t) = &r.target {
            println!(
                "  global.ini: {}{}",
                t.display(),
                if r.target_is_link {
                    " (through a link)"
                } else {
                    ""
                }
            );
        }
        if matches!(r.result, Outcome::Applied | Outcome::DryRun) {
            println!(
                "  changed {} keys ({} overrides, {} from patches), {} dropped",
                r.counts["changed"],
                r.counts["overrides"],
                r.counts["loc_patches"],
                r.counts["dropped"]
            );
            for p in &r.patches {
                println!("  patch {}: {}", p.id, p.state);
            }
        }
        if let Some(a) = &r.user_cfg.action {
            let mut s = format!("  user.cfg: {a}");
            if let Some(l) = &r.user_cfg.link_target {
                s.push_str(&format!("; link to {}", l.display()));
            }
            if !r.user_cfg.shared_with.is_empty() {
                s.push_str(&format!(
                    "; shared with {}",
                    r.user_cfg.shared_with.join(", ")
                ));
            }
            println!("{s}");
        }
        if let Some(b) = &r.backup {
            println!("  backup: {b}");
        }
        if let Some(o) = r.owned.as_ref().filter(|o| o.count > 0)
            && matches!(r.result, Outcome::Applied | Outcome::DryRun)
        {
            println!("  owned blueprints: {} ({})", o.count, o.mode);
        }
        for w in &r.warnings {
            eprintln!("  warning: {}: {}", w.kind, w.detail);
        }
    }
    for (ch, why) in skipped {
        if !g.quiet {
            println!("{ch}: skipped ({why})");
        }
    }
    code
}

pub(crate) fn summary(reports: &[Report]) -> String {
    reports
        .iter()
        .map(|r| {
            let res = serde_json::to_value(r.result).ok();
            format!(
                "{}: {}",
                r.channel,
                res.as_ref().and_then(Value::as_str).unwrap_or("?")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn is_set_up(c: &Ctx, id: &str, ch: &Channel) -> bool {
    ChannelState::load(&c.dirs.channel_dir(id, &ch.name))
        .map(|s| s.consent)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------------------

pub fn setup(
    g: &GlobalOpts,
    channel: &str,
    preset: Option<&str>,
    copy_from: Option<&str>,
    switch_language: bool,
    yes: bool,
) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = match install(g) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let ch = match find_channel(g, &i, channel) {
        Ok(ch) => ch,
        Err(e) => return e,
    };
    let lang = usercfg::locate(&ch.dir)
        .ok()
        .and_then(|t| usercfg::read(&t).ok().map(|x| (t, x)))
        .map(|(_, text)| match text {
            None => usercfg::LanguageLine::NoFile,
            Some(t) => usercfg::inspect(&t),
        });
    let lang_note = match &lang {
        Some(usercfg::LanguageLine::Other { value, .. }) if !switch_language => Some(format!(
            "user.cfg sets g_language = {value}; apply will ask for --switch-language (exit 6)"
        )),
        Some(usercfg::LanguageLine::Other { value, .. }) => Some(format!(
            "user.cfg g_language = {value} will be switched to english"
        )),
        Some(usercfg::LanguageLine::English { .. }) => None,
        _ => Some("user.cfg will get the line g_language = english".into()),
    };
    if !yes {
        let msg = format!(
            "setup writes nothing without --yes. It would allow StarOrg Strings to back up and replace {}'s loose global.ini{}",
            ch.name,
            lang_note
                .as_deref()
                .map(|n| format!("; {n}"))
                .unwrap_or_default()
        );
        return fail(g, exit::NEEDS_DECISION, "needs-confirmation", &msg);
    }
    let id = match register(g, &c, &i) {
        Ok(id) => id,
        Err(e) => return e,
    };
    let data = c.dirs.channel_dir(&id, &ch.name);
    let profile = match (preset, copy_from) {
        (Some(p), _) => Profile::preset(p),
        (None, Some(from)) => {
            let Some(src) = i.channel(from) else {
                return fail(
                    g,
                    exit::USAGE,
                    "usage",
                    &format!("--copy-from {from}: no such channel"),
                );
            };
            let path = Profile::path(&c.dirs.channel_dir(&id, &src.name));
            if !path.exists() {
                return fail(
                    g,
                    exit::USAGE,
                    "usage",
                    &format!("--copy-from {from}: that channel has no profile (set it up first)"),
                );
            }
            Profile::load_file(&path).ok()
        }
        (None, None) => None,
    };
    let Some(profile) = profile else {
        return fail(g, exit::USAGE, "usage", "give --preset or --copy-from");
    };
    let mut st = ChannelState::load(&data).unwrap_or_default();
    st.consent = true;
    st.setup_time = Some(fsutil::rfc3339(fsutil::unix_now()));
    st.switch_language = switch_language;
    if let Err(e) = st.save(&data).and_then(|()| profile.save(&data)) {
        return fail(g, exit::ERROR, "io", &e.to_string());
    }
    let mut cfg = Config::load(&c.dirs).unwrap_or_default();
    if cfg.default_channel.is_none() {
        cfg.default_channel = Some(ch.name.clone());
        let _ = cfg.save(&c.dirs);
    }
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "channel": ch.name, "result": "set-up", "install_id": id,
                    "switch_language": switch_language, "note": lang_note })
        );
    } else if !g.quiet {
        println!("{} is set up (install id {id}).", ch.name);
        if let Some(n) = lang_note {
            println!("  {n}");
        }
    }
    ExitCode::from(exit::OK)
}

pub fn apply(
    g: &GlobalOpts,
    sel: &ChannelSel,
    if_stale: bool,
    dry_run: bool,
    profile: Option<PathBuf>,
    timeout: Option<u64>,
    character: Option<String>,
) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = match install(g) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let names: Vec<String> = if sel.all {
        i.channels.iter().map(|c| c.name.clone()).collect()
    } else if let Some(ch) = &sel.channel {
        vec![ch.clone()]
    } else {
        match Config::load(&c.dirs).ok().and_then(|c| c.default_channel) {
            Some(d) => vec![d],
            None => {
                return fail(
                    g,
                    exit::USAGE,
                    "usage",
                    "give --channel or --all (no default channel is configured)",
                );
            }
        }
    };
    let id = match register(g, &c, &i) {
        Ok(id) => id,
        Err(e) => return e,
    };
    let src = match source(g, &c) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let opts = ApplyOpts {
        if_stale,
        dry_run,
        budget: timeout
            .map(|s| Budget::with_timeout(Duration::from_secs(s)))
            .unwrap_or_default(),
        profile_file: profile,
        character,
    };
    let (reports, skipped) =
        match apply_channels(g, &c, &i, &id, &names, sel.all, &opts, src.as_ref()) {
            Ok(x) => x,
            Err(e) => return e,
        };
    let code = print_reports(g, "apply", &reports, &skipped);
    let applied = reports.iter().any(|r| r.result == Outcome::Applied);
    notify(g, code, &summary(&reports), applied);
    ExitCode::from(code)
}

/// Apply reports, and the channels `--all` skipped with the reason.
pub(crate) type Applied = (Vec<Report>, Vec<(String, String)>);

/// Apply `names`; with `all`, channels that are not set up are skipped and listed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_channels(
    g: &GlobalOpts,
    c: &Ctx,
    i: &Install,
    id: &str,
    names: &[String],
    all: bool,
    opts: &ApplyOpts,
    src: &dyn GameSource,
) -> Result<Applied, ExitCode> {
    let mut reports = Vec::new();
    let mut skipped = Vec::new();
    for name in names {
        let ch = find_channel(g, i, name)?;
        if all && !is_set_up(c, id, ch) {
            skipped.push((ch.name.clone(), "not set up".to_string()));
            continue;
        }
        let t = Target::new(i, Some(id.to_string()), ch);
        reports.push(apply::apply(c, &t, opts, src));
    }
    Ok((reports, skipped))
}

pub fn remove(g: &GlobalOpts, sel: &ChannelSel, yes: bool) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = match install(g) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let names: Vec<String> = if sel.all {
        i.channels.iter().map(|c| c.name.clone()).collect()
    } else if let Some(ch) = &sel.channel {
        vec![ch.clone()]
    } else {
        return fail(g, exit::USAGE, "usage", "give --channel or --all");
    };
    let id = lookup(&c, &i);
    let mut reports = Vec::new();
    for name in &names {
        let ch = match find_channel(g, &i, name) {
            Ok(ch) => ch,
            Err(e) => return e,
        };
        let t = Target::new(&i, id.clone(), ch);
        reports.push(apply::remove(&c, &t, yes));
    }
    let code = print_reports(g, "remove", &reports, &[]);
    notify(g, code, &summary(&reports), false);
    ExitCode::from(code)
}

pub fn restore(g: &GlobalOpts, channel: &str, backup_id: &str, allow: bool) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = match install(g) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let ch = match find_channel(g, &i, channel) {
        Ok(ch) => ch,
        Err(e) => return e,
    };
    let t = Target::new(&i, lookup(&c, &i), ch);
    let r = apply::restore(&c, &t, backup_id, allow);
    ExitCode::from(print_reports(g, "restore", &[r], &[]))
}

pub fn status(g: &GlobalOpts, channel: Option<&str>) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = match install(g) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let id = lookup(&c, &i);
    let chans: Vec<&Channel> = match channel {
        Some(n) => match find_channel(g, &i, n) {
            Ok(ch) => vec![ch],
            Err(e) => return e,
        },
        None => i.channels.iter().collect(),
    };
    let mut out = Vec::new();
    for ch in chans {
        let t = Target::new(&i, id.clone(), ch);
        // "On the next run of any command for that channel" (spec 7.1, journal).
        match apply::recover_if_idle(&c, &t) {
            Some(Ok(Some(m))) if !g.quiet && !g.json => eprintln!("{}: {m}", ch.name),
            Some(Err(e)) => eprintln!("{}: recovery failed: {e}", ch.name),
            _ => {}
        }
        let st =
            apply::current_inputs(&c, &t, None, None).and_then(|inp| apply::status(&c, &t, &inp));
        match st {
            Ok(s) => out.push(s),
            Err(e) => return fail(g, exit::ERROR, "status", &format!("{}: {e}", ch.name)),
        }
    }
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "install": i.root, "install_id": id, "channels": out })
        );
    } else if !g.quiet {
        for s in &out {
            let state = match &s.state {
                State::Applied => "Applied".to_string(),
                State::Stale { reasons } => format!("Stale ({} changed)", reasons.join(", ")),
                State::ChangedByAnotherTool { tool } => format!(
                    "Changed by another tool{}",
                    tool.as_deref()
                        .map(|t| format!(" ({t})"))
                        .unwrap_or_default()
                ),
                State::NotApplied => "Not applied".to_string(),
            };
            println!(
                "{:<8} {:<40} {}{}",
                s.channel,
                state,
                if s.set_up { "set up" } else { "not set up" },
                if s.interrupted_apply {
                    ", interrupted apply pending"
                } else {
                    ""
                }
            );
        }
    }
    ExitCode::from(exit::OK)
}

pub fn backups(g: &GlobalOpts, channel: &str) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = match install(g) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let ch = match find_channel(g, &i, channel) {
        Ok(ch) => ch,
        Err(e) => return e,
    };
    let list = match lookup(&c, &i) {
        Some(id) => backup::list(&c.dirs.channel_dir(&id, &ch.name)),
        None => Vec::new(),
    };
    if g.json {
        let items: Vec<Value> = list
            .iter()
            .map(|b| json!({ "id": b.id, "pinned": b.pinned(), "manifest": b.manifest }))
            .collect();
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "channel": ch.name, "backups": items })
        );
    } else if !g.quiet {
        if list.is_empty() {
            println!("{}: no backups", ch.name);
        }
        for b in list.iter().rev() {
            let f = &b.manifest.file;
            let origin = serde_json::to_value(f.origin).ok();
            println!(
                "{:<36} change {:<10} {}{}{}",
                b.id,
                b.manifest
                    .build
                    .change
                    .map_or("unknown".into(), |c| c.to_string()),
                if f.existed {
                    format!(
                        "{} bytes, {}",
                        f.size.unwrap_or(0),
                        origin.as_ref().and_then(Value::as_str).unwrap_or("?")
                    )
                } else {
                    "no file (stock)".to_string()
                },
                f.foreign_tool
                    .as_deref()
                    .map(|t| format!(" ({t})"))
                    .unwrap_or_default(),
                if b.pinned() { " [pinned]" } else { "" }
            );
        }
    }
    ExitCode::from(exit::OK)
}

pub fn patches(g: &GlobalOpts, channel: Option<&str>) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    // Without a channel: list the patch files only.
    let (set, outcomes, note) = match channel {
        None => (patch::load_patches(&c.dirs, &[]), None, None),
        Some(name) => {
            let i = match install(g) {
                Ok(i) => i,
                Err(e) => return e,
            };
            let ch = match find_channel(g, &i, name) {
                Ok(ch) => ch,
                Err(e) => return e,
            };
            let t = Target::new(&i, lookup(&c, &i), ch);
            let inputs = match apply::current_inputs(&c, &t, None, None) {
                Ok(x) => x,
                Err(e) => return fail(g, exit::ERROR, "patches", &e),
            };
            // Outcomes need the stock text: only from a verified cache entry, never by
            // extracting here.
            let cached = t
                .install_id
                .as_ref()
                .and(inputs.fingerprint.as_ref())
                .and_then(|fp| {
                    let id = t.install_id.as_deref()?;
                    let k = KeyInputs::local(fp, STARBREAKER_COMMIT);
                    cache::load(&c.dirs.channel_cache(id, &ch.name), &k, c.min_stock_keys).ok()
                });
            match cached {
                Some(l) => {
                    let stock = StockIni::parse(l.base_ini);
                    let m = merge::merge(
                        &stock,
                        &l.game_data,
                        ch.build.as_ref(),
                        &[],
                        &inputs.patches,
                        &inputs.overrides,
                    );
                    (inputs.patches, Some(m.patch_outcomes), None)
                }
                None => (
                    inputs.patches,
                    None,
                    Some(
                        "no cached extraction for this build; run apply first to see outcomes"
                            .to_string(),
                    ),
                ),
            }
        }
    };
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "patches": set.patches.iter().map(|p| json!({
                "id": p.file.id, "title": p.file.title, "origin": p.origin, "enabled": p.enabled,
                "builds": p.file.builds, "data_entries": p.file.data.len(), "loc_entries": p.file.loc.len(),
            })).collect::<Vec<_>>(), "errors": set.errors, "outcomes": outcomes, "note": note })
        );
    } else if !g.quiet {
        if set.patches.is_empty() {
            println!(
                "No patches (built-in patches arrive with the M2 data mapping; user patches go in {}).",
                c.dirs.user_patches().display()
            );
        }
        for p in &set.patches {
            println!(
                "{:<32} {:<8} {}{}",
                p.file.id,
                if p.enabled { "enabled" } else { "disabled" },
                p.file.title,
                if matches!(p.origin, patch::PatchOrigin::Builtin) {
                    " (built-in)"
                } else {
                    ""
                }
            );
            for o in outcomes.iter().flatten().filter(|o| o.patch == p.file.id) {
                let s = serde_json::to_value(o.state).ok();
                println!(
                    "    {:<24} {}",
                    o.entry,
                    s.as_ref().and_then(Value::as_str).unwrap_or("?")
                );
            }
        }
        for e in &set.errors {
            eprintln!("error in {}: {}", e.name, e.error);
        }
        if let Some(n) = note {
            println!("{n}");
        }
    }
    ExitCode::from(exit::OK)
}

fn bom_and_endings(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    let ini = StockIni::parse(bytes.clone());
    Some(json!({
        "bytes": bytes.len(),
        "bom": ini.has_bom(),
        "lines": ini.line_count(),
        "crlf_lines": ini.crlf_count(),
        "keys": ini.key_count(),
        "flagged_keys": ini.flagged_key_count(),
        "sha256": fsutil::sha256_hex(&bytes),
    }))
}

fn link_info(p: &Path) -> Value {
    let link = std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    json!({
        "path": p,
        "exists": p.exists(),
        "is_link": link,
        "real": std::fs::canonicalize(p).ok(),
    })
}

/// Redacted owned-blueprint summary for `doctor`.
fn blueprint_summary(data: &Path) -> Value {
    use starorg_core::blueprints;
    let chars = blueprints::load_characters(data).unwrap_or_default();
    let owned: usize = chars
        .characters
        .keys()
        .filter_map(|g| blueprints::load_set(data, g).ok())
        .map(|s| s.entries.len())
        .sum();
    let setting = ChannelState::load(data)
        .ok()
        .and_then(|s| s.active_character);
    json!({
        "characters": chars.characters.len(),
        "owned_entries": owned,
        "unattributed": blueprints::load_unattributed(data).map(|s| s.entries.len()).unwrap_or(0),
        "active": match setting.as_deref() {
            None => "auto",
            Some("any") => "any",
            Some(_) => "character (redacted)",
        },
    })
}

pub fn doctor(g: &GlobalOpts) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let d = run_discovery(g);
    let mut installs = Vec::new();
    for i in &d.installs {
        let id = lookup(&c, i);
        let mut chans = Vec::new();
        for ch in &i.channels {
            let t = Target::new(i, id.clone(), ch);
            let ini = resolve_ci(&ch.dir, config::LOOSE_INI);
            let ini_json = match &ini {
                Ok(r) => {
                    let mut v = link_info(&r.path);
                    if r.exists() {
                        v["file"] = bom_and_endings(&r.path).unwrap_or(Value::Null);
                    }
                    v
                }
                Err(e) => json!({ "error": e.to_string() }),
            };
            let cfg = usercfg::locate(&ch.dir);
            let cfg_json = match &cfg {
                Ok(t) => {
                    let mut v = link_info(&t.path);
                    v["language"] = usercfg::read(t)
                        .ok()
                        .map(|x| match x {
                            None => json!({ "state": "no-file" }),
                            Some(text) => {
                                serde_json::to_value(usercfg::inspect(&text)).unwrap_or(Value::Null)
                            }
                        })
                        .unwrap_or(Value::Null);
                    v
                }
                Err(e) => json!({ "error": e.to_string() }),
            };
            let probe_dir = ini.as_ref().ok().and_then(|r| {
                fsutil::canonical_lenient(&r.path)
                    .parent()
                    .map(Path::to_path_buf)
            });
            let writable = probe_dir.as_deref().map(|dir| {
                let mut d = dir;
                while !d.is_dir() {
                    match d.parent() {
                        Some(p) => d = p,
                        None => return false,
                    }
                }
                std::fs::metadata(d)
                    .map(|m| !m.permissions().readonly())
                    .unwrap_or(false)
            });
            let mut traces = Vec::new();
            if let Ok(r) = &ini
                && r.exists()
                && let Ok(b) = std::fs::read(&r.path)
                && let Some(tool) = apply::foreign_tool(&b, &ch.dir)
                && !Outputs::load(&c.dirs)
                    .unwrap_or_default()
                    .contains(&fsutil::sha256_hex(&b))
            {
                traces.push(format!("loose global.ini written by {tool}"));
            }
            if matches!(resolve_ci(&ch.dir, &[".starstrings_version"]), Ok(r) if r.exists()) {
                traces.push(".starstrings_version file".into());
            }
            for stray in ["Files", "Industries", "Space"] {
                if matches!(resolve_ci(&ch.dir, &[stray]), Ok(r) if r.exists() && r.path.is_dir()) {
                    traces.push(format!(
                        "stray folder {stray}/ (old updater split a path at spaces)"
                    ));
                }
            }
            if let Ok(r) = resolve_ci(&ch.dir, &["Data", "languages.ini"])
                && r.exists()
                && std::fs::metadata(&r.path)
                    .map(|m| m.len() == 0)
                    .unwrap_or(false)
            {
                traces.push(
                    "empty Data/languages.ini (another tool copied it; the launcher's Verify files restores it)".into(),
                );
            }
            let interrupted = id
                .as_ref()
                .is_some_and(|_| t.data_dir(&c.dirs).join(INTENT_JSON).exists());
            let status = apply::current_inputs(&c, &t, None, None)
                .and_then(|inp| apply::status(&c, &t, &inp))
                .ok()
                .map(|s| serde_json::to_value(&s.state).unwrap_or(Value::Null));
            chans.push(json!({
                "channel": ch.name,
                "set_up": id.as_ref().is_some_and(|id| is_set_up(&c, id, ch)),
                "status": status,
                "global_ini": ini_json,
                "target_folder_writable": writable,
                "user_cfg": cfg_json,
                "other_tool_traces": traces,
                "interrupted_apply": interrupted,
                "backups": id.as_ref().map(|_| backup::list(&t.data_dir(&c.dirs)).len()).unwrap_or(0),
                // Counts only: names, geids and account ids are personal data (addendum §3).
                "blueprints": id.as_ref().map(|_| blueprint_summary(&t.data_dir(&c.dirs))),
            }));
        }
        installs.push(json!({
            "root": i.root, "prefix": i.prefix, "found_by": i.found_by, "install_id": id,
            "channels": chans,
        }));
    }
    let set = patch::load_patches(&c.dirs, &[]);
    let report = json!({
        "schema": JSON_SCHEMA,
        "app_dirs": c.dirs,
        "elevated": fsutil::is_elevated(),
        "installs": installs,
        "patches": set.patches.iter().map(|p| json!({"id": p.file.id, "origin": p.origin})).collect::<Vec<_>>(),
        "patch_errors": set.errors,
        "gpu_session": "reported by the GUI (starorg-strings-app); the CLI needs no display",
    });
    if g.json {
        println!("{report}");
    } else if !g.quiet {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    }
    if d.installs.is_empty() {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found",
        );
    }
    ExitCode::from(exit::OK)
}
