//! `user.cfg`: our one line, `g_language = english` (spec 7.4).
//!
//! - Found case-insensitively in the channel directory (`USER.cfg` here). It may be a symlink
//!   shared by several channels: links are followed and the **real** file is edited in place
//!   (temp file beside the real target, rename over it). A link is never replaced.
//! - Line present with `english`: nothing is written. Line missing (or no file): append it.
//!   Another language: stop unless the user agreed to switch.
//! - Every other line, comment and line ending is kept.
//! - A ledger per real file (`data/cfg/<hash>.json`) records what was there before, the line
//!   we wrote, and which channels rely on it. Undo removes or restores the line only when no
//!   channel relies on it any more and the line is still byte-for-byte ours.
//!
//! **Managed lines.** The same rules hold for every line we own ([`ManagedLine`]): the
//! language line ([`LANGUAGE`]) and the opt-in live telemetry lines ([`TELEMETRY`],
//! [`TELEMETRY_TRIGGERS`], see [`crate::telemetry`]). Each line has its own ledger per real
//! file (`data/cfg/<hash>.json` for the language, `data/cfg/<hash>.<key>.json` for the
//! others), so undoing one never touches another. All of them share the per-file lock
//! ([`lock_path`]) because they edit the same file.

use crate::appdirs::AppDirs;
use crate::fsutil::{self, read_json, write_json};
use crate::lock::{self, LockError};
use crate::paths::{ResolveError, resolve_ci};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const OUR_LINE: &str = "g_language = english";
/// `user.cfg` files larger than this are not edited.
pub const CFG_LIMIT: u64 = 1 << 20;

/// A line we own in `user.cfg`: one cvar set to one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedLine {
    /// The cvar, matched case-insensitively at the start of a line.
    pub name: &'static str,
    /// Values that already mean what we want (compared case-insensitively). A line with one of
    /// them is left alone.
    pub accepts: &'static [&'static str],
    /// The exact line we write.
    pub line: &'static str,
    /// Ledger file suffix: `None` gives `<hash>.json` (the language line; ledgers written
    /// before managed lines existed), `Some(k)` gives `<hash>.<k>.json`.
    pub key: Option<&'static str>,
}

/// `g_language = english` (spec 7.4).
pub const LANGUAGE: ManagedLine = ManagedLine {
    name: "g_language",
    accepts: &["english"],
    line: OUR_LINE,
    key: None,
};

/// Turns on the game's telemetry frames (5.0+, read from `USER.cfg` only).
pub const TELEMETRY: ManagedLine = ManagedLine {
    name: "server.game.telemetry.enabled",
    accepts: &["1", "true"],
    line: "server.game.telemetry.enabled = 1",
    key: Some("telemetry"),
};

/// Lets local programs press allowlisted game inputs (`TriggerGameAction`). Written only
/// while [`TELEMETRY`] is ours too.
pub const TELEMETRY_TRIGGERS: ManagedLine = ManagedLine {
    name: "server.game.telemetry.trigger.actions.enabled",
    accepts: &["1", "true"],
    line: "server.game.telemetry.trigger.actions.enabled = 1",
    key: Some("telemetry-triggers"),
};

/// One physical line, split into content and ending.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line<'a> {
    content: &'a str,
    ending: &'a str,
}

fn split_lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(i) => {
                let (line, tail) = rest.split_at(i + 1);
                let (content, ending) = match line.strip_suffix("\r\n") {
                    Some(c) => (c, "\r\n"),
                    None => (&line[..line.len() - 1], "\n"),
                };
                out.push(Line { content, ending });
                rest = tail;
            }
            None => {
                out.push(Line {
                    content: rest,
                    ending: "",
                });
                rest = "";
            }
        }
    }
    out
}

/// If `line` sets `g_language`, its value (unquoted, comment removed).
pub fn language_value(line: &str) -> Option<String> {
    cvar_value(line, LANGUAGE.name)
}

/// If `line` sets the cvar `name`, its value (unquoted, comment removed). `name` must be
/// followed by `=` (spaces allowed), so `g_language_audio` is not `g_language`.
pub fn cvar_value(line: &str, name: &str) -> Option<String> {
    let t = line.trim_start();
    if !t.get(..name.len())?.eq_ignore_ascii_case(name) {
        return None;
    }
    let rest = t.get(name.len()..)?.trim_start();
    let rest = rest.strip_prefix('=')?;
    let mut v = rest.trim();
    if let Some(q) = v.strip_prefix('"') {
        v = q.split('"').next().unwrap_or("");
    } else {
        let end = v.find([';', '#']).unwrap_or(v.len());
        v = v[..end].trim();
    }
    Some(v.to_string())
}

/// What the file says about the language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum LanguageLine {
    /// No `user.cfg`.
    NoFile,
    /// File without a `g_language` line.
    Missing,
    English {
        line: String,
    },
    Other {
        value: String,
        line: String,
    },
}

/// Find the language line (the last one wins, as in the game's cvar loading).
pub fn inspect(text: &str) -> LanguageLine {
    match inspect_line(Some(text), &LANGUAGE) {
        CvarLine::NoFile => LanguageLine::NoFile,
        CvarLine::Missing => LanguageLine::Missing,
        CvarLine::Set { line } => LanguageLine::English { line },
        CvarLine::Other { value, line } => LanguageLine::Other { value, line },
    }
}

/// What the file says about one [`ManagedLine`]'s cvar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum CvarLine {
    /// No `user.cfg`.
    NoFile,
    /// File without a line for the cvar.
    Missing,
    /// Set to a value the managed line accepts.
    Set { line: String },
    /// Set to something else.
    Other { value: String, line: String },
}

/// Find the line setting `m`'s cvar (the last one wins). `text` is `None` without a file.
pub fn inspect_line(text: Option<&str>, m: &ManagedLine) -> CvarLine {
    let Some(text) = text else {
        return CvarLine::NoFile;
    };
    let mut found = CvarLine::Missing;
    for l in split_lines(text) {
        if let Some(v) = cvar_value(l.content, m.name) {
            found = if m.accepts.iter().any(|a| v.eq_ignore_ascii_case(a)) {
                CvarLine::Set {
                    line: l.content.to_string(),
                }
            } else {
                CvarLine::Other {
                    value: v,
                    line: l.content.to_string(),
                }
            };
        }
    }
    found
}

/// The plan for a managed line: like [`plan`], with `replace_other` as the user's decision
/// to overwrite another value.
pub fn plan_line(state: &CvarLine, replace_other: bool) -> Plan {
    match state {
        CvarLine::Set { .. } => Plan::Nothing,
        CvarLine::NoFile | CvarLine::Missing => Plan::Append,
        CvarLine::Other { value, line } => {
            if replace_other {
                Plan::Replace {
                    previous: line.clone(),
                }
            } else {
                Plan::NeedsDecision {
                    value: value.clone(),
                }
            }
        }
    }
}

/// The change to make.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Plan {
    Nothing,
    Append,
    /// Replace the other-language line (only with a recorded `--switch-language`).
    Replace {
        previous: String,
    },
    /// Another language and no decision: apply exits 6.
    NeedsDecision {
        value: String,
    },
}

pub fn plan(state: &LanguageLine, switch_language: bool) -> Plan {
    match state {
        LanguageLine::English { .. } => Plan::Nothing,
        LanguageLine::NoFile | LanguageLine::Missing => Plan::Append,
        LanguageLine::Other { value, line } => {
            if switch_language {
                Plan::Replace {
                    previous: line.clone(),
                }
            } else {
                Plan::NeedsDecision {
                    value: value.clone(),
                }
            }
        }
    }
}

/// The file's dominant line ending (CRLF if any line uses it, else LF).
fn ending_of(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Apply a plan to the text. Returns the new text and whether a newline had to be added to
/// the old last line.
pub fn edit(text: &str, plan: &Plan) -> (String, bool) {
    edit_line(text, plan, &LANGUAGE)
}

/// [`edit`] for any managed line: append or put `m.line` in place of the previous line.
pub fn edit_line(text: &str, plan: &Plan, m: &ManagedLine) -> (String, bool) {
    match plan {
        Plan::Nothing | Plan::NeedsDecision { .. } => (text.to_string(), false),
        Plan::Append => {
            let nl = ending_of(text);
            let mut out = text.to_string();
            let mut added_newline = false;
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(nl);
                added_newline = true;
            }
            out.push_str(m.line);
            out.push_str(nl);
            (out, added_newline)
        }
        Plan::Replace { previous } => {
            let mut out = String::with_capacity(text.len());
            let lines = split_lines(text);
            // Replace the last matching line only (the one in effect).
            let idx = lines.iter().rposition(|l| l.content == previous);
            for (i, l) in lines.iter().enumerate() {
                out.push_str(if Some(i) == idx { m.line } else { l.content });
                out.push_str(l.ending);
            }
            (out, false)
        }
    }
}

/// Undo our line: remove it (`previous == None`) or put the previous line back. Only when the
/// current text still has our exact line. Returns `None` when the line is not ours any more.
pub fn undo(text: &str, previous: Option<&str>, added_newline: bool) -> Option<String> {
    undo_line(text, &LANGUAGE, previous, added_newline)
}

/// [`undo`] for any managed line: only `m.line` exactly is ours.
pub fn undo_line(
    text: &str,
    m: &ManagedLine,
    previous: Option<&str>,
    added_newline: bool,
) -> Option<String> {
    let lines = split_lines(text);
    let idx = lines.iter().rposition(|l| l.content == m.line)?;
    let mut out = String::with_capacity(text.len());
    for (i, l) in lines.iter().enumerate() {
        if i == idx {
            match previous {
                Some(p) => {
                    out.push_str(p);
                    out.push_str(l.ending);
                }
                None => {
                    // Our line was appended; if we had to end the old last line first and our
                    // line is still last, take that newline back off too.
                    if added_newline && i + 1 == lines.len() {
                        for e in ["\r\n", "\n"] {
                            if let Some(s) = out.strip_suffix(e) {
                                out.truncate(s.len());
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            out.push_str(l.content);
            out.push_str(l.ending);
        }
    }
    Some(out)
}

/// Where `user.cfg` is and what it points to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfgTarget {
    /// Path in the channel directory (on-disk spelling, or `user.cfg` when absent).
    pub path: PathBuf,
    pub exists: bool,
    /// The real file after following links (equals `path` when not a link).
    pub real: PathBuf,
    pub is_link: bool,
}

pub fn locate(channel_dir: &Path) -> Result<CfgTarget, ResolveError> {
    let r = resolve_ci(channel_dir, &["user.cfg"])?;
    let is_link = std::fs::symlink_metadata(&r.path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let exists = r.path.exists();
    let real = if exists {
        fsutil::canonicalize(&r.path).unwrap_or_else(|_| r.path.clone())
    } else if is_link {
        // A dangling link: follow it by hand so we never replace it with a file.
        let target = std::fs::read_link(&r.path).unwrap_or_else(|_| r.path.clone());
        // Normalise so the ledger and the cfg lock key match the existing-file case.
        let joined = if target.is_absolute() {
            target
        } else {
            channel_dir.join(target)
        };
        fsutil::canonical_lenient(&joined)
    } else {
        fsutil::canonical_lenient(&r.path)
    };
    Ok(CfgTarget {
        path: r.path,
        exists,
        real,
        is_link,
    })
}

/// Read a `user.cfg` (`None` when absent).
pub fn read(target: &CfgTarget) -> std::io::Result<Option<String>> {
    if !target.exists {
        return Ok(None);
    }
    let Some(b) = fsutil::read_limited(&target.real, CFG_LIMIT)? else {
        return Ok(None);
    };
    // The game reads it as bytes; keep invalid UTF-8 lossless by refusing to edit it.
    String::from_utf8(b)
        .map(Some)
        .map_err(|_| std::io::Error::other(format!("{} is not valid UTF-8", target.real.display())))
}

/// Write the real file atomically (temp beside the real target).
pub fn write(target: &CfgTarget, text: &str) -> std::io::Result<()> {
    let name = target
        .real
        .file_name()
        .ok_or_else(|| std::io::Error::other("user.cfg has no file name"))?;
    let tmp = target
        .real
        .with_file_name(format!(".{}.starorg-tmp", name.to_string_lossy()));
    fsutil::write_via(&tmp, &target.real, text.as_bytes())
}

/// `data/cfg/<hash>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub real_path: PathBuf,
    /// `None` = the line was absent; otherwise the exact previous line.
    pub previous: Option<String>,
    pub written: String,
    /// We ended the old last line with a newline before appending.
    #[serde(default)]
    pub added_newline: bool,
    /// `<install-id>/<CHANNEL>` entries relying on the line.
    pub channels: Vec<String>,
    /// Copy of the file from before our first change, for the user to look at.
    pub reference_copy: Option<PathBuf>,
    /// The file did not exist before we created it.
    #[serde(default)]
    pub created: bool,
}

pub fn path_hash(real: &Path) -> String {
    fsutil::sha256_hex(real.to_string_lossy().as_bytes())[..16].to_string()
}

pub fn ledger_path(dirs: &AppDirs, real: &Path) -> PathBuf {
    ledger_path_for(dirs, real, &LANGUAGE)
}

/// `data/cfg/<hash>.json` for the language line, `data/cfg/<hash>.<key>.json` for the others.
pub fn ledger_path_for(dirs: &AppDirs, real: &Path, m: &ManagedLine) -> PathBuf {
    let hash = path_hash(real);
    dirs.cfg_dir().join(match m.key {
        None => format!("{hash}.json"),
        Some(k) => format!("{hash}.{k}.json"),
    })
}

/// One lock per real file, shared by every managed line.
pub fn lock_path(dirs: &AppDirs, real: &Path) -> PathBuf {
    dirs.locks().join(format!("cfg-{}.lock", path_hash(real)))
}

pub fn load_ledger(dirs: &AppDirs, real: &Path) -> std::io::Result<Option<Ledger>> {
    load_ledger_for(dirs, real, &LANGUAGE)
}

pub fn load_ledger_for(
    dirs: &AppDirs,
    real: &Path,
    m: &ManagedLine,
) -> std::io::Result<Option<Ledger>> {
    read_json(&ledger_path_for(dirs, real, m))
}

pub fn save_ledger(dirs: &AppDirs, ledger: &Ledger) -> std::io::Result<()> {
    save_ledger_for(dirs, ledger, &LANGUAGE)
}

pub fn save_ledger_for(dirs: &AppDirs, ledger: &Ledger, m: &ManagedLine) -> std::io::Result<()> {
    write_json(&ledger_path_for(dirs, &ledger.real_path, m), ledger)
}

pub fn delete_ledger(dirs: &AppDirs, real: &Path) -> std::io::Result<()> {
    delete_ledger_for(dirs, real, &LANGUAGE)
}

pub fn delete_ledger_for(dirs: &AppDirs, real: &Path, m: &ManagedLine) -> std::io::Result<()> {
    match std::fs::remove_file(ledger_path_for(dirs, real, m)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `data/cfg/<hash>.before.cfg`: the file as it was before our first change (any line).
pub fn reference_copy_path(dirs: &AppDirs, real: &Path) -> PathBuf {
    dirs.cfg_dir()
        .join(format!("{}.before.cfg", path_hash(real)))
}

/// Outcome of undoing a channel's reliance on the line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UndoOutcome {
    /// No ledger, or the channel was not on it.
    NotOurs,
    /// Other channels still rely on it; file untouched.
    StillUsed { channels: Vec<String> },
    /// Line removed or previous line restored.
    Reverted,
    /// Someone changed the line since; left alone.
    ChangedSince,
}

impl UndoOutcome {
    /// Stable machine name (matches the serde spelling).
    pub fn kebab(&self) -> &'static str {
        match self {
            Self::NotOurs => "not-ours",
            Self::StillUsed { .. } => "still-used",
            Self::Reverted => "reverted",
            Self::ChangedSince => "changed-since",
        }
    }
}

/// Take `channel_ref` off the ledger of `target` and revert the line when nobody needs it.
pub fn release(
    dirs: &AppDirs,
    target: &CfgTarget,
    channel_ref: &str,
) -> std::io::Result<UndoOutcome> {
    release_line(dirs, target, channel_ref, &LANGUAGE)
}

/// [`release`] for any managed line. The caller holds [`lock_path`].
pub fn release_line(
    dirs: &AppDirs,
    target: &CfgTarget,
    channel_ref: &str,
    m: &ManagedLine,
) -> std::io::Result<UndoOutcome> {
    let Some(mut ledger) = load_ledger_for(dirs, &target.real, m)? else {
        return Ok(UndoOutcome::NotOurs);
    };
    if !ledger.channels.iter().any(|c| c == channel_ref) {
        return Ok(UndoOutcome::NotOurs);
    }
    ledger.channels.retain(|c| c != channel_ref);
    if !ledger.channels.is_empty() {
        save_ledger_for(dirs, &ledger, m)?;
        return Ok(UndoOutcome::StillUsed {
            channels: ledger.channels,
        });
    }
    let current = read(target)?;
    let outcome = match current {
        None => UndoOutcome::ChangedSince,
        Some(text) => match undo_line(&text, m, ledger.previous.as_deref(), ledger.added_newline) {
            Some(new_text) => {
                if ledger.created && new_text.is_empty() && !target.is_link {
                    fsutil::remove_retry(&target.real)?;
                } else {
                    write(target, &new_text)?;
                    if ledger.created {
                        hand_over_created(dirs, &target.real, m)?;
                    }
                }
                UndoOutcome::Reverted
            }
            None => UndoOutcome::ChangedSince,
        },
    };
    delete_ledger_for(dirs, &target.real, m)?;
    Ok(outcome)
}

/// Every line we manage; they share one file, one lock and the "we created it" fact.
const MANAGED: [&ManagedLine; 3] = [&LANGUAGE, &TELEMETRY, &TELEMETRY_TRIGGERS];

/// The file `from` created outlives `from`'s line (another managed line is still in it):
/// pass the fact on to the ledgers still there, so whichever line goes last removes the
/// file instead of leaving an empty one behind.
fn hand_over_created(dirs: &AppDirs, real: &Path, from: &ManagedLine) -> std::io::Result<()> {
    for m in MANAGED.into_iter().filter(|m| *m != from) {
        if let Some(mut l) = load_ledger_for(dirs, real, m)?
            && !l.created
        {
            l.created = true;
            save_ledger_for(dirs, &l, m)?;
        }
    }
    Ok(())
}

/// How long [`adopt`] and [`drop_line`] wait for another run holding the file's lock.
pub const LOCK_WAIT: Duration = Duration::from_secs(10);

/// Outcome of making a channel rely on a managed line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum AdoptOutcome {
    /// The line was there already (ours from another channel, or the user's own).
    AlreadySet,
    /// We appended the line or replaced the previous one.
    Written,
    /// The cvar has another value and the caller did not agree to replace it.
    NeedsDecision { value: String },
}

#[derive(Debug, thiserror::Error)]
pub enum CfgError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error("{0}")]
    NotWritable(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn wait_lock(path: &Path) -> Result<lock::Lock, LockError> {
    let start = std::time::Instant::now();
    loop {
        match lock::try_lock(path) {
            Err(LockError::Held(_)) if start.elapsed() < LOCK_WAIT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            r => return r,
        }
    }
}

/// Make `channel_ref` rely on `m` in the `user.cfg` of `channel_dir`, writing the line when
/// it is missing (or, with `replace_other`, set to another value). Same guarantees as the
/// language line in apply: links followed, the real file edited in place, other lines and
/// endings kept, the ledger saved before the file is written, [`CFG_LIMIT`].
pub fn adopt(
    dirs: &AppDirs,
    channel_dir: &Path,
    channel_ref: &str,
    m: &ManagedLine,
    replace_other: bool,
) -> Result<AdoptOutcome, CfgError> {
    let first = locate(channel_dir)?;
    let _l = wait_lock(&lock_path(dirs, &first.real))?;
    // Re-locate and re-read under the lock: another channel sharing the file may have
    // changed it.
    let cfg = locate(channel_dir)?;
    let text = read(&cfg)?;
    let plan = plan_line(&inspect_line(text.as_deref(), m), replace_other);
    let mut ledger = load_ledger_for(dirs, &cfg.real, m)?;
    match plan {
        Plan::NeedsDecision { value } => Ok(AdoptOutcome::NeedsDecision { value }),
        Plan::Nothing => {
            // Ours from another channel sharing the file: rely on it too.
            if let Some(l) = ledger.as_mut()
                && text
                    .as_deref()
                    .is_some_and(|tx| split_lines(tx).iter().any(|x| x.content == l.written))
                && !l.channels.iter().any(|c| c == channel_ref)
            {
                l.channels.push(channel_ref.to_string());
                save_ledger_for(dirs, l, m)?;
            }
            Ok(AdoptOutcome::AlreadySet)
        }
        Plan::Append | Plan::Replace { .. } => {
            if cfg.exists {
                fsutil::owned_regular_file(&cfg.real).map_err(CfgError::NotWritable)?;
            } else if !cfg.real.parent().is_some_and(Path::is_dir) {
                return Err(CfgError::NotWritable(format!(
                    "{} has no folder to create it in",
                    cfg.real.display()
                )));
            }
            let before = text.as_deref().unwrap_or("");
            let (new_text, added_newline) = edit_line(before, &plan, m);
            let reference_copy = if text.is_some() {
                let p = reference_copy_path(dirs, &cfg.real);
                if !p.exists() {
                    fsutil::write_atomic(&p, before.as_bytes())?;
                }
                Some(p)
            } else {
                None
            };
            // Channels on an old ledger still rely on the line (removed by hand since).
            let mut channels: Vec<String> = ledger.take().map(|l| l.channels).unwrap_or_default();
            if !channels.iter().any(|c| c == channel_ref) {
                channels.push(channel_ref.to_string());
            }
            let new_ledger = Ledger {
                real_path: cfg.real.clone(),
                previous: match &plan {
                    Plan::Replace { previous } => Some(previous.clone()),
                    _ => None,
                },
                written: m.line.to_string(),
                added_newline,
                channels,
                reference_copy,
                created: text.is_none(),
            };
            // Ledger first: if we die after the file write, undo can still find it.
            save_ledger_for(dirs, &new_ledger, m)?;
            if let Err(e) = write(&cfg, &new_text) {
                let _ = delete_ledger_for(dirs, &cfg.real, m);
                return Err(e.into());
            }
            Ok(AdoptOutcome::Written)
        }
    }
}

/// Take `channel_ref` off `m`'s ledger in the `user.cfg` of `channel_dir`, under the file's
/// lock, and revert the line when no channel relies on it and it is still ours.
pub fn drop_line(
    dirs: &AppDirs,
    channel_dir: &Path,
    channel_ref: &str,
    m: &ManagedLine,
) -> Result<UndoOutcome, CfgError> {
    let first = locate(channel_dir)?;
    let _l = wait_lock(&lock_path(dirs, &first.real))?;
    let cfg = locate(channel_dir)?;
    Ok(release_line(dirs, &cfg, channel_ref, m)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn dangling_relative_link_is_normalised() {
        let t = tempfile::tempdir().unwrap();
        let live = t.path().join("game/LIVE");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(t.path().join("shared")).unwrap();
        std::os::unix::fs::symlink("../../shared/USER.cfg", live.join("USER.cfg")).unwrap();
        let c = locate(&live).unwrap();
        assert!(c.is_link && !c.exists);
        let want = std::fs::canonicalize(t.path().join("shared"))
            .unwrap()
            .join("USER.cfg");
        assert_eq!(c.real, want);
        assert!(
            !c.real
                .components()
                .any(|p| p == std::path::Component::ParentDir)
        );
    }

    #[test]
    fn language_line_matching() {
        assert_eq!(
            language_value("g_language = english"),
            Some("english".into())
        );
        assert_eq!(
            language_value("  G_LANGUAGE=\"german_(germany)\" ; x"),
            Some("german_(germany)".into())
        );
        assert_eq!(
            language_value("g_language = english ; comment"),
            Some("english".into())
        );
        assert_eq!(language_value("g_language=french#c"), Some("french".into()));
        assert_eq!(language_value("; g_language = english"), None);
        assert_eq!(language_value("g_languageX = english"), None);
        assert_eq!(language_value("g_language_audio = english"), None);
    }

    #[test]
    fn plans() {
        assert_eq!(
            plan(&inspect("r_width = 1\r\ng_language = english\r\n"), false),
            Plan::Nothing
        );
        assert_eq!(plan(&inspect("r_width = 1\r\n"), false), Plan::Append);
        assert_eq!(plan(&LanguageLine::NoFile, false), Plan::Append);
        assert_eq!(
            plan(&inspect("g_language = german_(germany)\n"), false),
            Plan::NeedsDecision {
                value: "german_(germany)".into()
            }
        );
        assert!(matches!(
            plan(&inspect("g_language = french\n"), true),
            Plan::Replace { .. }
        ));
    }

    #[test]
    fn append_and_undo_keep_every_other_byte() {
        for before in [
            "",
            "a = 1\r\nb = 2\r\n",
            "a = 1\nno_final_newline",
            "a=1\r\nlast",
        ] {
            let (after, added) = edit(before, &Plan::Append);
            assert!(after.contains(OUR_LINE));
            assert!(after.starts_with(before));
            // A user line added after ours survives undo.
            let user = format!("{after}sys_MaxFPS = 144\n");
            let undone = undo(&user, None, added).unwrap();
            assert!(undone.contains("sys_MaxFPS = 144"));
            assert!(!undone.contains(OUR_LINE));
            // Without the user line, undo gives the original bytes back.
            assert_eq!(undo(&after, None, added).unwrap(), before, "{before:?}");
        }
    }

    #[test]
    fn replace_and_restore_previous() {
        let before = "x = 1\r\ng_language = french ; mine\r\ny = 2\r\n";
        let p = plan(&inspect(before), true);
        let (after, _) = edit(before, &p);
        assert_eq!(after, "x = 1\r\ng_language = english\r\ny = 2\r\n");
        assert_eq!(
            undo(&after, Some("g_language = french ; mine"), false).unwrap(),
            before
        );
        // Someone changed our line: not ours any more.
        assert_eq!(undo("g_language = english ; edited\n", None, false), None);
    }

    #[cfg(unix)]
    #[test]
    fn shared_symlink_ledger() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&t.path().join("app"));
        let shared = t.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("USER.cfg"), "r_width = 1\r\n").unwrap();
        for ch in ["LIVE", "HOTFIX"] {
            std::fs::create_dir_all(t.path().join(ch)).unwrap();
            std::os::unix::fs::symlink(shared.join("USER.cfg"), t.path().join(ch).join("USER.cfg"))
                .unwrap();
        }
        let live = locate(&t.path().join("LIVE")).unwrap();
        let hot = locate(&t.path().join("HOTFIX")).unwrap();
        assert!(live.is_link && live.exists);
        assert_eq!(live.real, hot.real);
        let text = read(&live).unwrap().unwrap();
        let (new, added) = edit(&text, &Plan::Append);
        write(&live, &new).unwrap();
        save_ledger(
            &dirs,
            &Ledger {
                real_path: live.real.clone(),
                previous: None,
                written: OUR_LINE.into(),
                added_newline: added,
                channels: vec!["i/LIVE".into(), "i/HOTFIX".into()],
                reference_copy: None,
                created: false,
            },
        )
        .unwrap();
        assert!(
            std::fs::symlink_metadata(t.path().join("LIVE/USER.cfg"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        // Removing LIVE leaves the shared line for HOTFIX.
        assert!(matches!(
            release(&dirs, &live, "i/LIVE").unwrap(),
            UndoOutcome::StillUsed { .. }
        ));
        assert!(
            std::fs::read_to_string(shared.join("USER.cfg"))
                .unwrap()
                .contains(OUR_LINE)
        );
        assert_eq!(
            release(&dirs, &hot, "i/HOTFIX").unwrap(),
            UndoOutcome::Reverted
        );
        assert_eq!(
            std::fs::read_to_string(shared.join("USER.cfg")).unwrap(),
            "r_width = 1\r\n"
        );
        assert!(
            std::fs::symlink_metadata(t.path().join("HOTFIX/USER.cfg"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn dotted_cvar_matching() {
        let n = TELEMETRY.name;
        assert_eq!(
            cvar_value("server.game.telemetry.enabled = 1", n),
            Some("1".into())
        );
        assert_eq!(
            cvar_value("  SERVER.GAME.TELEMETRY.ENABLED=true ; x", n),
            Some("true".into())
        );
        assert_eq!(cvar_value("; server.game.telemetry.enabled = 1", n), None);
        // The triggers cvar starts with a different word, so it never matches the base one.
        assert_eq!(cvar_value(TELEMETRY_TRIGGERS.line, n), None);
        assert_eq!(cvar_value("server.game.telemetry.enabledX = 1", n), None);
        assert_eq!(
            inspect_line(Some("server.game.telemetry.enabled = TRUE\n"), &TELEMETRY),
            CvarLine::Set {
                line: "server.game.telemetry.enabled = TRUE".into()
            }
        );
        assert_eq!(inspect_line(None, &TELEMETRY), CvarLine::NoFile);
        // The language line keeps its own view.
        assert_eq!(
            inspect("server.game.telemetry.enabled = 1\n"),
            LanguageLine::Missing
        );
    }

    #[test]
    fn old_ledger_still_loads_and_paths_are_separate() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        let real = Path::new("/games/LIVE/USER.cfg");
        // A ledger as 0.1 wrote it: no added_newline, no created.
        std::fs::create_dir_all(dirs.cfg_dir()).unwrap();
        std::fs::write(
            ledger_path(&dirs, real),
            r#"{"real_path":"/games/LIVE/USER.cfg","previous":null,
                "written":"g_language = english","channels":["i/LIVE"],"reference_copy":null}"#,
        )
        .unwrap();
        let l = load_ledger(&dirs, real).unwrap().unwrap();
        assert_eq!(l.channels, vec!["i/LIVE".to_string()]);
        assert!(!l.added_newline && !l.created);
        assert_eq!(
            ledger_path(&dirs, real),
            ledger_path_for(&dirs, real, &LANGUAGE)
        );
        let a = ledger_path_for(&dirs, real, &TELEMETRY);
        let b = ledger_path_for(&dirs, real, &TELEMETRY_TRIGGERS);
        assert!(a != b && a != ledger_path(&dirs, real));
        assert!(load_ledger_for(&dirs, real, &TELEMETRY).unwrap().is_none());
    }

    #[test]
    fn managed_lines_do_not_touch_the_language_line() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&t.path().join("app"));
        let ch = t.path().join("LIVE");
        std::fs::create_dir_all(&ch).unwrap();
        std::fs::write(ch.join("USER.cfg"), "a = 1\nno_final_newline").unwrap();
        // The language line as apply writes it.
        let cfg = locate(&ch).unwrap();
        let text = read(&cfg).unwrap().unwrap();
        let (new, added) = edit(&text, &Plan::Append);
        write(&cfg, &new).unwrap();
        save_ledger(
            &dirs,
            &Ledger {
                real_path: cfg.real.clone(),
                previous: None,
                written: OUR_LINE.into(),
                added_newline: added,
                channels: vec!["i/LIVE".into()],
                reference_copy: None,
                created: false,
            },
        )
        .unwrap();
        assert_eq!(
            adopt(&dirs, &ch, "i/LIVE", &TELEMETRY, false).unwrap(),
            AdoptOutcome::Written
        );
        assert_eq!(
            adopt(&dirs, &ch, "i/LIVE", &TELEMETRY, false).unwrap(),
            AdoptOutcome::AlreadySet
        );
        let both = std::fs::read_to_string(ch.join("USER.cfg")).unwrap();
        assert_eq!(
            both,
            format!("a = 1\nno_final_newline\n{OUR_LINE}\n{}\n", TELEMETRY.line)
        );
        // Language undo leaves the telemetry line; telemetry undo then gives the bytes back.
        assert_eq!(
            release(&dirs, &cfg, "i/LIVE").unwrap(),
            UndoOutcome::Reverted
        );
        assert_eq!(
            std::fs::read_to_string(ch.join("USER.cfg")).unwrap(),
            format!("a = 1\nno_final_newline\n{}\n", TELEMETRY.line)
        );
        assert_eq!(
            drop_line(&dirs, &ch, "i/LIVE", &TELEMETRY).unwrap(),
            UndoOutcome::Reverted
        );
        // The newline apply added before its line stays: the telemetry line was appended
        // to a file that already ended in one.
        assert_eq!(
            std::fs::read_to_string(ch.join("USER.cfg")).unwrap(),
            "a = 1\nno_final_newline\n"
        );
        assert_eq!(
            drop_line(&dirs, &ch, "i/LIVE", &TELEMETRY).unwrap(),
            UndoOutcome::NotOurs
        );
    }

    #[test]
    fn managed_line_edited_by_user_is_left_alone() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&t.path().join("app"));
        let ch = t.path().join("PTU");
        std::fs::create_dir_all(&ch).unwrap();
        assert_eq!(
            adopt(&dirs, &ch, "i/PTU", &TELEMETRY, false).unwrap(),
            AdoptOutcome::Written
        );
        let l = load_ledger_for(&dirs, &locate(&ch).unwrap().real, &TELEMETRY)
            .unwrap()
            .unwrap();
        assert!(l.created && l.previous.is_none());
        std::fs::write(
            ch.join("user.cfg"),
            "server.game.telemetry.enabled = 1 ; edited\n",
        )
        .unwrap();
        assert_eq!(
            drop_line(&dirs, &ch, "i/PTU", &TELEMETRY).unwrap(),
            UndoOutcome::ChangedSince
        );
        assert_eq!(
            std::fs::read_to_string(ch.join("user.cfg")).unwrap(),
            "server.game.telemetry.enabled = 1 ; edited\n"
        );
        assert!(
            load_ledger_for(&dirs, &locate(&ch).unwrap().real, &TELEMETRY)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn created_file_is_removed_on_undo() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&t.path().join("app"));
        let ch = t.path().join("PTU");
        std::fs::create_dir_all(&ch).unwrap();
        adopt(&dirs, &ch, "i/PTU", &TELEMETRY, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(ch.join("user.cfg")).unwrap(),
            format!("{}\n", TELEMETRY.line)
        );
        assert_eq!(
            drop_line(&dirs, &ch, "i/PTU", &TELEMETRY).unwrap(),
            UndoOutcome::Reverted
        );
        assert!(!ch.join("user.cfg").exists());
        // No folder: refused, nothing written.
        assert!(matches!(
            adopt(&dirs, &t.path().join("GONE"), "i/GONE", &TELEMETRY, false),
            Err(CfgError::NotWritable(_))
        ));
    }

    #[test]
    fn created_file_is_removed_whichever_line_goes_last() {
        // The first line created the file; the second found it there. Either release order
        // ends with no file, not an empty one.
        for first_out in [&TELEMETRY, &TELEMETRY_TRIGGERS] {
            let t = tempfile::tempdir().unwrap();
            let dirs = AppDirs::under(&t.path().join("app"));
            let ch = t.path().join("PTU");
            std::fs::create_dir_all(&ch).unwrap();
            adopt(&dirs, &ch, "i/PTU", &TELEMETRY, false).unwrap();
            adopt(&dirs, &ch, "i/PTU", &TELEMETRY_TRIGGERS, false).unwrap();
            let last_out = if first_out == &TELEMETRY {
                &TELEMETRY_TRIGGERS
            } else {
                &TELEMETRY
            };
            for m in [first_out, last_out] {
                assert_eq!(
                    drop_line(&dirs, &ch, "i/PTU", m).unwrap(),
                    UndoOutcome::Reverted
                );
            }
            assert!(
                !ch.join("user.cfg").exists(),
                "released {} first",
                first_out.name
            );
        }
    }

    #[test]
    fn oversized_file_is_not_edited() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&t.path().join("app"));
        let ch = t.path().join("PTU");
        std::fs::create_dir_all(&ch).unwrap();
        let big = "a".repeat(CFG_LIMIT as usize + 1);
        std::fs::write(ch.join("USER.cfg"), &big).unwrap();
        assert!(matches!(
            adopt(&dirs, &ch, "i/PTU", &TELEMETRY, false),
            Err(CfgError::Io(_))
        ));
        assert_eq!(std::fs::read_to_string(ch.join("USER.cfg")).unwrap(), big);
    }

    #[cfg(unix)]
    #[test]
    fn managed_line_shared_symlink_ledger() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&t.path().join("app"));
        let shared = t.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("USER.cfg"), "r_width = 1\r\n").unwrap();
        for ch in ["LIVE", "PTU"] {
            std::fs::create_dir_all(t.path().join(ch)).unwrap();
            std::os::unix::fs::symlink(shared.join("USER.cfg"), t.path().join(ch).join("USER.cfg"))
                .unwrap();
        }
        let live = t.path().join("LIVE");
        let ptu = t.path().join("PTU");
        assert_eq!(
            adopt(&dirs, &ptu, "i/PTU", &TELEMETRY, false).unwrap(),
            AdoptOutcome::Written
        );
        assert_eq!(
            adopt(&dirs, &live, "i/LIVE", &TELEMETRY, false).unwrap(),
            AdoptOutcome::AlreadySet
        );
        let real = locate(&live).unwrap().real;
        let l = load_ledger_for(&dirs, &real, &TELEMETRY).unwrap().unwrap();
        assert_eq!(l.channels, vec!["i/PTU".to_string(), "i/LIVE".to_string()]);
        assert!(
            load_ledger(&dirs, &real).unwrap().is_none(),
            "no language ledger"
        );
        assert!(matches!(
            drop_line(&dirs, &ptu, "i/PTU", &TELEMETRY).unwrap(),
            UndoOutcome::StillUsed { .. }
        ));
        assert!(
            std::fs::read_to_string(shared.join("USER.cfg"))
                .unwrap()
                .contains(TELEMETRY.line)
        );
        assert_eq!(
            drop_line(&dirs, &live, "i/LIVE", &TELEMETRY).unwrap(),
            UndoOutcome::Reverted
        );
        assert_eq!(
            std::fs::read_to_string(shared.join("USER.cfg")).unwrap(),
            "r_width = 1\r\n"
        );
        for ch in [&live, &ptu] {
            assert!(
                std::fs::symlink_metadata(ch.join("USER.cfg"))
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
    }
}
