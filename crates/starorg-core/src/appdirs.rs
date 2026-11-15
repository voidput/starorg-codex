//! App directories (spec 4.2), shared by the GUI and the CLI.
//!
//! | Purpose | Linux | Windows |
//! |---|---|---|
//! | Config | `$XDG_CONFIG_HOME/starorg-strings/` (`~/.config/...`) | `%APPDATA%\StarOrg Tools\Strings\config\` |
//! | Data | `$XDG_DATA_HOME/starorg-strings/` (`~/.local/share/...`) | `%APPDATA%\StarOrg Tools\Strings\data\` |
//! | Cache | `$XDG_CACHE_HOME/starorg-strings/` (`~/.cache/...`) | `%LOCALAPPDATA%\StarOrg Tools\Strings\cache\` |
//!
//! Resolved from the environment directly (the same XDG rules the `directories` crate
//! follows), never from Tauri's `AppHandle`. Nothing of ours is stored inside the game
//! directory, so the launcher's "Verify files" cannot remove it.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

const NAME: &str = "starorg-strings";

impl AppDirs {
    /// From the process environment. `None` when no home directory can be found.
    pub fn from_env() -> Option<Self> {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        if cfg!(windows) {
            let roaming = var("APPDATA")?;
            let local = var("LOCALAPPDATA").unwrap_or_else(|| roaming.clone());
            let base = roaming.join("StarOrg Tools").join("Strings");
            Some(Self {
                config: base.join("config"),
                data: base.join("data"),
                cache: local.join("StarOrg Tools").join("Strings").join("cache"),
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
            Some(Self {
                config: pick("XDG_CONFIG_HOME", &[".config"])?.join(NAME),
                data: pick("XDG_DATA_HOME", &[".local", "share"])?.join(NAME),
                cache: pick("XDG_CACHE_HOME", &[".cache"])?.join(NAME),
            })
        }
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
