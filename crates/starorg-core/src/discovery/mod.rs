//! Install and channel discovery (spec 4.1, 10.1).
//!
//! An **install** is a game library root (the folder that holds `LIVE`). A **channel** is any
//! direct subdirectory of an install that contains `Data.p4k`. StarBreaker's own discovery is
//! not used: it skips HOTFIX and non-default roots.
//!
//! Order (first valid result is the default; all are listed):
//!
//! 1. `--game-dir` (skips everything else).
//! 2. Linux: `--prefix` or `$WINEPREFIX`; the LUG Helper config; `~/Games/star-citizen` and
//!    `~/games/star-citizen`. For each Wine prefix: the RSI Launcher logs inside it, then the
//!    default library path.
//! 3. Windows: the RSI Launcher logs in `%APPDATA%`, then common paths on every drive.
//!
//! All roots are canonicalised before comparing, so `~/games/star-citizen` and
//! `/mnt/games/star-citizen` (the same prefix through a symlink) appear once.

pub mod launcher_log;

use crate::build::BuildInfo;
use crate::paths::{find_existing_ci, resolve_ci};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Channel names the launcher uses, in display order. Unknown names are accepted and sorted
/// after these.
pub const KNOWN_CHANNELS: &[&str] = &["LIVE", "HOTFIX", "PTU", "EPTU", "TECH-PREVIEW"];

/// Default library root inside a Wine prefix.
const PREFIX_LIBRARY: &[&str] = &[
    "drive_c",
    "Program Files",
    "Roberts Space Industries",
    "StarCitizen",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetOs {
    Linux,
    Windows,
}

/// Process environment that discovery reads. Built from the real process with
/// [`DiscoveryEnv::from_process`]; tests build it by hand.
#[derive(Debug, Clone)]
pub struct DiscoveryEnv {
    pub os: TargetOs,
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub wineprefix: Option<PathBuf>,
    /// Windows `%APPDATA%`.
    pub appdata: Option<PathBuf>,
    /// Windows drive roots to scan (`C:\`, `D:\`, ...).
    pub drives: Vec<PathBuf>,
}

impl DiscoveryEnv {
    pub fn from_process() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let os = if cfg!(windows) {
            TargetOs::Windows
        } else {
            TargetOs::Linux
        };
        let drives = if os == TargetOs::Windows {
            (b'C'..=b'Z')
                .map(|d| PathBuf::from(format!("{}:\\", d as char)))
                .filter(|p| p.is_dir())
                .collect()
        } else {
            Vec::new()
        };
        Self {
            os,
            home: var(if os == TargetOs::Windows {
                "USERPROFILE"
            } else {
                "HOME"
            }),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            wineprefix: var("WINEPREFIX"),
            appdata: var("APPDATA"),
            drives,
        }
    }
}

/// What the user passed on the command line.
#[derive(Debug, Clone, Default)]
pub struct DiscoveryRequest {
    pub game_dir: Option<PathBuf>,
    pub prefix: Option<PathBuf>,
}

/// How an install was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FoundBy {
    GameDir,
    PrefixOption,
    WinePrefixEnv,
    LugConfig,
    DefaultPrefix,
    LauncherLog,
    DefaultPath,
}

impl FoundBy {
    pub fn describe(&self) -> &'static str {
        match self {
            FoundBy::GameDir => "--game-dir",
            FoundBy::PrefixOption => "--prefix",
            FoundBy::WinePrefixEnv => "$WINEPREFIX",
            FoundBy::LugConfig => "LUG Helper config",
            FoundBy::DefaultPrefix => "default prefix location",
            FoundBy::LauncherLog => "RSI Launcher log",
            FoundBy::DefaultPath => "default install path",
        }
    }
}

/// Identity of a channel's game data: change number + `Data.p4k` size + mtime (spec 4.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Fingerprint {
    pub change: Option<u64>,
    pub p4k_size: u64,
    pub p4k_mtime_ns: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    pub name: String,
    pub dir: PathBuf,
    pub p4k: PathBuf,
    pub build: Option<BuildInfo>,
    /// Why `build` is missing, when it is.
    pub build_error: Option<String>,
}

impl Channel {
    /// Read a channel directory. `None` when it has no `Data.p4k`.
    pub fn read(dir: &Path) -> Option<Self> {
        let p4k = find_existing_ci(dir, &["Data.p4k"]).ok().flatten()?;
        if !p4k.is_file() {
            return None;
        }
        let name = dir.file_name()?.to_string_lossy().into_owned();
        let (build, build_error) = match find_existing_ci(dir, &["build_manifest.id"]) {
            Ok(Some(path)) => match BuildInfo::read(&path) {
                Ok(b) => (Some(b), None),
                Err(e) => (None, Some(e.to_string())),
            },
            Ok(None) => (None, Some("no build_manifest.id".to_string())),
            Err(e) => (None, Some(e.to_string())),
        };
        Some(Self {
            name,
            dir: dir.to_path_buf(),
            p4k,
            build,
            build_error,
        })
    }

    pub fn fingerprint(&self) -> std::io::Result<Fingerprint> {
        let meta = std::fs::metadata(&self.p4k)?;
        let mtime = meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Ok(Fingerprint {
            change: self.build.as_ref().and_then(|b| b.change),
            p4k_size: meta.len(),
            p4k_mtime_ns: mtime,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Install {
    /// Canonical library root.
    pub root: PathBuf,
    /// Wine prefix the install sits in, when known.
    pub prefix: Option<PathBuf>,
    pub found_by: FoundBy,
    pub channels: Vec<Channel>,
}

impl Install {
    pub fn channel(&self, name: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.name == name).or_else(|| {
            self.channels
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
        })
    }
}

/// List the channels of a library root, known names first in launcher order.
pub fn list_channels(root: &Path) -> Vec<Channel> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut channels: Vec<Channel> = rd
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Channel::read(&e.path()))
        .collect();
    channels.sort_by_key(|c| {
        let rank = KNOWN_CHANNELS
            .iter()
            .position(|k| k.eq_ignore_ascii_case(&c.name))
            .unwrap_or(KNOWN_CHANNELS.len());
        (rank, c.name.clone())
    });
    channels
}

/// Is `dir` a Wine prefix (has `drive_c/` and `system.reg`)?
pub fn is_wine_prefix(dir: &Path) -> bool {
    matches!(find_existing_ci(dir, &["drive_c"]), Ok(Some(p)) if p.is_dir())
        && matches!(find_existing_ci(dir, &["system.reg"]), Ok(Some(p)) if p.is_file())
}

/// The Wine prefix `path` lies in: the nearest ancestor (or `path` itself) that is a
/// prefix, or else the parent of a `drive_c` component (case-insensitive).
pub fn enclosing_wine_prefix(path: &Path) -> Option<PathBuf> {
    for a in path.ancestors() {
        if is_wine_prefix(a) {
            return Some(a.to_path_buf());
        }
    }
    path.ancestors()
        .find(|a| {
            a.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("drive_c"))
        })
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

impl Install {
    /// Directories nothing but an apply may write into: the install root and, on Linux,
    /// the whole Wine prefix it sits in (found or derived from the path).
    pub fn protected_roots(&self) -> Vec<PathBuf> {
        let mut out = vec![self.root.clone()];
        if let Some(p) = self
            .prefix
            .clone()
            .or_else(|| enclosing_wine_prefix(&self.root))
        {
            out.push(p);
        }
        for p in out.clone() {
            if let Ok(c) = std::fs::canonicalize(&p)
                && !out.contains(&c)
            {
                out.push(c);
            }
        }
        out
    }
}

/// Map a Windows path from inside a prefix to the host path: `C:` is `<prefix>/drive_c`,
/// `Z:` is `/`, other letters go through `<prefix>/dosdevices/<x>:`. Components are matched
/// case-insensitively.
pub fn map_windows_path(prefix: &Path, win: &str) -> Option<PathBuf> {
    let bytes = win.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let letter = bytes[0].to_ascii_lowercase();
    let rest: Vec<&str> = win[2..]
        .split(['\\', '/'])
        .filter(|c| !c.is_empty())
        .collect();
    let (base, prefix_comps): (PathBuf, Vec<String>) = match letter {
        b'c' => (prefix.to_path_buf(), vec!["drive_c".to_string()]),
        b'z' => (PathBuf::from("/"), Vec::new()),
        l => (prefix.join("dosdevices"), vec![format!("{}:", l as char)]),
    };
    let comps: Vec<&str> = prefix_comps
        .iter()
        .map(String::as_str)
        .chain(rest)
        .collect();
    resolve_ci(&base, &comps).ok().map(|r| r.path)
}

/// Launcher log files under a prefix's users (skipping `Public`).
fn prefix_launcher_logs(prefix: &Path) -> Vec<PathBuf> {
    let Ok(Some(users)) = find_existing_ci(prefix, &["drive_c", "users"]) else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(users) else {
        return Vec::new();
    };
    let mut user_dirs: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .filter(|e| {
            !e.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("public")
        })
        .map(|e| e.path())
        .collect();
    user_dirs.sort();
    let mut logs = Vec::new();
    for u in user_dirs {
        for name in ["log.log", "log.old.log"] {
            if let Ok(Some(p)) =
                find_existing_ci(&u, &["AppData", "Roaming", "rsilauncher", "logs", name])
            {
                logs.push(p);
            }
        }
    }
    logs
}

/// Read the first non-empty line of a LUG Helper config file.
fn read_conf_line(path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(PathBuf::from)
}

/// Common library paths on Windows drives (spec 10.1 step 3).
pub fn windows_common_roots(drives: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in drives {
        out.push(
            d.join("Program Files")
                .join("Roberts Space Industries")
                .join("StarCitizen"),
        );
        out.push(d.join("Roberts Space Industries").join("StarCitizen"));
        out.push(
            d.join("Games")
                .join("Roberts Space Industries")
                .join("StarCitizen"),
        );
        // One level of <drive>\<folder>\StarCitizen.
        if let Ok(rd) = std::fs::read_dir(d) {
            let mut sub: Vec<PathBuf> = rd
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .filter_map(|p| find_existing_ci(&p, &["StarCitizen"]).ok().flatten())
                .collect();
            sub.sort();
            out.extend(sub);
        }
    }
    out
}

struct Candidate {
    root: PathBuf,
    prefix: Option<PathBuf>,
    found_by: FoundBy,
}

/// Result of a discovery run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Discovery {
    /// Valid installs (at least one channel), default first.
    pub installs: Vec<Install>,
}

impl Discovery {
    pub fn default_install(&self) -> Option<&Install> {
        self.installs.first()
    }
}

/// Run discovery. Never fails; an empty result means nothing was found (CLI exit 4).
pub fn discover(req: &DiscoveryRequest, env: &DiscoveryEnv) -> Discovery {
    let mut cands: Vec<Candidate> = Vec::new();

    if let Some(dir) = &req.game_dir {
        cands.push(Candidate {
            root: dir.clone(),
            prefix: None,
            found_by: FoundBy::GameDir,
        });
        return finish(cands);
    }

    match env.os {
        TargetOs::Linux => {
            let mut prefixes: Vec<(PathBuf, FoundBy)> = Vec::new();
            if let Some(p) = &req.prefix {
                prefixes.push((p.clone(), FoundBy::PrefixOption));
            } else if let Some(p) = &env.wineprefix {
                prefixes.push((p.clone(), FoundBy::WinePrefixEnv));
            }
            let config = env
                .xdg_config_home
                .clone()
                .or_else(|| env.home.as_ref().map(|h| h.join(".config")));
            if let Some(cfg) = config {
                let lug = cfg.join("starcitizen-lug");
                if let Some(p) = read_conf_line(&lug.join("winedir.conf")) {
                    prefixes.push((p, FoundBy::LugConfig));
                }
                if let Some(g) = read_conf_line(&lug.join("gamedir.conf")) {
                    cands.push(Candidate {
                        root: g,
                        prefix: None,
                        found_by: FoundBy::LugConfig,
                    });
                }
            }
            if let Some(home) = &env.home {
                for d in ["Games", "games"] {
                    prefixes.push((home.join(d).join("star-citizen"), FoundBy::DefaultPrefix));
                }
            }
            // With an explicit --prefix only that prefix is searched.
            if req.prefix.is_some() {
                prefixes.truncate(1);
                cands.clear();
            }
            for (prefix, found_by) in prefixes {
                if !is_wine_prefix(&prefix) {
                    continue;
                }
                let prefix = std::fs::canonicalize(&prefix).unwrap_or(prefix);
                for log in prefix_launcher_logs(&prefix) {
                    for hit in launcher_log::read_tail(&log) {
                        if let Some(root) = map_windows_path(&prefix, &hit.library_root) {
                            cands.push(Candidate {
                                root,
                                prefix: Some(prefix.clone()),
                                found_by: found_by.clone(),
                            });
                        }
                    }
                }
                if let Ok(r) = resolve_ci(&prefix, PREFIX_LIBRARY) {
                    cands.push(Candidate {
                        root: r.path,
                        prefix: Some(prefix.clone()),
                        found_by,
                    });
                }
            }
        }
        TargetOs::Windows => {
            if let Some(appdata) = &env.appdata {
                for name in ["log.log", "log.old.log"] {
                    let log = appdata.join("rsilauncher").join("logs").join(name);
                    for hit in launcher_log::read_tail(&log) {
                        cands.push(Candidate {
                            root: PathBuf::from(&hit.library_root),
                            prefix: None,
                            found_by: FoundBy::LauncherLog,
                        });
                    }
                }
            }
            for root in windows_common_roots(&env.drives) {
                cands.push(Candidate {
                    root,
                    prefix: None,
                    found_by: FoundBy::DefaultPath,
                });
            }
        }
    }
    finish(cands)
}

fn finish(cands: Vec<Candidate>) -> Discovery {
    let mut installs: Vec<Install> = Vec::new();
    for c in cands {
        let Ok(root) = std::fs::canonicalize(&c.root) else {
            continue;
        };
        if installs.iter().any(|i| i.root == root) {
            continue;
        }
        let channels = list_channels(&root);
        if channels.is_empty() {
            continue;
        }
        installs.push(Install {
            root,
            prefix: c.prefix,
            found_by: c.found_by,
            channels,
        });
    }
    Discovery { installs }
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    #[test]
    fn prefix_from_drive_c_and_from_markers() {
        let t = tempfile::tempdir().unwrap();
        let pfx = t.path().join("pfx");
        let root = pfx.join("drive_c/Program Files/Roberts Space Industries/StarCitizen");
        std::fs::create_dir_all(&root).unwrap();
        // No system.reg: derived from the drive_c component.
        assert_eq!(enclosing_wine_prefix(&root), Some(pfx.clone()));
        std::fs::write(pfx.join("system.reg"), b"").unwrap();
        assert_eq!(enclosing_wine_prefix(&root.join("LIVE")), Some(pfx.clone()));
        assert_eq!(enclosing_wine_prefix(t.path()), None);
        let i = Install {
            root: root.clone(),
            prefix: None,
            found_by: FoundBy::GameDir,
            channels: vec![],
        };
        let roots = i.protected_roots();
        assert!(roots.contains(&root) && roots.contains(&pfx), "{roots:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MANIFEST: &str = r#"{"Data":{"Branch":"sc-alpha-4.10.0-hotfix","RequestedP4ChangeNum":"12572603","Version":"1.0.191.55227"}}"#;

    fn make_channel(root: &Path, name: &str) {
        let d = root.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("Data.p4k"), b"").unwrap();
        fs::write(d.join("build_manifest.id"), MANIFEST).unwrap();
    }

    fn make_prefix(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir.join("drive_c/users/Public")).unwrap();
        fs::write(dir.join("system.reg"), "").unwrap();
        let lib = dir.join("drive_c/Program Files/Roberts Space Industries/StarCitizen");
        fs::create_dir_all(&lib).unwrap();
        lib
    }

    fn linux_env(home: &Path) -> DiscoveryEnv {
        DiscoveryEnv {
            os: TargetOs::Linux,
            home: Some(home.to_path_buf()),
            xdg_config_home: None,
            wineprefix: None,
            appdata: None,
            drives: Vec::new(),
        }
    }

    #[test]
    fn channels_sorted_known_first_and_unknown_accepted() {
        let t = tempfile::tempdir().unwrap();
        for n in ["PTU", "ZZZ-TEST", "LIVE", "HOTFIX"] {
            make_channel(t.path(), n);
        }
        fs::create_dir_all(t.path().join("Files")).unwrap(); // no Data.p4k: not a channel
        let names: Vec<_> = list_channels(t.path())
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["LIVE", "HOTFIX", "PTU", "ZZZ-TEST"]);
    }

    #[test]
    fn channel_reads_build_with_game_version_from_branch() {
        let t = tempfile::tempdir().unwrap();
        make_channel(t.path(), "HOTFIX");
        let c = Channel::read(&t.path().join("HOTFIX")).unwrap();
        let b = c.build.unwrap();
        assert_eq!(b.game_version.unwrap().to_string(), "4.10.0");
        assert_eq!(b.change, Some(12572603));
    }

    #[test]
    fn channel_without_manifest_still_lists() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("LIVE")).unwrap();
        fs::write(t.path().join("LIVE/data.P4K"), b"").unwrap();
        let c = Channel::read(&t.path().join("LIVE")).unwrap();
        assert!(c.build.is_none());
        assert!(c.build_error.is_some());
        assert!(c.p4k.ends_with("data.P4K"));
    }

    #[test]
    fn default_prefixes_and_symlink_dedup() {
        let t = tempfile::tempdir().unwrap();
        let real = t.path().join("mnt/star-citizen");
        let lib = make_prefix(&real);
        make_channel(&lib, "LIVE");
        let home = t.path().join("home");
        fs::create_dir_all(home.join("games")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, home.join("games/star-citizen")).unwrap();
        let mut env = linux_env(&home);
        env.wineprefix = Some(real.clone());
        let d = discover(&DiscoveryRequest::default(), &env);
        assert_eq!(
            d.installs.len(),
            1,
            "same prefix through a symlink appears once"
        );
        let i = &d.installs[0];
        assert_eq!(i.found_by, FoundBy::WinePrefixEnv);
        assert_eq!(i.root, fs::canonicalize(&lib).unwrap());
        assert_eq!(i.channels[0].name, "LIVE");
    }

    #[test]
    fn launcher_log_in_prefix_finds_other_library() {
        let t = tempfile::tempdir().unwrap();
        let prefix = t.path().join("pfx");
        make_prefix(&prefix);
        let other = prefix.join("drive_c/Games/SC");
        make_channel(&other, "PTU");
        let logs = prefix.join("drive_c/users/me/AppData/Roaming/rsilauncher/logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(
            logs.join("log.log"),
            r#"{ "t":"x", "[main][info] ": "[Launcher::launch] Launching Star Citizen PTU from (C:\\games\\sc\\PTU)"  },"#,
        )
        .unwrap();
        let req = DiscoveryRequest {
            prefix: Some(prefix.clone()),
            game_dir: None,
        };
        let d = discover(&req, &linux_env(t.path()));
        assert_eq!(d.installs.len(), 1, "default library has no channels");
        assert_eq!(d.installs[0].found_by, FoundBy::PrefixOption);
        assert_eq!(d.installs[0].root, fs::canonicalize(&other).unwrap());
    }

    #[test]
    fn game_dir_skips_discovery() {
        let t = tempfile::tempdir().unwrap();
        make_channel(t.path(), "LIVE");
        let req = DiscoveryRequest {
            game_dir: Some(t.path().to_path_buf()),
            prefix: None,
        };
        let d = discover(&req, &linux_env(Path::new("/nonexistent")));
        assert_eq!(d.installs[0].found_by, FoundBy::GameDir);
        let empty = tempfile::tempdir().unwrap();
        let req = DiscoveryRequest {
            game_dir: Some(empty.path().to_path_buf()),
            prefix: None,
        };
        assert!(discover(&req, &linux_env(t.path())).installs.is_empty());
    }

    #[test]
    fn windows_common_paths_on_drives() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("D");
        make_channel(&d.join("Roberts Space Industries/StarCitizen"), "LIVE");
        make_channel(&d.join("MyGames/StarCitizen"), "PTU");
        let env = DiscoveryEnv {
            os: TargetOs::Windows,
            home: None,
            xdg_config_home: None,
            wineprefix: None,
            appdata: None,
            drives: vec![d.clone()],
        };
        let found = discover(&DiscoveryRequest::default(), &env);
        let roots: Vec<_> = found.installs.iter().map(|i| i.root.clone()).collect();
        assert_eq!(
            roots,
            vec![
                fs::canonicalize(d.join("Roberts Space Industries/StarCitizen")).unwrap(),
                fs::canonicalize(d.join("MyGames/StarCitizen")).unwrap(),
            ]
        );
    }

    #[test]
    #[cfg(unix)] // Wine prefixes only exist on Linux; on Windows "d:" in a path is a drive prefix
    fn windows_path_mapping() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path();
        fs::create_dir_all(p.join("drive_c/Program Files")).unwrap();
        assert_eq!(
            map_windows_path(p, r"C:\program files\RSI").unwrap(),
            p.join("drive_c/Program Files/RSI")
        );
        assert_eq!(
            map_windows_path(p, r"Z:\mnt\x").unwrap(),
            PathBuf::from("/mnt/x")
        );
        assert_eq!(
            map_windows_path(p, r"D:\Games").unwrap(),
            p.join("dosdevices/d:/Games")
        );
        assert_eq!(map_windows_path(p, "relative"), None);
    }

    #[test]
    fn lug_config_prefix() {
        let t = tempfile::tempdir().unwrap();
        let prefix = t.path().join("lugpfx");
        let lib = make_prefix(&prefix);
        make_channel(&lib, "LIVE");
        let cfg = t.path().join("cfg/starcitizen-lug");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(cfg.join("winedir.conf"), format!("{}\n", prefix.display())).unwrap();
        let mut env = linux_env(Path::new("/nonexistent"));
        env.xdg_config_home = Some(t.path().join("cfg"));
        let d = discover(&DiscoveryRequest::default(), &env);
        assert_eq!(d.installs[0].found_by, FoundBy::LugConfig);
    }
}
