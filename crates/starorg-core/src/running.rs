//! Is the game running, is `Data.p4k` being written, is the launcher installing? (spec 7.1
//! step 3, 10.6)
//!
//! **Linux.** The game is a Wine process whose command line holds a *Windows* path such as
//! `C:\...\StarCitizen\LIVE\Bin64\StarCitizen.exe`. For each `/proc/<pid>/cmdline` whose first
//! argument ends in `StarCitizen.exe` (split on `\` and `/`, case ignored), the channel is the
//! path part before `Bin64` and the prefix is `WINEPREFIX` from `/proc/<pid>/environ`
//! (default `~/.wine`), canonicalised and compared with the install's prefix. If the
//! environment cannot be read or the path cannot be parsed, the game counts as running for
//! every channel: wrongly skipping an apply is the safe mistake.
//!
//! **Windows.** `StarCitizen.exe` whose image path is inside the channel folder.
//!
//! `RSI Launcher.exe` is found the same way and only gives a warning.

use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exe {
    Game,
    Launcher,
}

/// What a command line says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matched {
    pub exe: Exe,
    /// Channel folder name before `Bin64`; `None` when it cannot be parsed.
    pub channel: Option<String>,
}

fn components(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|c| !c.is_empty()).collect()
}

/// Classify one argument (a Windows or Unix path).
pub fn classify_arg(arg: &str) -> Option<Matched> {
    let comps = components(arg);
    let last = comps.last()?;
    let exe = if last.eq_ignore_ascii_case("StarCitizen.exe") {
        Exe::Game
    } else if last.eq_ignore_ascii_case("RSI Launcher.exe") {
        Exe::Launcher
    } else {
        return None;
    };
    let channel = comps
        .iter()
        .rposition(|c| c.eq_ignore_ascii_case("Bin64"))
        .and_then(|i| i.checked_sub(1))
        .map(|i| comps[i].to_string());
    Some(Matched { exe, channel })
}

/// Classify a NUL-separated `/proc/<pid>/cmdline`. The first argument is used; when it is a
/// Wine loader (`wine`, `wine64`, `wine64-preloader`, ...) the second is used instead.
pub fn classify_cmdline(cmdline: &[u8]) -> Option<Matched> {
    let args: Vec<String> = cmdline
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let first = args.first()?;
    if let Some(m) = classify_arg(first) {
        return Some(m);
    }
    let base = components(first).last().map(|s| s.to_ascii_lowercase())?;
    if base.starts_with("wine") {
        return args.get(1).and_then(|a| classify_arg(a));
    }
    None
}

/// `WINEPREFIX` from a NUL-separated environment block, or the Wine default `$HOME/.wine`.
pub fn prefix_from_environ(environ: &[u8]) -> Option<PathBuf> {
    let mut home = None;
    for kv in environ.split(|&b| b == 0) {
        if let Some(v) = kv.strip_prefix(b"WINEPREFIX=")
            && !v.is_empty()
        {
            return Some(PathBuf::from(String::from_utf8_lossy(v).into_owned()));
        }
        if let Some(v) = kv.strip_prefix(b"HOME=") {
            home = Some(PathBuf::from(String::from_utf8_lossy(v).into_owned()));
        }
    }
    home.map(|h| h.join(".wine"))
}

/// One relevant process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub pid: u32,
    pub exe: Exe,
    pub channel: Option<String>,
    /// Canonical prefix; `None` when the environment could not be read.
    pub prefix: Option<PathBuf>,
    /// Windows: the image path.
    pub image: Option<PathBuf>,
}

/// List game and launcher processes.
#[cfg(target_os = "linux")]
pub fn scan() -> Vec<Seen> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return out;
    };
    let me = std::process::id();
    for e in rd.filter_map(Result::ok) {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(cmd) = std::fs::read(e.path().join("cmdline")) else {
            continue;
        };
        let Some(m) = classify_cmdline(&cmd) else {
            continue;
        };
        let prefix = std::fs::read(e.path().join("environ"))
            .ok()
            .and_then(|env| prefix_from_environ(&env))
            .map(|p| std::fs::canonicalize(&p).unwrap_or(p));
        out.push(Seen {
            pid,
            exe: m.exe,
            channel: m.channel,
            prefix,
            image: None,
        });
    }
    out
}

#[cfg(windows)]
pub fn scan() -> Vec<Seen> {
    // No extra crate and no unsafe code: ask PowerShell for the image paths.
    let o = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-Process -Name StarCitizen,'RSI Launcher' -ErrorAction SilentlyContinue | ForEach-Object { \"$($_.Id)|$($_.Path)\" }",
        ])
        .output();
    let Ok(o) = o else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| {
            let (pid, path) = l.trim().split_once('|')?;
            let m = classify_arg(path).or_else(|| {
                // Path can be empty for processes we may not query: count as the game.
                path.is_empty().then_some(Matched {
                    exe: Exe::Game,
                    channel: None,
                })
            })?;
            Some(Seen {
                pid: pid.parse().ok()?,
                exe: m.exe,
                channel: m.channel,
                prefix: None,
                image: (!path.is_empty()).then(|| PathBuf::from(path)),
            })
        })
        .collect()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn scan() -> Vec<Seen> {
    Vec::new()
}

/// Does `seen` block the channel `channel` of an install in `prefix`?
pub fn blocks(seen: &Seen, prefix: Option<&Path>, channel: &str, channel_dir: &Path) -> bool {
    if seen.exe != Exe::Game {
        return false;
    }
    if let Some(img) = &seen.image {
        // Windows: image inside the channel folder.
        return img.starts_with(channel_dir);
    }
    let Some(ch) = &seen.channel else {
        return true;
    };
    if !ch.eq_ignore_ascii_case(channel) {
        return false;
    }
    match (&seen.prefix, prefix) {
        (None, _) => true,
        (Some(_), None) => true,
        (Some(p), Some(want)) => {
            let want = std::fs::canonicalize(want).unwrap_or_else(|_| want.to_path_buf());
            *p == want
        }
    }
}

/// The game process blocking this channel, if any.
pub fn game_running(prefix: Option<&Path>, channel: &str, channel_dir: &Path) -> Option<Seen> {
    scan()
        .into_iter()
        .find(|s| blocks(s, prefix, channel, channel_dir))
}

pub fn launcher_running() -> bool {
    scan().iter().any(|s| s.exe == Exe::Launcher)
}

/// Is `Data.p4k` being written? Size or mtime change between two reads `gap` apart.
pub fn p4k_changing(p4k: &Path, gap: Duration) -> bool {
    let read = || {
        std::fs::metadata(p4k)
            .ok()
            .map(|m| (m.len(), m.modified().ok()))
    };
    let a = read();
    std::thread::sleep(gap);
    a != read()
}

/// Logs newer than this are considered live for the pipeline check.
pub const PIPELINE_FRESH: Duration = Duration::from_secs(30 * 60);

/// Does a launcher log show an unfinished install pipeline for `channel`?
pub fn pipeline_running(logs: &[PathBuf], channel: &str) -> bool {
    logs.iter().any(|log| {
        let fresh = std::fs::metadata(log)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age < PIPELINE_FRESH);
        fresh
            && crate::discovery::launcher_log::read_tail_text(log)
                .is_some_and(|t| crate::discovery::launcher_log::pipeline_in_progress(&t, channel))
    })
}

/// Walk up from `path` to the Wine prefix that contains it, if any.
pub fn enclosing_prefix(path: &Path) -> Option<PathBuf> {
    let mut cur = Some(path);
    while let Some(p) = cur {
        if crate::discovery::is_wine_prefix(p) {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wine_cmdlines() {
        let m = classify_cmdline(
            b"C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE\\Bin64\\StarCitizen.exe\0-no_login_dialog\0",
        )
        .unwrap();
        assert_eq!(m.exe, Exe::Game);
        assert_eq!(m.channel.as_deref(), Some("LIVE"));
        let m = classify_cmdline(
            b"/usr/bin/wine64-preloader\0Z:\\games\\sc\\hotfix\\bin64\\starcitizen.EXE\0",
        )
        .unwrap();
        assert_eq!(m.channel.as_deref(), Some("hotfix"));
        let m = classify_cmdline(
            b"C:\\Program Files\\Roberts Space Industries\\RSI Launcher\\RSI Launcher.exe\0",
        )
        .unwrap();
        assert_eq!(m.exe, Exe::Launcher);
        assert!(classify_cmdline(b"/usr/bin/vim\0StarCitizen.exe\0").is_none());
        assert_eq!(
            classify_cmdline(b"StarCitizen.exe\0").unwrap().channel,
            None
        );
    }

    #[test]
    fn environ_prefix() {
        assert_eq!(
            prefix_from_environ(b"A=1\0WINEPREFIX=/p\0HOME=/h\0"),
            Some(PathBuf::from("/p"))
        );
        assert_eq!(
            prefix_from_environ(b"HOME=/h\0"),
            Some(PathBuf::from("/h/.wine"))
        );
        assert_eq!(prefix_from_environ(b""), None);
    }

    #[test]
    fn blocking_rules() {
        let t = tempfile::tempdir().unwrap();
        let p = std::fs::canonicalize(t.path()).unwrap();
        let other = p.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let seen = |channel: Option<&str>, prefix: Option<PathBuf>| Seen {
            pid: 1,
            exe: Exe::Game,
            channel: channel.map(str::to_string),
            prefix,
            image: None,
        };
        let dir = Path::new("/x/LIVE");
        assert!(blocks(
            &seen(Some("LIVE"), Some(p.clone())),
            Some(&p),
            "LIVE",
            dir
        ));
        assert!(!blocks(
            &seen(Some("PTU"), Some(p.clone())),
            Some(&p),
            "LIVE",
            dir
        ));
        assert!(!blocks(
            &seen(Some("LIVE"), Some(other.clone())),
            Some(&p),
            "LIVE",
            dir
        ));
        // Unknown environment or path: running for every channel.
        assert!(blocks(&seen(Some("LIVE"), None), Some(&p), "LIVE", dir));
        assert!(blocks(&seen(None, Some(other)), Some(&p), "LIVE", dir));
    }
}
