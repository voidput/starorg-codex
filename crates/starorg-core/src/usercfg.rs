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

use crate::appdirs::AppDirs;
use crate::fsutil::{self, read_json, write_json};
use crate::paths::{ResolveError, resolve_ci};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const OUR_LINE: &str = "g_language = english";
/// `user.cfg` files larger than this are not edited.
pub const CFG_LIMIT: u64 = 1 << 20;

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
    let t = line.trim_start();
    if !t.get(..10)?.eq_ignore_ascii_case("g_language") {
        return None;
    }
    let rest = t.get(10..)?.trim_start();
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
    let mut found = LanguageLine::Missing;
    for l in split_lines(text) {
        if let Some(v) = language_value(l.content) {
            found = if v.eq_ignore_ascii_case("english") {
                LanguageLine::English {
                    line: l.content.to_string(),
                }
            } else {
                LanguageLine::Other {
                    value: v,
                    line: l.content.to_string(),
                }
            };
        }
    }
    found
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
            out.push_str(OUR_LINE);
            out.push_str(nl);
            (out, added_newline)
        }
        Plan::Replace { previous } => {
            let mut out = String::with_capacity(text.len());
            let lines = split_lines(text);
            // Replace the last matching line only (the one in effect).
            let idx = lines.iter().rposition(|l| l.content == previous);
            for (i, l) in lines.iter().enumerate() {
                out.push_str(if Some(i) == idx { OUR_LINE } else { l.content });
                out.push_str(l.ending);
            }
            (out, false)
        }
    }
}

/// Undo our line: remove it (`previous == None`) or put the previous line back. Only when the
/// current text still has our exact line. Returns `None` when the line is not ours any more.
pub fn undo(text: &str, previous: Option<&str>, added_newline: bool) -> Option<String> {
    let lines = split_lines(text);
    let idx = lines.iter().rposition(|l| l.content == OUR_LINE)?;
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
        std::fs::canonicalize(&r.path).unwrap_or_else(|_| r.path.clone())
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
    dirs.cfg_dir().join(format!("{}.json", path_hash(real)))
}

pub fn lock_path(dirs: &AppDirs, real: &Path) -> PathBuf {
    dirs.locks().join(format!("cfg-{}.lock", path_hash(real)))
}

pub fn load_ledger(dirs: &AppDirs, real: &Path) -> std::io::Result<Option<Ledger>> {
    read_json(&ledger_path(dirs, real))
}

pub fn save_ledger(dirs: &AppDirs, ledger: &Ledger) -> std::io::Result<()> {
    write_json(&ledger_path(dirs, &ledger.real_path), ledger)
}

pub fn delete_ledger(dirs: &AppDirs, real: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(ledger_path(dirs, real)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
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
    let Some(mut ledger) = load_ledger(dirs, &target.real)? else {
        return Ok(UndoOutcome::NotOurs);
    };
    if !ledger.channels.iter().any(|c| c == channel_ref) {
        return Ok(UndoOutcome::NotOurs);
    }
    ledger.channels.retain(|c| c != channel_ref);
    if !ledger.channels.is_empty() {
        save_ledger(dirs, &ledger)?;
        return Ok(UndoOutcome::StillUsed {
            channels: ledger.channels,
        });
    }
    let current = read(target)?;
    let outcome = match current {
        None => UndoOutcome::ChangedSince,
        Some(text) => match undo(&text, ledger.previous.as_deref(), ledger.added_newline) {
            Some(new_text) => {
                if ledger.created && new_text.is_empty() && !target.is_link {
                    std::fs::remove_file(&target.real)?;
                } else {
                    write(target, &new_text)?;
                }
                UndoOutcome::Reverted
            }
            None => UndoOutcome::ChangedSince,
        },
    };
    delete_ledger(dirs, &target.real)?;
    Ok(outcome)
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
}
