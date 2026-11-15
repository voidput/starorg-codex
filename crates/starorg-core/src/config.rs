//! `config.toml` (installs and their ids, default channel) and `outputs.json` (every output
//! we ever wrote), spec 4.2.
//!
//! **Install id.** A random id created when an install is first seen, stored with its last
//! known canonical path. It is not derived from the path, so moving or remounting a prefix
//! does not orphan the profile, overrides and backups.
//!
//! **Re-link.** When discovery finds an install at an unknown path and a stored install's path
//! no longer exists, the two are matched if a loose `global.ini` there is in `outputs.json`
//! (the CLI re-links by itself only on this match), or, weaker, when the channel names and
//! change numbers equal the stored ones (the GUI asks first; [`weak_relink_candidates`]).

use crate::appdirs::AppDirs;
use crate::discovery::Install;
use crate::fsutil::{self, read_json, write_json};
use crate::paths::find_existing_ci;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Newest entries kept in `outputs.json`.
pub const OUTPUTS_KEEP: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_channel: Option<String>,
    #[serde(default)]
    pub installs: Vec<InstallEntry>,
    /// StarOrg Tools environment and what to sync (`[sync]`, [`crate::starorg`]).
    #[serde(default)]
    pub sync: crate::starorg::SyncPrefs,
    /// Keys a newer app version wrote; kept on write.
    #[serde(flatten)]
    pub extra: toml::Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallEntry {
    pub id: String,
    /// Last known canonical library root.
    pub path: PathBuf,
    /// Channel name -> change number at last sight (for the weak re-link match).
    #[serde(default)]
    pub channels: BTreeMap<String, u64>,
}

impl Config {
    pub fn load(dirs: &AppDirs) -> std::io::Result<Self> {
        match fsutil::read_limited(&dirs.config_toml(), 1 << 20)? {
            None => Ok(Self::default()),
            Some(b) => {
                let s = String::from_utf8(b).map_err(std::io::Error::other)?;
                toml::from_str(&s).map_err(|e| {
                    std::io::Error::other(format!("{}: {e}", dirs.config_toml().display()))
                })
            }
        }
    }

    pub fn save(&self, dirs: &AppDirs) -> std::io::Result<()> {
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fsutil::write_atomic(&dirs.config_toml(), s.as_bytes())
    }

    pub fn find_by_path(&self, root: &Path) -> Option<&InstallEntry> {
        self.installs.iter().find(|i| i.path == root)
    }
}

fn channel_changes(install: &Install) -> BTreeMap<String, u64> {
    install
        .channels
        .iter()
        .filter_map(|c| Some((c.name.clone(), c.build.as_ref()?.change?)))
        .collect()
}

/// How an install id was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdOrigin {
    Known,
    /// Matched a stored install whose path is gone, by a loose file in `outputs.json`.
    Relinked {
        old_path: PathBuf,
    },
    New,
}

/// Look up the id of `install` without changing anything. Returns the id (or `None` for an
/// install never seen) and, when it would be re-linked, the stored entry it matches.
pub fn lookup_install(
    config: &Config,
    outputs: &Outputs,
    install: &Install,
) -> Option<(String, IdOrigin)> {
    if let Some(e) = config.find_by_path(&install.root) {
        return Some((e.id.clone(), IdOrigin::Known));
    }
    // Re-link on an outputs.json match only.
    let gone: Vec<&InstallEntry> = config
        .installs
        .iter()
        .filter(|e| !e.path.exists())
        .collect();
    if gone.is_empty() {
        return None;
    }
    for ch in &install.channels {
        let Ok(Some(ini)) = find_existing_ci(&ch.dir, LOOSE_INI) else {
            continue;
        };
        let Ok(sha) = fsutil::sha256_file(&ini) else {
            continue;
        };
        let ids: Vec<&str> = outputs
            .entries
            .iter()
            .filter(|o| o.sha256 == sha)
            .map(|o| o.install_id.as_str())
            .collect();
        if let Some(e) = gone.iter().find(|e| ids.contains(&e.id.as_str())) {
            return Some((
                e.id.clone(),
                IdOrigin::Relinked {
                    old_path: e.path.clone(),
                },
            ));
        }
    }
    None
}

/// Stored installs whose path is gone and whose channel names and change numbers equal the
/// ones found now. The GUI asks "Is this the same install, moved?"; the CLI never re-links on
/// this alone.
pub fn weak_relink_candidates<'a>(config: &'a Config, install: &Install) -> Vec<&'a InstallEntry> {
    let now = channel_changes(install);
    config
        .installs
        .iter()
        .filter(|e| !e.path.exists() && !e.channels.is_empty() && e.channels == now)
        .collect()
}

/// Find or create the id of `install` and save the config. Used by commands that write.
pub fn register_install(
    dirs: &AppDirs,
    config: &mut Config,
    outputs: &Outputs,
    install: &Install,
) -> std::io::Result<(String, IdOrigin)> {
    let (id, origin) = match lookup_install(config, outputs, install) {
        Some(found) => found,
        None => (fsutil::random_id(8), IdOrigin::New),
    };
    let changes = channel_changes(install);
    let before = config.clone();
    match config.installs.iter_mut().find(|e| e.id == id) {
        Some(e) => {
            e.path = install.root.clone();
            e.channels = changes;
        }
        None => config.installs.push(InstallEntry {
            id: id.clone(),
            path: install.root.clone(),
            channels: changes,
        }),
    }
    // Write only when something changed: `apply --if-stale` must stay a cheap no-op.
    if *config != before || !dirs.config_toml().exists() {
        config.save(dirs)?;
    }
    Ok((id, origin))
}

/// `Data/Localization/english/global.ini`, in the spelling the game uses on Windows.
pub const LOOSE_INI: &[&str] = &["Data", "Localization", "english", "global.ini"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outputs {
    /// Newest last.
    #[serde(default)]
    pub entries: Vec<OutputEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputEntry {
    pub sha256: String,
    pub install_id: String,
    pub channel: String,
    pub change: Option<u64>,
    pub time: String,
}

impl Outputs {
    pub fn load(dirs: &AppDirs) -> std::io::Result<Self> {
        Ok(read_json(&dirs.outputs_json())?.unwrap_or_default())
    }

    pub fn contains(&self, sha256: &str) -> bool {
        self.entries.iter().any(|e| e.sha256 == sha256)
    }

    /// Add an entry (moving an existing equal hash to the end) and keep the newest 200.
    pub fn add(&mut self, entry: OutputEntry) {
        self.entries.retain(|e| {
            !(e.sha256 == entry.sha256
                && e.install_id == entry.install_id
                && e.channel == entry.channel)
        });
        self.entries.push(entry);
        if self.entries.len() > OUTPUTS_KEEP {
            let drop = self.entries.len() - OUTPUTS_KEEP;
            self.entries.drain(..drop);
        }
    }

    pub fn save(&self, dirs: &AppDirs) -> std::io::Result<()> {
        write_json(&dirs.outputs_json(), self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_keeps_unknown_keys() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        std::fs::create_dir_all(&dirs.config).unwrap();
        std::fs::write(
            dirs.config_toml(),
            "future_switch = true\n[[installs]]\nid = \"abc\"\npath = \"/x\"\n",
        )
        .unwrap();
        let c = Config::load(&dirs).unwrap();
        assert_eq!(c.installs[0].id, "abc");
        c.save(&dirs).unwrap();
        let again = std::fs::read_to_string(dirs.config_toml()).unwrap();
        assert!(again.contains("future_switch = true"), "{again}");
    }

    #[test]
    fn outputs_keep_newest_200() {
        let mut o = Outputs::default();
        for i in 0..250 {
            o.add(OutputEntry {
                sha256: format!("{i}"),
                install_id: "a".into(),
                channel: "LIVE".into(),
                change: None,
                time: String::new(),
            });
        }
        assert_eq!(o.entries.len(), 200);
        assert!(o.contains("249") && !o.contains("49"));
    }
}
