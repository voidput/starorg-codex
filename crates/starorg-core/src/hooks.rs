//! Marked blocks in the LUG Helper pre-launch / post-exit scripts (addendum §2).
//!
//! `hook install` adds one block per script, between [`BEGIN`] and [`END`]; everything outside
//! the markers is the user's and is kept byte for byte. The block goes before a final
//! top-level `exit` line when the script ends with one, otherwise at the end. No blank lines
//! are added, so `hook uninstall` removes exactly the block lines and gives the script back.

use crate::fsutil;
use std::path::{Path, PathBuf};

pub const BEGIN: &str = "# >>> starorg-codex hook >>>";
pub const END: &str = "# <<< starorg-codex hook <<<";
/// The markers before 0.3.0. A block between them is found like a current one, reported as
/// [`BlockState::Outdated`], and `install` replaces it in place with the new block.
pub const LEGACY_BEGIN: &str = "# >>> starorg-strings hook >>>";
pub const LEGACY_END: &str = "# <<< starorg-strings hook <<<";

/// The CLI the block calls (from `PATH` unless `--bin` is given).
pub const CLI: &str = "starorg-codex";
/// The CLI's name before 0.3.0; still installed as an alias for one release.
pub const LEGACY_CLI: &str = "starorg-strings";

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
    /// Absolute binary path; `None` = [`CLI`] from `PATH`.
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
            format!("if command -v {CLI} >/dev/null 2>&1; then"),
            CLI.to_string(),
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
            "# StarOrg Codex: re-apply enhanced strings when the build or settings changed."
        }
        Kind::Postexit => {
            "# StarOrg Codex: read new blueprints from Game.log, then re-apply if needed."
        }
    };
    [
        BEGIN,
        what,
        "# Managed by `starorg-codex hook install`; `hook uninstall` removes it. Edit outside.",
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

/// Line spans of the block: (first line index, last line index). Old (pre-0.3.0) and new
/// markers count alike, so an old block is found, and replaced, like a current one.
fn find(lines: &[&str]) -> Result<Option<(usize, usize)>, String> {
    let is = |l: &str, m: &[&str]| m.contains(&l.trim_end_matches('\r').trim());
    let begins: Vec<usize> = (0..lines.len())
        .filter(|&i| is(lines[i], &[BEGIN, LEGACY_BEGIN]))
        .collect();
    let ends: Vec<usize> = (0..lines.len())
        .filter(|&i| is(lines[i], &[END, LEGACY_END]))
        .collect();
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

fn split(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The line without its line ending.
fn bare(l: &str) -> &str {
    l.trim_end_matches(['\n', '\r'])
}

/// The line ends in a shell line continuation (`\` right before the line ending).
fn continues(l: &str) -> bool {
    let b = bare(l);
    let slashes = b.len() - b.trim_end_matches('\\').len();
    slashes % 2 == 1
}

pub fn inspect(text: &str, expected: &str) -> BlockState {
    let lines = split(text);
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

/// The block (or a stray marker) uses the pre-0.3.0 markers.
pub fn has_legacy_markers(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_end_matches('\r').trim();
        t == LEGACY_BEGIN || t == LEGACY_END
    })
}

/// Lines outside the block that call the CLI, by its name or its pre-0.3.0 name (a
/// hand-made snippet like the one in spec 8): 1-based line numbers.
pub fn manual_calls(text: &str) -> Vec<usize> {
    let lines = split(text);
    let span = find(&lines).ok().flatten();
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| span.is_none_or(|(b, e)| *i < b || *i > e))
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with('#')
                && (t.contains(&format!("{CLI} ")) || t.contains(&format!("{LEGACY_CLI} ")))
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
///
/// The block's lines always end in `\n`, also in a CRLF script: bash reads `then\r` as a
/// syntax error, and a mixed file still gives the user's lines back byte for byte. When the
/// script has no final newline, the block goes at the very end with one `\n` before it and
/// none after its last line, so `uninstall` can drop exactly that `\n` again.
pub fn install(text: &str, block: &str) -> Result<String, String> {
    let lines = split(text);
    let block_lines: Vec<String> = block.lines().map(|l| format!("{l}\n")).collect();
    let unterminated = |mut v: Vec<String>| {
        if let Some(last) = v.last_mut() {
            last.pop();
        }
        v
    };
    if let Some((b, e)) = find(&lines)? {
        let at_eof_open = e + 1 == lines.len() && !lines[e].ends_with('\n');
        let mut out: String = lines[..b].concat();
        out.extend(if at_eof_open {
            unterminated(block_lines)
        } else {
            block_lines
        });
        out.push_str(&lines[e + 1..].concat());
        return Ok(out);
    }
    // Before a final top-level `exit` (with only blank lines and comments after it). The
    // exit may end a command continued over several lines (`a && \` + `exit 0`): the block
    // then goes before the whole command, never inside it.
    let mut at = lines.len();
    for i in (0..lines.len()).rev() {
        let t = lines[i].trim();
        if t.is_empty() || (t.starts_with('#') && !t.starts_with("#!")) {
            continue;
        }
        let mut start = i;
        while start > 0 && continues(lines[start - 1]) {
            start -= 1;
        }
        let first = lines[start];
        let top_level = !first.starts_with([' ', '\t']);
        let is_exit = |l: &str| {
            let t = l.trim();
            t == "exit" || t.starts_with("exit ") || t.starts_with("exit\t")
        };
        if top_level && (is_exit(lines[i]) && (start == i || !continues(lines[i]))) {
            at = start;
        }
        break;
    }
    // Never put the block right after a continuation line (it would join that command).
    if at == lines.len() && lines.last().is_some_and(|l| continues(l)) {
        return Err("the script ends inside a continued line (`\\`); fix it by hand".into());
    }
    let mut out: String = lines[..at].concat();
    if !out.is_empty() && !out.ends_with('\n') {
        // Only possible at the end of the script: no final newline.
        out.push('\n');
        out.extend(unterminated(block_lines));
        return Ok(out);
    }
    out.extend(block_lines);
    out.push_str(&lines[at..].concat());
    Ok(out)
}

/// `text` without the block. `None` when there is no block.
pub fn uninstall(text: &str) -> Result<Option<String>, String> {
    let lines = split(text);
    let Some((b, e)) = find(&lines)? else {
        return Ok(None);
    };
    let mut out: String = lines[..b].concat();
    if e + 1 == lines.len() && !lines[e].ends_with('\n') && b > 0 {
        // Installed at the end of a script without a final newline: drop the `\n` we added.
        out.pop();
    }
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
    let real = crate::fsutil::canonicalize(path)
        .map_err(|e| (false, format!("{}: {e}", path.display())))?;
    for f in forbidden {
        let f = crate::fsutil::canonicalize(f).unwrap_or_else(|_| f.clone());
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
        // No final newline: byte for byte back, also after a re-install.
        let s = "#!/bin/sh\necho hi";
        let out = install(s, &blk()).unwrap();
        assert!(out.starts_with("#!/bin/sh\necho hi\n# >>>"));
        assert!(out.ends_with(END), "{out:?}");
        assert_eq!(uninstall(&out).unwrap().unwrap(), s);
        assert_eq!(inspect(&out, &blk()), BlockState::Current);
        assert_eq!(install(&out, &blk()).unwrap(), out, "idempotent");
        let s = "#!/bin/sh\necho hi\n";
        let out = install(s, &blk()).unwrap();
        assert!(out.ends_with(&format!("{END}\n")));
        assert_eq!(uninstall(&out).unwrap().unwrap(), s);
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
                bin: Some("/opt/My Tools/starorg-codex".into()),
                timeout: Some(30),
                notify: true,
                ..BlockOpts::default()
            },
        );
        assert_eq!(inspect(&old, &newer), BlockState::Outdated);
        let out = install(&old, &newer).unwrap();
        assert!(out.contains("if [ -x '/opt/My Tools/starorg-codex' ]; then"));
        assert!(out.contains("--timeout 30 --notify || true"));
        assert_eq!(inspect(&out, &newer), BlockState::Current);
        assert_eq!(uninstall(&out).unwrap().unwrap(), SCRIPT);
    }

    /// The block 0.2.x wrote (old markers, old CLI name), as in a real LUG Helper script.
    const OLD_BLOCK: &str = "# >>> starorg-strings hook >>>\n# StarOrg Codex: re-apply enhanced strings when the build or settings changed.\n# Managed by `starorg-strings hook install`; `hook uninstall` removes it. Edit outside.\nif command -v starorg-strings >/dev/null 2>&1; then\n    starorg-strings hook prelaunch ${WINEPREFIX:+--prefix \"$WINEPREFIX\"} --timeout 60 || true\nfi\n# <<< starorg-strings hook <<<\n";

    #[test]
    fn old_marker_block_is_outdated_and_replaced_in_place() {
        let s = format!("#!/bin/bash\necho user\n{OLD_BLOCK}exit 0\n");
        assert_eq!(inspect(&s, &blk()), BlockState::Outdated);
        assert!(has_legacy_markers(&s));
        assert!(
            manual_calls(&s).is_empty(),
            "the old block is not a hand-made call"
        );
        let out = install(&s, &blk()).unwrap();
        assert_eq!(out, format!("#!/bin/bash\necho user\n{}exit 0\n", blk()));
        assert!(!out.contains("starorg-strings"), "{out}");
        assert!(out.contains("if command -v starorg-codex >/dev/null 2>&1; then"));
        assert!(out.contains("    starorg-codex hook prelaunch "));
        assert_eq!(inspect(&out, &blk()), BlockState::Current);
        assert!(!has_legacy_markers(&out));
        // Uninstall removes an old block too.
        assert_eq!(
            uninstall(&s).unwrap().unwrap(),
            "#!/bin/bash\necho user\nexit 0\n"
        );
        // An old block next to a new one is two blocks: refused, fixed by hand.
        let two = format!("{OLD_BLOCK}{}", blk());
        assert!(matches!(inspect(&two, &blk()), BlockState::Broken { .. }));
        assert!(install(&two, &blk()).is_err());
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
    fn crlf_script_gets_an_lf_block_and_round_trips() {
        let s = "#!/bin/sh\r\necho a\r\nexit 0\r\n";
        let out = install(s, &blk()).unwrap();
        let start = out.find(BEGIN).unwrap();
        let end = out.find(END).unwrap() + END.len() + 1;
        assert!(
            !out[start..end].contains('\r'),
            "block lines end in LF: {out:?}"
        );
        assert!(out.ends_with(&format!("{END}\nexit 0\r\n")));
        assert_eq!(inspect(&out, &blk()), BlockState::Current);
        assert_eq!(uninstall(&out).unwrap().unwrap(), s);
    }

    #[test]
    fn continued_exit_line_is_not_split() {
        let s = "#!/bin/bash\necho a && \\\nexit 0\n";
        let out = install(s, &blk()).unwrap();
        assert!(
            out.ends_with(&format!("{END}\necho a && \\\nexit 0\n")),
            "{out}"
        );
        assert_eq!(uninstall(&out).unwrap().unwrap(), s);
        // Two continuation lines, and an escaped backslash that is not a continuation.
        let s = "#!/bin/bash\ntrue && \\\n  true && \\\nexit 0\n";
        let out = install(s, &blk()).unwrap();
        assert!(out.starts_with(&format!("#!/bin/bash\n{BEGIN}")), "{out}");
        let s = "#!/bin/bash\necho a\\\\\nexit 0\n";
        let out = install(s, &blk()).unwrap();
        assert!(out.ends_with(&format!("{END}\nexit 0\n")), "{out}");
        // A script that ends inside a continued line is refused.
        assert!(install("#!/bin/bash\necho a \\\n", &blk()).is_err());
    }

    #[test]
    fn manual_and_other_tools() {
        let s = format!(
            "{SCRIPT}starorg-strings apply --all --if-stale\n# starorg-strings commented\nstarorg-codex apply --all\n\"$SMARTCITIZEN\" check\n"
        );
        let out = install(&s, &blk()).unwrap();
        assert_eq!(manual_calls(&out).len(), 2, "old and new CLI names");
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

#[cfg(all(test, unix))]
mod bash_tests {
    use super::*;

    /// `bash -n` accepts the installed block in LF, CRLF and no-final-newline scripts.
    #[test]
    fn installed_scripts_parse() {
        let Ok(bash) = which_bash() else { return };
        let blk = block(Kind::Prelaunch, &BlockOpts::default());
        for s in [
            "#!/bin/bash\necho a\nexit 0\n",
            "#!/bin/bash\r\necho a\r\n",
            "#!/bin/bash\necho a",
            "#!/bin/bash\necho a && \\\nexit 0\n",
        ] {
            let out = install(s, &blk).unwrap();
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("s.sh");
            std::fs::write(&p, &out).unwrap();
            let st = std::process::Command::new(&bash)
                .arg("-n")
                .arg(&p)
                .status()
                .unwrap();
            // A CRLF line of the user's own is not a syntax error for `echo`; our block
            // must not add one.
            assert!(st.success(), "{out:?}");
        }
    }

    fn which_bash() -> Result<std::path::PathBuf, ()> {
        ["/bin/bash", "/usr/bin/bash"]
            .iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.exists())
            .ok_or(())
    }
}
