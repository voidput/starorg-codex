//! App directories (spec 4.2), shared by the GUI and the CLI.
//!
//! | Purpose | Linux | Windows |
//! |---|---|---|
//! | Config | `$XDG_CONFIG_HOME/starorg-codex/` (`~/.config/...`) | `%APPDATA%\StarOrg Tools\Codex\config\` |
//! | Data | `$XDG_DATA_HOME/starorg-codex/` (`~/.local/share/...`) | `%APPDATA%\StarOrg Tools\Codex\data\` |
//! | Cache | `$XDG_CACHE_HOME/starorg-codex/` (`~/.cache/...`) | `%LOCALAPPDATA%\StarOrg Tools\Codex\cache\` |
//!
//! Resolved from the environment directly (the same XDG rules the `directories` crate
//! follows), never from Tauri's `AppHandle`. Nothing of ours is stored inside the game
//! directory, so the launcher's "Verify files" cannot remove it.
//!
//! Until 0.3.0 the folders were named `starorg-strings` (Linux) and `StarOrg Tools\Strings`
//! (Windows). [`crate::legacy`] moves them once; until it has, a folder that exists only under
//! the old name is used where it is, so nothing is ever looked for in an empty new folder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

/// Linux XDG folder name.
pub const NAME: &str = "starorg-codex";
/// Linux XDG folder name before 0.3.0 (migration only).
pub const LEGACY_NAME: &str = "starorg-strings";
/// Windows: `%APPDATA%\<VENDOR>\<WIN_NAME>`.
pub const VENDOR: &str = "StarOrg Tools";
pub const WIN_NAME: &str = "Codex";
/// Windows folder name before 0.3.0 (migration only).
pub const LEGACY_WIN_NAME: &str = "Strings";
/// The Tauri identifier: the webview keeps its storage in a folder of this name.
pub const WEBVIEW_ID: &str = "tools.starorg.codex";
/// The Tauri identifier before 0.3.0 (migration only).
pub const LEGACY_WEBVIEW_ID: &str = "tools.starorg.strings";

/// One app folder under its old and its new name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyPair {
    pub old: PathBuf,
    pub new: PathBuf,
}

impl LegacyPair {
    fn new(parent: &Path, old: &str, new: &str) -> Self {
        Self {
            old: parent.join(old),
            new: parent.join(new),
        }
    }

    /// The folder to use now: the new one, unless only the old one exists (not moved yet,
    /// or the move failed).
    pub fn current(&self) -> &Path {
        if !self.new.exists() && self.old.exists() {
            &self.old
        } else {
            &self.new
        }
    }
}

/// Where the app folders live, under their old and new names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub config: LegacyPair,
    pub data: LegacyPair,
    pub cache: LegacyPair,
    /// Every folder the migration moves, each once: the three above (on Windows the roaming
    /// and local roots that hold them) and the webview's storage.
    pub pairs: Vec<LegacyPair>,
    /// Serialises the migration between processes (outside every folder it moves).
    pub lock: PathBuf,
    windows: bool,
}

impl Layout {
    /// From the process environment. `None` when no home directory can be found.
    pub fn from_env() -> Option<Self> {
        Self::from_vars(|k| std::env::var_os(k), cfg!(windows))
    }

    /// From `var` (an environment lookup), for Linux or Windows rules.
    pub fn from_vars(var: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<Self> {
        let var = |k: &str| {
            var(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        if windows {
            let roaming = var("APPDATA")?;
            let local = var("LOCALAPPDATA").unwrap_or_else(|| roaming.clone());
            let roam = LegacyPair::new(&roaming.join(VENDOR), LEGACY_WIN_NAME, WIN_NAME);
            let loc = LegacyPair::new(&local.join(VENDOR), LEGACY_WIN_NAME, WIN_NAME);
            let mut pairs = vec![roam.clone()];
            if loc != roam {
                pairs.push(loc.clone());
            }
            pairs.push(LegacyPair::new(&local, LEGACY_WEBVIEW_ID, WEBVIEW_ID));
            Some(Self {
                config: roam.clone(),
                data: roam.clone(),
                cache: loc,
                pairs,
                lock: roaming.join(VENDOR).join(".codex-migration.lock"),
                windows,
            })
        } else {
            let home = var("HOME");
            let pick = |xdg: &str, default: &[&str]| -> Option<PathBuf> {
                var(xdg).or_else(|| {
                    let mut p = home.clone()?;
                    for d in default {
                        p.push(d);
                    }
                    Some(p)
                })
            };
            let config_home = pick("XDG_CONFIG_HOME", &[".config"])?;
            let data_home = pick("XDG_DATA_HOME", &[".local", "share"])?;
            let cache_home = pick("XDG_CACHE_HOME", &[".cache"])?;
            let config = LegacyPair::new(&config_home, LEGACY_NAME, NAME);
            let data = LegacyPair::new(&data_home, LEGACY_NAME, NAME);
            let cache = LegacyPair::new(&cache_home, LEGACY_NAME, NAME);
            let mut pairs = vec![config.clone(), data.clone(), cache.clone()];
            // WebKitGTK's storage (the UI's remembered layout and filters).
            pairs.push(LegacyPair::new(&data_home, LEGACY_WEBVIEW_ID, WEBVIEW_ID));
            let wv_cache = LegacyPair::new(&cache_home, LEGACY_WEBVIEW_ID, WEBVIEW_ID);
            if !pairs.contains(&wv_cache) {
                pairs.push(wv_cache);
            }
            pairs.dedup();
            Some(Self {
                config,
                data,
                cache,
                pairs,
                lock: data_home.join(".starorg-codex-migration.lock"),
                windows,
            })
        }
    }

    /// The folders to use now ([`LegacyPair::current`] for each).
    pub fn dirs(&self) -> AppDirs {
        if self.windows {
            AppDirs {
                config: self.config.current().join("config"),
                data: self.data.current().join("data"),
                cache: self.cache.current().join("cache"),
            }
        } else {
            AppDirs {
                config: self.config.current().to_path_buf(),
                data: self.data.current().to_path_buf(),
                cache: self.cache.current().to_path_buf(),
            }
        }
    }
}

impl AppDirs {
    /// From the process environment. `None` when no home directory can be found.
    ///
    /// The new folders, except one that exists only under its pre-0.3.0 name (see
    /// [`crate::legacy::migrate_from_env`], which every entry point runs first).
    pub fn from_env() -> Option<Self> {
        Layout::from_env().map(|l| l.dirs())
    }

    /// All three under one root (tests).
    pub fn under(root: &Path) -> Self {
        Self {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    pub fn config_toml(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn outputs_json(&self) -> PathBuf {
        self.data.join("outputs.json")
    }

    pub fn user_patches(&self) -> PathBuf {
        self.data.join("patches")
    }

    /// `fleet.json`: the imported fleet (addendum §6.4).
    pub fn fleet_json(&self) -> PathBuf {
        self.data.join("fleet.json")
    }

    /// `ship-names.toml`: shared order, groups, renames and prefix style.
    pub fn ship_names_toml(&self) -> PathBuf {
        self.data.join("ship-names.toml")
    }

    pub fn cfg_dir(&self) -> PathBuf {
        self.data.join("cfg")
    }

    pub fn locks(&self) -> PathBuf {
        self.data.join("locks")
    }

    /// `channels/<install-id>/<CHANNEL>/`
    pub fn channel_dir(&self, install_id: &str, channel: &str) -> PathBuf {
        self.data
            .join("channels")
            .join(install_id)
            .join(channel.to_ascii_uppercase())
    }

    /// `cache/<install-id>/<CHANNEL>/`
    pub fn channel_cache(&self, install_id: &str, channel: &str) -> PathBuf {
        self.cache
            .join(install_id)
            .join(channel.to_ascii_uppercase())
    }

    pub fn channel_lock(&self, install_id: &str, channel: &str) -> PathBuf {
        self.locks().join(format!(
            "{install_id}-{}.lock",
            channel.to_ascii_uppercase()
        ))
    }

    pub fn extract_lock(&self) -> PathBuf {
        self.locks().join("extract.lock")
    }

    pub fn outputs_lock(&self) -> PathBuf {
        self.locks().join("outputs.lock")
    }
}
