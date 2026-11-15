//! `blueprints scan|characters|list|use|import|reimport|rescan` (addendum §3, §4).

use crate::apply_cmds::{ctx, find_channel, install, lookup, register, source};
use crate::commands::{JSON_SCHEMA, fail};
use crate::{BlueprintCmd, GlobalOpts, exit};
use serde_json::json;
use starorg_core::apply::{self, Ctx, Target};
use starorg_core::blueprints::catalog::{BlueprintCatalog, GameDataCatalog, ListCatalog};
use starorg_core::blueprints::import::{self, EXPORT_LIMIT};
use starorg_core::blueprints::logscan::{self, ScanReport, Store};
use starorg_core::blueprints::{self, Characters, Choice, repair};
use starorg_core::cache::{self, KeyInputs};
use starorg_core::discovery::{Channel, Install};
use starorg_core::fsutil;
use starorg_core::ini::StockIni;
use starorg_core::lock::{self, Lock, LockError};
use starorg_core::source::{Budget, Loaded};
use starorg_core::state::ChannelState;
use starorg_data::starbreaker::STARBREAKER_COMMIT;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

pub fn run(g: &GlobalOpts, cmd: BlueprintCmd) -> ExitCode {
    match cmd {
        BlueprintCmd::Scan { channel } => scan_cmd(g, channel.as_deref()),
        BlueprintCmd::Characters { channel } => characters(g, &channel),
        BlueprintCmd::List { channel, character } => list(g, &channel, character.as_deref()),
        BlueprintCmd::Use { channel, character } => use_cmd(g, &channel, &character),
        BlueprintCmd::Import {
            file,
            character,
            channel,
            dry_run,
            replace,
            #[cfg(feature = "test-hooks")]
            test_catalog,
        } => {
            #[cfg(not(feature = "test-hooks"))]
            let test_catalog: Option<std::path::PathBuf> = None;
            import_cmd(
                g,
                Some(&file),
                &character,
                &channel,
                (dry_run, replace),
                test_catalog.as_deref(),
            )
        }
        BlueprintCmd::Reimport {
            file,
            character,
            channel,
            dry_run,
            replace,
            #[cfg(feature = "test-hooks")]
            test_catalog,
        } => {
            #[cfg(not(feature = "test-hooks"))]
            let test_catalog: Option<std::path::PathBuf> = None;
            import_cmd(
                g,
                file.as_deref(),
                &character,
                &channel,
                (dry_run, replace),
                test_catalog.as_deref(),
            )
        }
        BlueprintCmd::Rescan {
            channel,
            dry_run,
            replace,
        } => rescan_cmd(g, &channel, dry_run, replace),
    }
}

pub(crate) fn blueprint_lock(c: &Ctx, id: &str, ch: &Channel) -> Result<Lock, String> {
    let p = c.dirs.locks().join(format!(
        "{id}-{}-blueprints.lock",
        ch.name.to_ascii_uppercase()
    ));
    lock::try_lock(&p).map_err(|e| match e {
        LockError::Held(_) => format!("{}: another blueprint scan is running", ch.name),
        other => other.to_string(),
    })
}

/// Scan one channel's logs. With `save`, under the channel's blueprint lock, and written.
pub(crate) fn scan_channel(
    c: &Ctx,
    id: Option<&str>,
    ch: &Channel,
    deadline: Option<Instant>,
    save: bool,
) -> Result<(Store, ScanReport), String> {
    let data = c.dirs.channel_dir(id.unwrap_or("unregistered"), &ch.name);
    let _lock = match (save, id) {
        (true, Some(id)) => Some(blueprint_lock(c, id, ch)?),
        _ => None,
    };
    let mut store = if id.is_some() {
        Store::load(&data)?
    } else {
        Store::default()
    };
    let on = starorg_core::starorg::SyncPrefs::load(&c.dirs).import_game_log;
    let rep = logscan::scan_if(on, &ch.dir, &data, &mut store, deadline);
    if save {
        store.save(&data)?;
    }
    Ok((store, rep))
}

/// A character's name, with "(newest)" or "(last seen <time>)" when another character has
/// the same name.
fn label_of(chars: &Characters, geid: &str) -> String {
    let Some(c) = chars.characters.get(geid) else {
        return geid.to_string();
    };
    let key = blueprints::normalize(&c.name);
    let same: Vec<&blueprints::Character> = chars
        .characters
        .values()
        .filter(|x| blueprints::normalize(&x.name) == key)
        .collect();
    if same.len() < 2 {
        return c.name.clone();
    }
    let newest = same.iter().map(|x| &x.last_seen).max().cloned().flatten();
    if c.last_seen == newest {
        format!("{} (newest)", c.name)
    } else {
        format!(
            "{} (last seen {})",
            c.name,
            c.last_seen.as_deref().unwrap_or("never")
        )
    }
}

fn name_of(chars: &Characters, geid: Option<&str>) -> String {
    match geid {
        Some(g) => chars
            .characters
            .get(g)
            .map_or_else(|| g.to_string(), |c| c.name.clone()),
        None => "unattributed".into(),
    }
}

pub(crate) fn print_scan(g: &GlobalOpts, ch: &str, store: &Store, r: &ScanReport) {
    if g.quiet || g.json {
        return;
    }
    print_repairs(ch, store, r);
    if r.disabled {
        println!(
            "{ch}: Game.log import is off (starorg-codex sync set --import-game-log on); no log read"
        );
        return;
    }
    let mut line = format!(
        "{ch}: {} log files, {} read ({:.1} MB), {} blueprint lines, {} new",
        r.files,
        r.files_read,
        r.bytes_read as f64 / 1e6,
        r.blueprint_lines,
        r.new_blueprints.len()
    );
    if let Some(gid) = &r.last_character {
        line.push_str(&format!(
            "; last played: {}",
            name_of(&store.characters, Some(gid))
        ));
    }
    if r.out_of_time {
        line.push_str(" (out of time; the rest next run)");
    }
    println!("{line}");
    for (who, name) in &r.new_blueprints {
        println!(
            "  + {name} ({})",
            name_of(&store.characters, who.as_deref())
        );
    }
    for w in &r.warnings {
        eprintln!("  warning: {w}");
    }
}

/// The one-time store repair and character merges a scan made, and names shared by
/// characters of different accounts.
fn print_repairs(ch: &str, store: &Store, r: &ScanReport) {
    if let Some(x) = &r.repaired
        && x.changed()
    {
        println!("{ch}: blueprint store repaired: {}", x.summary());
        if let Some(b) = &x.backup {
            println!("  backup: {}", b.display());
        }
    }
    let merged = r
        .repaired
        .iter()
        .flat_map(|x| x.merged.iter())
        .chain(r.merged.iter());
    for m in merged {
        println!(
            "  merged character {} into {} (same account and name, new geid; {} blueprints added)",
            m.from,
            name_of(&store.characters, Some(&m.into)),
            m.added
        );
    }
    let mut shown = std::collections::BTreeSet::new();
    for group in &r.same_name {
        if shown.insert(group.clone()) {
            println!(
                "  note: {} characters of different accounts are named {}; kept apart (pick one with `blueprints use`)",
                group.len(),
                name_of(&store.characters, group.first().map(String::as_str))
            );
        }
    }
}

fn rescan_cmd(g: &GlobalOpts, channel: &str, dry_run: bool, replace: bool) -> ExitCode {
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
    let id = if dry_run {
        lookup(&c, &i)
    } else {
        match register(g, &c, &i) {
            Ok(id) => Some(id),
            Err(e) => return e,
        }
    };
    if !starorg_core::starorg::SyncPrefs::load(&c.dirs).import_game_log {
        return fail(
            g,
            exit::USAGE,
            "disabled",
            "Game.log import is off (starorg-codex sync set --import-game-log on); no log read",
        );
    }
    let data = c
        .dirs
        .channel_dir(id.as_deref().unwrap_or("unregistered"), &ch.name);
    let _lock = match (&id, dry_run) {
        (Some(id), false) => match blueprint_lock(&c, id, ch) {
            Ok(l) => Some(l),
            Err(e) => return fail(g, exit::SKIPPED, "busy", &e),
        },
        _ => None,
    };
    let mut store = if id.is_some() {
        match Store::load(&data) {
            Ok(s) => s,
            Err(e) => return fail(g, exit::ERROR, "io", &e),
        }
    } else {
        Store::default()
    };
    let rep = match repair::rebuild_from_logs(&ch.dir, &data, &mut store, replace, None) {
        Ok(r) => r,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let refused = rep.replace_refusal();
    if !dry_run && let Some(why) = &refused {
        for w in &rep.scan.warnings {
            eprintln!("  warning: {w}");
        }
        return fail(g, exit::ERROR, "unread-log", why);
    }
    let mut wrote = false;
    if !dry_run && id.is_some() {
        if let Err(e) = store.save(&data) {
            return fail(g, exit::ERROR, "io", &e);
        }
        wrote = true;
    }
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "blueprints rescan", "channel": ch.name,
                    "dry_run": dry_run, "written": wrote, "replace_refused": refused, "report": rep })
        );
    } else if !g.quiet {
        println!(
            "{}: re-read {} log files from the start{}{}",
            ch.name,
            rep.scan.files_read,
            if replace {
                ", every set rebuilt from the logs only"
            } else {
                ""
            },
            if dry_run {
                " [dry run: nothing written]"
            } else {
                ""
            }
        );
        print_repairs(&ch.name, &store, &rep.scan);
        for d in &rep.characters {
            let who = if d.id.is_some() {
                d.name.as_str()
            } else {
                "unattributed"
            };
            println!(
                "  {who}: {} -> {} blueprints (+{} -{})",
                d.before,
                d.after,
                d.added.len(),
                d.removed.len()
            );
            for n in &d.added {
                println!("    + {n}");
            }
            for n in &d.removed {
                println!("    - {n}");
            }
        }
        for w in &rep.scan.warnings {
            eprintln!("  warning: {w}");
        }
        if let Some(why) = &refused {
            eprintln!("  {why}");
        }
    }
    ExitCode::from(exit::OK)
}

fn scan_cmd(g: &GlobalOpts, channel: Option<&str>) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let i = match install(g) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let chans: Vec<&Channel> = match channel {
        Some(n) => match find_channel(g, &i, n) {
            Ok(ch) => vec![ch],
            Err(e) => return e,
        },
        None => i.channels.iter().collect(),
    };
    let id = match register(g, &c, &i) {
        Ok(id) => id,
        Err(e) => return e,
    };
    let mut out = Vec::new();
    let mut code = exit::OK;
    for ch in chans {
        match scan_channel(&c, Some(&id), ch, None, true) {
            Ok((store, r)) => {
                print_scan(g, &ch.name, &store, &r);
                out.push(json!({ "channel": ch.name, "scan": r }));
            }
            Err(e) => {
                eprintln!("starorg-codex: {e}");
                code = exit::worst([code, exit::SKIPPED]);
                out.push(json!({ "channel": ch.name, "error": e }));
            }
        }
    }
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "blueprints scan", "channels": out })
        );
    }
    ExitCode::from(code)
}

fn characters(g: &GlobalOpts, channel: &str) -> ExitCode {
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
    let Some(id) = lookup(&c, &i) else {
        return empty(g, ch, "no characters yet (run `blueprints scan`)");
    };
    let data = c.dirs.channel_dir(&id, &ch.name);
    let chars = match blueprints::load_characters(&data) {
        Ok(x) => x,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let setting = ChannelState::load(&data)
        .ok()
        .and_then(|s| s.active_character);
    let active = blueprints::select(&data, setting.as_deref()).ok();
    let mut rows = Vec::new();
    for (geid, ch_) in &chars.characters {
        let owned = blueprints::load_set(&data, geid)
            .map(|s| s.entries.len())
            .unwrap_or(0);
        let is_active = active.as_ref().is_some_and(|a| a.characters.contains(geid));
        rows.push(
            json!({ "geid": geid, "name": ch_.name, "account_id": ch_.account_id,
            "last_seen": ch_.last_seen, "owned": owned, "active": is_active }),
        );
        if !g.json && !g.quiet {
            println!(
                "{} {:<24} geid {:<16} last seen {:<26} {} owned",
                if is_active { "*" } else { " " },
                ch_.name,
                geid,
                ch_.last_seen.as_deref().unwrap_or("-"),
                owned
            );
        }
    }
    let unattributed = blueprints::load_unattributed(&data)
        .map(|s| s.entries.len())
        .unwrap_or(0);
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "channel": ch.name, "setting": setting.as_deref().unwrap_or("auto"),
                    "characters": rows, "unattributed": unattributed,
                    "fallback_from": active.as_ref().and_then(|a| a.fallback_from.clone()) })
        );
    } else if !g.quiet {
        if chars.characters.is_empty() {
            println!("{}: no characters yet (run `blueprints scan`)", ch.name);
        }
        if let Some(from) = active.as_ref().and_then(|a| a.fallback_from.as_deref()) {
            let used = active.as_ref().and_then(|a| a.characters.first());
            println!(
                "note: the newest character, {}, owns no blueprints; auto uses {} (pick with `blueprints use`)",
                label_of(&chars, from),
                used.map_or_else(|| "-".to_string(), |g| label_of(&chars, g))
            );
        }
        if setting.is_none()
            && let Ok(p) = blueprints::auto_character(&data, &chars)
            && let (Some(big), Some(used)) = (p.larger.as_deref(), p.used.as_deref())
        {
            println!(
                "note: {} owns far more blueprints than {}; `blueprints use any` marks both",
                label_of(&chars, big),
                label_of(&chars, used)
            );
        }
        for w in active.iter().flat_map(|a| a.warnings.iter()) {
            eprintln!("warning: {w}");
        }
        println!(
            "setting: {}; unattributed blueprints: {unattributed}",
            setting.as_deref().unwrap_or("auto")
        );
    }
    ExitCode::from(exit::OK)
}

fn empty(g: &GlobalOpts, ch: &Channel, msg: &str) -> ExitCode {
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "channel": ch.name, "characters": [], "note": msg })
        );
    } else if !g.quiet {
        println!("{}: {msg}", ch.name);
    }
    ExitCode::from(exit::OK)
}

fn list(g: &GlobalOpts, channel: &str, character: Option<&str>) -> ExitCode {
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
    let Some(id) = lookup(&c, &i) else {
        return empty(g, ch, "nothing known yet (run `blueprints scan`)");
    };
    let data = c.dirs.channel_dir(&id, &ch.name);
    let setting = ChannelState::load(&data)
        .ok()
        .and_then(|s| s.active_character);
    let sel = match blueprints::select(&data, character.or(setting.as_deref())) {
        Ok(s) => s,
        Err(e) => return fail(g, exit::USAGE, "usage", &e),
    };
    let mut entries = Vec::new();
    for geid in &sel.characters {
        if let Ok(set) = blueprints::load_set(&data, geid) {
            for e in set.entries.into_values() {
                entries.push((geid.clone(), e));
            }
        }
    }
    entries.sort_by_key(|e| e.1.name.to_lowercase());
    if g.json {
        let v: Vec<_> = entries
            .iter()
            .map(|(geid, e)| json!({ "geid": geid, "entry": e }))
            .collect();
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "channel": ch.name, "mode": sel.mode, "count": sel.count, "entries": v })
        );
    } else if !g.quiet {
        for (_, e) in &entries {
            let srcs: Vec<&str> = e.sources.iter().map(|s| s.kind.as_str()).collect();
            println!(
                "{}{}{}  [{}]",
                if e.favorite { "* " } else { "  " },
                e.name,
                if e.ambiguous { " (ambiguous)" } else { "" },
                srcs.join(", ")
            );
        }
        println!("{} owned ({})", sel.count, sel.mode);
    }
    ExitCode::from(exit::OK)
}

fn use_cmd(g: &GlobalOpts, channel: &str, who: &str) -> ExitCode {
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
    let id = match register(g, &c, &i) {
        Ok(id) => id,
        Err(e) => return e,
    };
    let data = c.dirs.channel_dir(&id, &ch.name);
    let chars = blueprints::load_characters(&data).unwrap_or_default();
    let value = match Choice::parse(Some(who), &chars) {
        Ok(Choice::Auto) => None,
        Ok(Choice::Any) => Some("any".to_string()),
        Ok(Choice::Character(gid)) => Some(gid),
        Err(e) => return fail(g, exit::USAGE, "usage", &e.to_string()),
    };
    let mut st = ChannelState::load(&data).unwrap_or_default();
    st.active_character = value.clone();
    if let Err(e) = st.save(&data) {
        return fail(g, exit::ERROR, "io", &e.to_string());
    }
    // An explicit choice ends a one-off `apply --character` pick.
    if let Err(e) = starorg_core::apply::forget_one_off_character(&data) {
        return fail(g, exit::ERROR, "io", &e);
    }
    let label = match &value {
        None => "auto".to_string(),
        Some(v) if v == "any" => "any".to_string(),
        Some(gid) => name_of(&chars, Some(gid)),
    };
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "channel": ch.name, "active_character": value.as_deref().unwrap_or("auto") })
        );
    } else if !g.quiet {
        println!(
            "{}: [Owned] markers use {label}; the next apply picks it up",
            ch.name
        );
    }
    ExitCode::from(exit::OK)
}

/// Stock text and GameData of the channel's current build, to name blueprints. From the
/// extraction cache when it has this build; otherwise extracted from `Data.p4k` (read only).
/// A real import caches the extraction like an apply does; a dry run reads the cache but
/// writes nothing, not even the cache or a lock file.
pub(crate) fn channel_data(
    g: &GlobalOpts,
    c: &Ctx,
    i: &Install,
    id: Option<&str>,
    ch: &Channel,
    dry_run: bool,
) -> Result<Loaded, ExitCode> {
    let fp = ch.fingerprint().ok();
    if let (true, Some(id)) = (fp.is_some(), id)
        && c.use_cache
        && !dry_run
    {
        let t = Target::new(i, Some(id.to_string()), ch);
        let source = source(g, c)?;
        if !g.quiet && !g.json {
            eprintln!("Loading {} blueprint names ...", ch.name);
        }
        let mut warnings = Vec::new();
        return apply::load_channel(c, &t, source.as_ref(), &Budget::unlimited(), &mut warnings)
            .map_err(|e| fail(g, exit::ERROR, "extraction", &e));
    }
    if let (Some(fp), Some(id)) = (&fp, id)
        && c.use_cache
        && let Ok(l) = cache::load(
            &c.dirs.channel_cache(id, &ch.name),
            &KeyInputs::local(fp, STARBREAKER_COMMIT),
            c.min_stock_keys,
        )
    {
        return Ok(l);
    }
    let source = source(g, c)?;
    if !g.quiet && !g.json {
        eprintln!("Extracting {} from {} ...", ch.name, ch.p4k.display());
    }
    source
        .load(ch, &Budget::unlimited())
        .map_err(|e| fail(g, exit::ERROR, "extraction", &e.to_string()))
}

fn import_cmd(
    g: &GlobalOpts,
    file: Option<&Path>,
    who: &str,
    channel: &str,
    (dry_run, replace): (bool, bool),
    test_catalog: Option<&Path>,
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
    let last = lookup(&c, &i)
        .map(|id| import::last_import(&c.dirs.channel_dir(&id, &ch.name)).smart_citizen)
        .unwrap_or_default();
    let file: std::path::PathBuf = match (file, last) {
        (Some(f), _) => f.to_path_buf(),
        (None, Some(f)) => f,
        (None, None) => {
            return fail(
                g,
                exit::USAGE,
                "usage",
                "no earlier Smart Citizen import on this channel; give the export FILE",
            );
        }
    };
    let file = file.as_path();
    let bytes = match fsutil::read_limited(file, EXPORT_LIMIT) {
        Ok(Some(b)) => b,
        Ok(None) => {
            return fail(
                g,
                exit::NOT_FOUND,
                "not-found",
                &format!("{} not found", file.display()),
            );
        }
        Err(e) => return fail(g, exit::ERROR, "io", &e.to_string()),
    };
    let export = match import::parse_export(&bytes) {
        Ok(x) => x,
        Err(e) => return fail(g, exit::USAGE, "bad-export", &e),
    };
    let id = if dry_run {
        lookup(&c, &i)
    } else {
        match register(g, &c, &i) {
            Ok(id) => Some(id),
            Err(e) => return e,
        }
    };
    let loaded;
    let stock;
    let catalog: Box<dyn BlueprintCatalog + '_> = match test_catalog {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(t) => Box::new(ListCatalog::parse(&t)),
            Err(e) => {
                return fail(g, exit::ERROR, "io", &format!("{}: {e}", p.display()));
            }
        },
        None => {
            loaded = match channel_data(g, &c, &i, id.as_deref(), ch, dry_run) {
                Ok(l) => l,
                Err(code) => return code,
            };
            stock = StockIni::parse(loaded.base_ini);
            Box::new(GameDataCatalog::new(&loaded.game_data, &stock))
        }
    };
    let data = c
        .dirs
        .channel_dir(id.as_deref().unwrap_or("unregistered"), &ch.name);
    // Hold the lock from the scan to the write (a dry run writes nothing and takes none).
    let _lock = match (&id, dry_run) {
        (Some(id), false) => match blueprint_lock(&c, id, ch) {
            Ok(l) => Some(l),
            Err(e) => return fail(g, exit::SKIPPED, "busy", &e),
        },
        _ => None,
    };
    // The character is resolved from the logs: scan first (in memory for a dry run).
    let mut store = if id.is_some() {
        match Store::load(&data) {
            Ok(s) => s,
            Err(e) => return fail(g, exit::ERROR, "io", &e),
        }
    } else {
        Store::default()
    };
    let on = starorg_core::starorg::SyncPrefs::load(&c.dirs).import_game_log;
    let scan = logscan::scan_if(on, &ch.dir, &data, &mut store, None);
    let geid = match store.characters.resolve(who) {
        Ok(gid) => gid,
        Err(e @ blueprints::ResolveError::Ambiguous(..)) => {
            return fail(g, exit::NEEDS_DECISION, "ambiguous", &e.to_string());
        }
        Err(e) => return fail(g, exit::USAGE, "usage", &e.to_string()),
    };
    let set = match store.set_mut(&data, &geid) {
        Ok(s) => s,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let rep = import::merge_with(&export, catalog.as_ref(), set, replace);
    let count_after = set.entries.len();
    let name = store
        .characters
        .characters
        .get(&geid)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| geid.clone());
    let mut code = exit::OK;
    let mut wrote = false;
    if rep.empty_catalog && !dry_run {
        code = exit::ERROR;
    } else if !dry_run {
        if let Err(e) = store.save(&data) {
            return fail(g, exit::ERROR, "io", &e);
        }
        let abs = fsutil::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        if let Err(e) = import::save_last_import(&data, &abs) {
            eprintln!("  warning: could not remember the export for a re-import: {e}");
        }
        wrote = true;
    }
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "blueprints import", "channel": ch.name,
                    "character": { "geid": geid, "name": name }, "dry_run": dry_run, "written": wrote,
                    "owned_after": count_after, "report": rep, "scan": scan })
        );
    } else if !g.quiet {
        println!(
            "Smart Citizen export {} ({} entries, exported {}) -> {} on {}{}",
            file.display(),
            rep.total,
            rep.exported_at.as_deref().unwrap_or("?"),
            name,
            ch.name,
            if dry_run {
                " [dry run: nothing written]"
            } else {
                ""
            }
        );
        println!(
            "  imported {}, already known {}, removed {}, unmatched {}, ambiguous {}, not completed {}, duplicates {}",
            rep.imported.len(),
            rep.known.len(),
            rep.removed.len(),
            rep.unmatched.len(),
            rep.ambiguous.len(),
            rep.not_completed,
            rep.duplicates
        );
        for n in &rep.removed {
            println!("  removed (no longer in the export): {n}");
        }
        for n in &rep.unmatched {
            println!("  unmatched: {n}");
        }
        for a in &rep.ambiguous {
            println!("  ambiguous: {} -> {}", a.name, a.candidates.join(", "));
        }
        println!("  owned after import: {count_after}");
    }
    if rep.empty_catalog {
        let msg = "no blueprint names to match against: this build's GameData has no named blueprints; nothing imported";
        if dry_run {
            eprintln!("  warning: {msg}");
        } else {
            eprintln!("starorg-codex: {msg}");
        }
    }
    ExitCode::from(code)
}
