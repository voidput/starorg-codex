//! Marked blocks in the LUG Helper pre-launch / post-exit scripts (addendum §2).
//!
//! `hook install` adds one block per script, between [`BEGIN`] and [`END`]; everything outside
//! the markers is the user's and is kept byte for byte. The block goes before a final
//! top-level `exit` line when the script ends with one, otherwise at the end. No blank lines
//! are added, so `hook uninstall` removes exactly the block lines and gives the script back.

use crate::fsutil;
use std::path::{Path, PathBuf};

pub const BEGIN: &str = "# >>> starorg-strings hook >>>";
pub const END: &str = "# <<< starorg-strings hook <<<";

/// Scripts are small; anything bigger is not a hook script.
pub const SCRIPT_LIMIT: u64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Prelaunch,
    Postexit,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Prelaunch, Kind::Postexit];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Prelaunch => "prelaunch",
            Kind::Postexit => "postexit",
        }
    }

    /// LUG Helper's file name in the prefix.
    pub fn script_name(self) -> &'static str {
        match self {
            Kind::Prelaunch => "sc-prelaunch.sh",
            Kind::Postexit => "sc-postexit.sh",
        }
    }

    pub fn default_timeout(self) -> u64 {
        match self {
            Kind::Prelaunch => 60,
            Kind::Postexit => 120,
        }
    }
}

/// POSIX single-quote a word.
pub fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:=+@%".contains(&b))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// What the block calls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockOpts {
    /// Absolute binary path; `None` = `starorg-strings` from `PATH`.
    pub bin: Option<String>,
    /// `--game-dir` to pass (when the install was given explicitly).
    pub game_dir: Option<String>,
    pub timeout: Option<u64>,
    pub notify: bool,
}

/// The block for `kind`, lines joined with `\n`, ending with a newline.
pub fn block(kind: Kind, o: &BlockOpts) -> String {
    let (guard, cmd) = match &o.bin {
        Some(b) => {
            let q = sh_quote(b);
            (format!("if [ -x {q} ]; then"), q)
        }
        None => (
            "if command -v starorg-strings >/dev/null 2>&1; then".to_string(),
            "starorg-strings".to_string(),
        ),
    };
    let mut call = format!("    {cmd} hook {}", kind.as_str());
    if let Some(g) = &o.game_dir {
        call.push_str(&format!(" --game-dir {}", sh_quote(g)));
    }
    call.push_str(" ${WINEPREFIX:+--prefix \"$WINEPREFIX\"}");
    call.push_str(&format!(
        " --timeout {}",
        o.timeout.unwrap_or(kind.default_timeout())
    ));
    if o.notify {
        call.push_str(" --notify");
    }
    call.push_str(" || true");
    let what = match kind {
        Kind::Prelaunch => {
            "# StarOrg Strings: re-apply enhanced strings when the build or settings changed."
        }
        Kind::Postexit => {
            "# StarOrg Strings: read new blueprints from Game.log, then re-apply if needed."
        }
    };
    [
        BEGIN,
        what,
        "# Managed by `starorg-strings hook install`; `hook uninstall` removes it. Edit outside.",
        &guard,
        &call,
        "fi",
        END,
    ]
    .join("\n")
        + "\n"
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case", tag = "state")]
pub enum BlockState {
    Absent,
    Current,
    /// Present but not what `install` would write now (older version, other options).
    Outdated,
    /// Markers unbalanced or repeated: refuse to edit.
    Broken {
        why: String,
    },
}

/// Line spans of the block: (first line index, last line index).
fn find(lines: &[&str]) -> Result<Option<(usize, usize)>, String> {
    let is = |l: &str, m: &str| l.trim_end_matches('\r').trim() == m;
    let begins: Vec<usize> = (0..lines.len()).filter(|&i| is(lines[i], BEGIN)).collect();
    let ends: Vec<usize> = (0..lines.len()).filter(|&i| is(lines[i], END)).collect();
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([b], [e]) if b < e => Ok(Some((*b, *e))),
        _ => Err(format!(
            "{} begin and {} end markers; fix the script by hand",
            begins.len(),
            ends.len()
        )),
    }
}

fn split(text: &str) -> (Vec<&str>, &'static str) {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    (lines, nl)
}

pub fn inspect(text: &str, expected: &str) -> BlockState {
    let (lines, _) = split(text);
    match find(&lines) {
        Err(why) => BlockState::Broken { why },
        Ok(None) => BlockState::Absent,
        Ok(Some((b, e))) => {
            let have: Vec<&str> = lines[b..=e]
                .iter()
                .map(|l| l.trim_end_matches(['\n', '\r']))
                .collect();
            let want: Vec<&str> = expected.lines().collect();
            if have == want {
                BlockState::Current
            } else {
                BlockState::Outdated
            }
        }
    }
}

/// Lines outside the block that call `starorg-strings` (a hand-made snippet like the one in
/// spec 8): 1-based line numbers.
pub fn manual_calls(text: &str) -> Vec<usize> {
    let (lines, _) = split(text);
    let span = find(&lines).ok().flatten();
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| span.is_none_or(|(b, e)| *i < b || *i > e))
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with('#') && t.contains("starorg-strings ")
        })
        .map(|(i, _)| i + 1)
        .collect()
}

/// Lines that look like the other localisation tools' hooks (Smart Citizen, StarStrings).
pub fn other_tools(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in text.lines() {
        let t = l.trim_start();
        if t.starts_with('#') {
            continue;
        }
        if t.contains("sc-smartcitizen.sh") || t.contains("SMARTCITIZEN") {
            if !out.contains(&"Smart Citizen".to_string()) {
                out.push("Smart Citizen".into());
            }
        } else if t.to_ascii_lowercase().contains("starstrings")
            && !out.contains(&"StarStrings".to_string())
        {
            out.push("StarStrings".into());
        }
    }
    out
}

/// `text` with the block installed (or replaced). Unchanged text when already current.
pub fn install(text: &str, block: &str) -> Result<String, String> {
    let (lines, nl) = split(text);
    let block_lines: Vec<String> = block.lines().map(|l| format!("{l}{nl}")).collect();
    if let Some((b, e)) = find(&lines)? {
        let mut out: String = lines[..b].concat();
        out.extend(block_lines);
        out.push_str(&lines[e + 1..].concat());
        return Ok(out);
    }
    // Before a final top-level `exit` (with only blank lines and comments after it).
    let mut at = lines.len();
    for i in (0..lines.len()).rev() {
        let t = lines[i].trim();
        if t.is_empty() || (t.starts_with('#') && !t.starts_with("#!")) {
            continue;
        }
        let top_level = !lines[i].starts_with([' ', '\t']);
        if top_level && (t == "exit" || t.starts_with("exit ") || t.starts_with("exit\t")) {
            at = i;
        }
        break;
    }
    let mut out: String = lines[..at].concat();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(nl);
    }
    out.extend(block_lines);
    out.push_str(&lines[at..].concat());
    Ok(out)
}

/// `text` without the block. `None` when there is no block.
pub fn uninstall(text: &str) -> Result<Option<String>, String> {
    let (lines, _) = split(text);
    let Some((b, e)) = find(&lines)? else {
        return Ok(None);
    };
    let mut out: String = lines[..b].concat();
    out.push_str(&lines[e + 1..].concat());
    Ok(Some(out))
}

/// A script to edit, after links.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Script {
    pub kind: Kind,
    /// As given (may be a link, as on the owner's machine: prefix → ~/git/sc-setup).
    pub path: PathBuf,
    pub real: PathBuf,
    pub is_link: bool,
}

/// Resolve and check a hook script: must exist, be a regular file we own, and not lie inside
/// the game install or the prefix's `drive_c` (we only edit the scripts LUG Helper keeps in
/// the prefix root, and never create files there).
pub fn locate(kind: Kind, path: &Path, forbidden: &[PathBuf]) -> Result<Script, (bool, String)> {
    if !path.exists() {
        return Err((true, format!("{} not found", path.display())));
    }
    let real =
        std::fs::canonicalize(path).map_err(|e| (false, format!("{}: {e}", path.display())))?;
    for f in forbidden {
        let f = std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        if real.starts_with(&f) {
            return Err((
                false,
                format!(
                    "{} is inside {}; refusing to edit it",
                    real.display(),
                    f.display()
                ),
            ));
        }
    }
    fsutil::owned_regular_file(&real).map_err(|e| (false, e))?;
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    Ok(Script {
        kind,
        path: path.to_path_buf(),
        real,
        is_link,
    })
}

pub fn read(script: &Script) -> Result<String, String> {
    let b = fsutil::read_limited(&script.real, SCRIPT_LIMIT)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{} vanished", script.real.display()))?;
    String::from_utf8(b).map_err(|_| format!("{} is not UTF-8 text", script.real.display()))
}

/// Back up the current script into `backup_dir`, then replace it in place (temp file beside
/// the real file, rename; mode kept; a link stays a link). Returns the backup path.
pub fn write(script: &Script, old: &str, new: &str, backup_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(backup_dir).map_err(|e| e.to_string())?;
    let name = script
        .real
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| script.kind.script_name().into());
    let now = fsutil::unix_now();
    let mut backup = backup_dir.join(format!("{}-{name}", fsutil::stamp(now)));
    let mut n = 1;
    while backup.exists() {
        n += 1;
        backup = backup_dir.join(format!("{}-{n}-{name}", fsutil::stamp(now)));
    }
    fsutil::write_atomic(&backup, old.as_bytes()).map_err(|e| e.to_string())?;
    // Re-read: refuse when the script changed under us.
    if read(script)? != old {
        return Err(format!(
            "{} changed while editing; try again",
            script.real.display()
        ));
    }
    let tmp = script
        .real
        .with_file_name(format!(".{name}.starorg-tmp-{}", std::process::id()));
    fsutil::write_via(&tmp, &script.real, new.as_bytes()).map_err(|e| e.to_string())?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "#!/usr/bin/env bash\n# user header\nWINEPREFIX=\"${WINEPREFIX:-$HOME/x}\"\n\necho user stuff\nif true; then\n    exit 0\nfi\n\nexit 0\n";

    fn blk() -> String {
        block(Kind::Prelaunch, &BlockOpts::default())
    }

    #[test]
    fn install_before_final_exit_and_roundtrip() {
        let out = install(SCRIPT, &blk()).unwrap();
        assert!(out.starts_with("#!/usr/bin/env bash\n# user header\n"));
        assert!(out.ends_with(&format!("{END}\nexit 0\n")), "{out}");
        assert!(out.contains("echo user stuff\nif true; then\n    exit 0\nfi\n\n# >>>"));
        assert_eq!(inspect(&out, &blk()), BlockState::Current);
        assert_eq!(install(&out, &blk()).unwrap(), out, "idempotent");
        assert_eq!(uninstall(&out).unwrap().unwrap(), SCRIPT);
        assert_eq!(uninstall(SCRIPT).unwrap(), None);
    }

    #[test]
    fn install_at_end_without_exit() {
        let s = "#!/bin/sh\necho hi";
        let out = install(s, &blk()).unwrap();
        assert!(out.starts_with("#!/bin/sh\necho hi\n# >>>"));
        assert!(out.ends_with(&format!("{END}\n")));
        assert_eq!(uninstall(&out).unwrap().unwrap(), "#!/bin/sh\necho hi\n");
        // Indented exit is not a final top-level exit.
        let s2 = "#!/bin/sh\nif x; then\n  exit 1\nfi\n";
        let out2 = install(s2, &blk()).unwrap();
        assert!(out2.starts_with(s2));
        assert_eq!(uninstall(&out2).unwrap().unwrap(), s2);
    }

    #[test]
    fn outdated_is_replaced_in_place() {
        let old = install(SCRIPT, &blk()).unwrap();
        let newer = block(
            Kind::Prelaunch,
            &BlockOpts {
                bin: Some("/opt/My Tools/starorg-strings".into()),
                timeout: Some(30),
                notify: true,
                ..BlockOpts::default()
            },
        );
        assert_eq!(inspect(&old, &newer), BlockState::Outdated);
        let out = install(&old, &newer).unwrap();
        assert!(out.contains("if [ -x '/opt/My Tools/starorg-strings' ]; then"));
        assert!(out.contains("--timeout 30 --notify || true"));
        assert_eq!(inspect(&out, &newer), BlockState::Current);
        assert_eq!(uninstall(&out).unwrap().unwrap(), SCRIPT);
    }

    #[test]
    fn broken_markers_refused() {
        let s = format!("{SCRIPT}{BEGIN}\necho\n");
        assert!(matches!(inspect(&s, &blk()), BlockState::Broken { .. }));
        assert!(install(&s, &blk()).is_err());
        assert!(uninstall(&s).is_err());
        let two = format!("{BEGIN}\n{END}\n{BEGIN}\n{END}\n");
        assert!(install(&two, &blk()).is_err());
    }

    #[test]
    fn crlf_kept() {
        let s = "#!/bin/sh\r\necho a\r\nexit 0\r\n";
        let out = install(s, &blk()).unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'), "only CRLF endings");
        assert_eq!(uninstall(&out).unwrap().unwrap(), s);
    }

    #[test]
    fn manual_and_other_tools() {
        let s = format!(
            "{SCRIPT}starorg-strings apply --all --if-stale\n# starorg-strings commented\n\"$SMARTCITIZEN\" check\n"
        );
        let out = install(&s, &blk()).unwrap();
        assert_eq!(manual_calls(&out).len(), 1);
        assert_eq!(other_tools(&out), vec!["Smart Citizen".to_string()]);
        assert!(manual_calls(&install(SCRIPT, &blk()).unwrap()).is_empty());
    }

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("/usr/bin/x"), "/usr/bin/x");
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        let b = block(
            Kind::Postexit,
            &BlockOpts {
                game_dir: Some("/g/Roberts Space Industries/StarCitizen".into()),
                ..BlockOpts::default()
            },
        );
        assert!(b.contains("hook postexit --game-dir '/g/Roberts Space Industries/StarCitizen' ${WINEPREFIX:+--prefix \"$WINEPREFIX\"} --timeout 120 || true"));
    }

    #[cfg(unix)]
    #[test]
    fn write_follows_link_keeps_mode_and_backs_up() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("repo/sc-prelaunch.sh");
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, SCRIPT).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
        let prefix = d.path().join("prefix");
        std::fs::create_dir_all(prefix.join("drive_c")).unwrap();
        let link = prefix.join("sc-prelaunch.sh");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let s = locate(Kind::Prelaunch, &link, &[prefix.join("drive_c")]).unwrap();
        assert!(s.is_link);
        let old = read(&s).unwrap();
        let new = install(&old, &blk()).unwrap();
        let bak = write(&s, &old, &new, &d.path().join("backups")).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), SCRIPT);
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&real).unwrap(), new);
        let mode = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        // Inside drive_c: refused. Missing: not found.
        let inside = prefix.join("drive_c/x.sh");
        std::fs::write(&inside, "x").unwrap();
        assert!(locate(Kind::Prelaunch, &inside, &[prefix.join("drive_c")]).is_err());
        assert!(matches!(
            locate(Kind::Prelaunch, &prefix.join("nope.sh"), &[]),
            Err((true, _))
        ));
    }
}
