//! `fleet import|list|match|clear` (addendum §6.7).
//!
//! Order, groups, renames and the prefix style are edited by hand in `ship-names.toml` (or
//! the channel's `profile.toml` `[ship_names]` table); `fleet list` is the check.

use crate::apply_cmds::{ctx, find_channel, install, lookup, register};
use crate::blueprint_cmds::channel_data;
use crate::commands::{JSON_SCHEMA, fail};
use crate::{FleetCmd, GlobalOpts, exit};
use serde_json::json;
use starorg_core::apply::Ctx;
use starorg_core::config::Config;
use starorg_core::discovery::{Channel, Install};
use starorg_core::fleet::import::{self, Format, IMPORT_LIMIT};
use starorg_core::fleet::names;
use starorg_core::fleet::store::{self, FleetFile};
use starorg_core::fleet::{Catalog, normalize};
use starorg_core::fsutil;
use starorg_core::ini::StockIni;
use starorg_core::lock::{self, Lock, LockError};
use starorg_core::state::{self, Profile};
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

pub fn run(g: &GlobalOpts, cmd: FleetCmd) -> ExitCode {
    match cmd {
        FleetCmd::Import {
            file,
            dry_run,
            merge,
            format,
            channel,
        } => import_cmd(g, &file, dry_run, merge, &format, channel.as_deref()),
        FleetCmd::List { channel } => list(g, channel.as_deref()),
        FleetCmd::Match {
            name,
            hull,
            channel,
        } => match_cmd(g, &name, &hull, channel.as_deref()),
        FleetCmd::Clear => clear(g),
    }
}

/// `fleet.json` and `ship-names.toml` are shared by every channel: one writer at a time.
fn fleet_lock(c: &Ctx) -> Result<Lock, String> {
    lock::try_lock(&c.dirs.locks().join("fleet.lock")).map_err(|e| match e {
        LockError::Held(_) => "another fleet import is running".to_string(),
        other => other.to_string(),
    })
}

fn channel_name(c: &Ctx, given: Option<&str>) -> String {
    given
        .map(str::to_string)
        .or_else(|| Config::load(&c.dirs).ok().and_then(|c| c.default_channel))
        .unwrap_or_else(|| "LIVE".into())
}

struct Opened {
    c: Ctx,
    i: Install,
    id: Option<String>,
}

fn open(g: &GlobalOpts, write: bool) -> Result<Opened, ExitCode> {
    let c = ctx(g)?;
    let i = install(g)?;
    let id = if write {
        Some(register(g, &c, &i)?)
    } else {
        lookup(&c, &i)
    };
    Ok(Opened { c, i, id })
}

/// The hull catalogue of a channel's build: from the extraction cache, or extracted from
/// `Data.p4k` (read only). A dry run or a read caches nothing.
fn catalog(g: &GlobalOpts, o: &Opened, ch: &Channel, dry_run: bool) -> Result<Catalog, ExitCode> {
    let loaded = channel_data(g, &o.c, &o.i, o.id.as_deref(), ch, dry_run)?;
    let stock = StockIni::parse(loaded.base_ini);
    Ok(Catalog::build(&loaded.game_data, &stock))
}

fn read_input(file: &Path) -> Result<Vec<u8>, String> {
    if file == Path::new("-") {
        let mut b = Vec::new();
        std::io::stdin()
            .take(IMPORT_LIMIT + 1)
            .read_to_end(&mut b)
            .map_err(|e| format!("stdin: {e}"))?;
        return Ok(b);
    }
    match fsutil::read_limited(file, IMPORT_LIMIT) {
        Ok(Some(b)) => Ok(b),
        Ok(None) => Err(format!("{} not found", file.display())),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// Hulls new to the fleet go to the end of the shared order list (ungrouped).
fn append_to_order(c: &Ctx, added: &[String]) -> Result<usize, String> {
    if added.is_empty() {
        return Ok(0);
    }
    let mut s = store::load_shared(&c.dirs)?;
    let n = s.append(added);
    if n > 0 {
        store::save_shared(&c.dirs, &s)?;
    }
    Ok(n)
}

fn import_cmd(
    g: &GlobalOpts,
    file: &Path,
    dry_run: bool,
    merge: bool,
    format: &str,
    channel: Option<&str>,
) -> ExitCode {
    let fmt = match format {
        "auto" => None,
        f => match Format::parse(f) {
            Some(f) => Some(f),
            None => {
                return fail(
                    g,
                    exit::USAGE,
                    "usage",
                    &format!("--format {f}: one of auto, xplor-json, xplor-csv, text"),
                );
            }
        },
    };
    let bytes = match read_input(file) {
        Ok(b) => b,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let parsed = match import::parse(&bytes, fmt) {
        Ok(p) => p,
        Err(e) => return fail(g, exit::ERROR, "bad-file", &e),
    };
    let o = match open(g, !dry_run) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let name = channel_name(&o.c, channel);
    let ch = match find_channel(g, &o.i, &name) {
        Ok(ch) => ch,
        Err(code) => return code,
    };
    let cat = match catalog(g, &o, ch, dry_run) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let _lock = if dry_run {
        None
    } else {
        match fleet_lock(&o.c) {
            Ok(l) => Some(l),
            Err(e) => return fail(g, exit::SKIPPED, "busy", &e),
        }
    };
    let fleet = match FleetFile::load(&o.c.dirs) {
        Ok(f) => f,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let mut out = import::run(&parsed, &cat, &fleet, merge);
    let r = &out.report;
    let mut written = false;
    let mut appended = 0;
    if !dry_run && !r.empty_catalog {
        out.fleet.imported_at = Some(fsutil::rfc3339(fsutil::unix_now()));
        if let Err(e) = out.fleet.save(&o.c.dirs) {
            return fail(g, exit::ERROR, "io", &e);
        }
        written = true;
        appended = match append_to_order(&o.c, &out.added) {
            Ok(n) => n,
            Err(e) => return fail(g, exit::ERROR, "io", &e),
        };
    }
    let r = &out.report;
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "fleet import", "channel": ch.name,
                    "dry_run": dry_run, "written": written, "added_to_order": appended,
                    "fleet_file": store::fleet_path(&o.c.dirs), "report": r })
        );
    } else if !g.quiet {
        let matched: usize = r.tiers.values().sum();
        println!(
            "Fleet {} ({}, {} rows) matched against {}{}",
            if file == Path::new("-") {
                "from stdin".to_string()
            } else {
                file.display().to_string()
            },
            r.format.name(),
            r.total,
            ch.name,
            if dry_run {
                " [dry run: nothing written]"
            } else {
                ""
            }
        );
        println!(
            "  {} hulls ({} rows matched, {} ships), {} unmatched, {} ambiguous, {} not ships, {} marked not in the game{}",
            r.hulls.len(),
            matched,
            r.hulls.iter().map(|h| h.count).sum::<u32>(),
            r.unmatched.len(),
            r.ambiguous.len(),
            r.skipped,
            r.not_in_game,
            if merge { " (merged)" } else { "" }
        );
        for h in &r.hulls {
            let names = if h.names.is_empty() {
                String::new()
            } else {
                format!("  suggested: {}", h.names.join(", "))
            };
            println!(
                "  {}{:<40} x{}  {}{names}",
                if h.new { "+ " } else { "  " },
                h.name,
                h.count,
                h.hull
            );
        }
        for u in &r.unmatched {
            println!("  unmatched: {} (x{})", u.name, u.count);
        }
        for a in &r.ambiguous {
            println!("  ambiguous: {} -> {}", a.name, a.candidates.join(", "));
        }
        if !r.unmatched.is_empty() || !r.ambiguous.is_empty() {
            println!("  match by hand: starorg-strings fleet match \"<row name>\" <hull id|none>");
        }
        if written && appended > 0 {
            println!(
                "  {appended} new hulls added to the end of {}",
                store::ship_names_path(&o.c.dirs).display()
            );
        }
    }
    if r.empty_catalog {
        let msg = format!(
            "no ship names to match against: the game data of {} has no player hulls; nothing imported",
            ch.name
        );
        if dry_run {
            eprintln!("  warning: {msg}");
            return ExitCode::from(exit::OK);
        }
        eprintln!("starorg-strings: {msg}");
        return ExitCode::from(exit::ERROR);
    }
    ExitCode::from(exit::OK)
}

fn profile_of(o: &Opened, ch: &Channel) -> (Profile, Vec<state::Override>) {
    let Some(id) = &o.id else {
        return (Profile::default(), Vec::new());
    };
    let data = o.c.dirs.channel_dir(id, &ch.name);
    let p = Profile::load_file(&Profile::path(&data)).unwrap_or_default();
    let ov = state::load_overrides(&data)
        .map(|(_, o, _)| o)
        .unwrap_or_default();
    (p, ov)
}

fn list(g: &GlobalOpts, channel: Option<&str>) -> ExitCode {
    let o = match open(g, false) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let name = channel_name(&o.c, channel);
    let ch = match find_channel(g, &o.i, &name) {
        Ok(ch) => ch,
        Err(code) => return code,
    };
    let fleet = match FleetFile::load(&o.c.dirs) {
        Ok(f) => f,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let (profile, overrides) = profile_of(&o, ch);
    let eff = store::effective(&o.c.dirs, &profile);
    let settings_file = match eff.scope {
        store::Scope::Channel => {
            o.id.as_ref()
                .map(|id| Profile::path(&o.c.dirs.channel_dir(id, &ch.name)))
        }
        _ => Some(store::ship_names_path(&o.c.dirs)),
    };
    let cat = match catalog(g, &o, ch, true) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let rows = names::listing(&eff.settings, &cat, &overrides);
    let count = |hull: &str| fleet.owned(hull).map(|h| h.count).unwrap_or(0);
    if g.json {
        let ships: Vec<_> = rows
            .iter()
            .map(|r| {
                json!({ "position": r.position, "hull": r.hull, "group": r.group, "prefix": r.prefix,
                        "value": r.value, "stock": r.stock, "count": count(&r.hull),
                        "renamed": r.renamed, "overridden": r.overridden,
                        "overridden_short": r.overridden_short, "not_in_build": r.not_in_build })
            })
            .collect();
        let pending: Vec<_> = fleet
            .pending
            .iter()
            .map(|p| json!({ "name": p.name, "count": p.count, "candidates": p.candidates }))
            .collect();
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "fleet list", "channel": ch.name,
                    "fleet_file": store::fleet_path(&o.c.dirs), "settings_file": settings_file,
                    "scope": eff.scope, "enabled": eff.settings.enabled, "active": eff.active().is_some(),
                    "style": eff.settings.style, "owned_hulls": fleet.hulls.len(), "owned_ships": fleet.total(),
                    "ships": ships, "pending": pending,
                    "warnings": eff.warnings.iter().map(|w| &w.detail).collect::<Vec<_>>() })
        );
        return ExitCode::from(exit::OK);
    }
    if g.quiet {
        return ExitCode::from(exit::OK);
    }
    println!(
        "fleet:    {} ({} hulls, {} ships, {} pending)",
        store::fleet_path(&o.c.dirs).display(),
        fleet.hulls.len(),
        fleet.total(),
        fleet.pending.len()
    );
    println!(
        "settings: {} (use = {}, {}, style {})",
        settings_file
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".into()),
        eff.scope.name(),
        match (eff.scope, eff.settings.enabled, eff.invalid) {
            (store::Scope::Off, _, _) => "off for this channel",
            (_, _, true) => "invalid: nothing written",
            (_, true, _) => "on",
            (_, false, _) => "off (enabled = false)",
        },
        serde_json::to_value(eff.settings.style)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    );
    for w in &eff.warnings {
        println!("  warning: {}", w.detail);
    }
    if rows.is_empty() {
        println!("  (no ships listed; add [[ship]] entries or run `fleet import`)");
    }
    for r in &rows {
        let mut flags = Vec::new();
        if r.overridden {
            flags.push("overridden");
        }
        if r.overridden_short {
            flags.push("overridden-short");
        }
        if r.not_in_build {
            flags.push("not-in-build");
        }
        let n = count(&r.hull);
        println!(
            "  {:<5}{:<36} {:<36} {:>4}  {}{}",
            r.prefix.trim_end(),
            if r.not_in_build { &r.hull } else { &r.value },
            r.stock.trim(),
            if n > 0 {
                format!("x{n}")
            } else {
                String::new()
            },
            r.group
                .as_deref()
                .map(|g| format!("[{g}] "))
                .unwrap_or_default(),
            flags.join(", ")
        );
    }
    for p in &fleet.pending {
        if p.candidates.is_empty() {
            println!("  pending: {} (x{}) not matched", p.name, p.count);
        } else {
            println!(
                "  pending: {} (x{}) ambiguous: {}",
                p.name,
                p.count,
                p.candidates.join(", ")
            );
        }
    }
    ExitCode::from(exit::OK)
}

fn match_cmd(g: &GlobalOpts, row: &str, hull: &str, channel: Option<&str>) -> ExitCode {
    let o = match open(g, true) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let name = channel_name(&o.c, channel);
    let ch = match find_channel(g, &o.i, &name) {
        Ok(ch) => ch,
        Err(code) => return code,
    };
    let cat = match catalog(g, &o, ch, false) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let target = if hull.eq_ignore_ascii_case("none") {
        None
    } else {
        match cat.get(hull).filter(|h| h.flyable) {
            Some(h) => Some(h),
            None => {
                return fail(
                    g,
                    exit::USAGE,
                    "usage",
                    &format!(
                        "{hull} is not a player hull of {} (ids as `fleet list` and `fleet import` show them, or none)",
                        ch.name
                    ),
                );
            }
        }
    };
    let _lock = match fleet_lock(&o.c) {
        Ok(l) => l,
        Err(e) => return fail(g, exit::SKIPPED, "busy", &e),
    };
    let mut fleet = match FleetFile::load(&o.c.dirs) {
        Ok(f) => f,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let was_pending = fleet
        .pending
        .iter()
        .any(|p| normalize(&p.name) == normalize(row));
    let added = import::hand_match(&mut fleet, row, target);
    if let Err(e) = fleet.save(&o.c.dirs) {
        return fail(g, exit::ERROR, "io", &e);
    }
    let appended = match append_to_order(&o.c, added.as_slice()) {
        Ok(n) => n,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "fleet match", "name": row,
                    "hull": target.map(|h| &h.hull), "was_pending": was_pending,
                    "added_to_fleet": added, "added_to_order": appended })
        );
    } else if !g.quiet {
        match target {
            Some(h) => println!("{row} -> {} ({})", h.name.trim(), h.hull),
            None => println!("{row} -> not in the game (not asked again)"),
        }
        if !was_pending {
            println!("  (no pending row of that name; saved for the next import)");
        }
    }
    ExitCode::from(exit::OK)
}

fn clear(g: &GlobalOpts) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let _lock = match fleet_lock(&c) {
        Ok(l) => l,
        Err(e) => return fail(g, exit::SKIPPED, "busy", &e),
    };
    let mut fleet = match FleetFile::load(&c.dirs) {
        Ok(f) => f,
        Err(e) => return fail(g, exit::ERROR, "io", &e),
    };
    let n = fleet.hulls.len() + fleet.pending.len();
    fleet.hulls.clear();
    fleet.pending.clear();
    fleet.imported_at = None;
    fleet.format = None;
    if let Err(e) = fleet.save(&c.dirs) {
        return fail(g, exit::ERROR, "io", &e);
    }
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": "fleet clear", "removed": n,
                    "kept_matches": fleet.matches.len() })
        );
    } else if !g.quiet {
        println!(
            "Fleet cleared ({n} entries). Hand matches and the ship order in {} are kept.",
            store::ship_names_path(&c.dirs).display()
        );
    }
    ExitCode::from(exit::OK)
}
