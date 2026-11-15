//! StarOrg Tools: which environment to talk to and what to sync (addendum §5).
//!
//! Plain data, no HTTP: the client lives in `starorg-net` and is linked by the app only. The
//! preferences are stored under `[sync]` in `config.toml` so the app, the CLI and the launch
//! hooks read the same switches:
//!
//! - `import_game_log` (default on): the Game.log reader. Off, nothing reads the channel's
//!   logs for blueprints or characters ([`crate::blueprints::logscan::scan_if`]).
//! - `push_blueprints`, `pull_blueprints`, `pull_fleet`, `auto_sync` (default off): what the
//!   app may send to or read from StarOrg Tools, and whether it does so by itself after the
//!   game exits. The CLI has no HTTP client; its post-exit hook only leaves a
//!   [`PENDING_FILE`] note for the app when `auto_sync` is on.
//!
//! `STARORG_ENV` (`production` / `dev`) is what a bare `starorg-codex sync set` saves; it
//! never overrides the saved environment on its own.

use crate::appdirs::AppDirs;
use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The environment variable a bare `sync set` saves.
pub const ENV_VAR: &str = "STARORG_ENV";
/// Written by the post-exit hook (one channel name per line) when `auto_sync` is on; the app
/// syncs those channels and deletes it.
pub const PENDING_FILE: &str = "starorg-sync-pending.txt";

/// The two StarOrg Tools deployments (SC-ORG-BOT `docs/internal/reference/api.md`, "StarOrg
/// Strings sign-in": base URLs `https://starorg.tools` and `https://dev.starorg.tools`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Environment {
    #[default]
    Production,
    Dev,
}

impl Environment {
    pub const ALL: [Self; 2] = [Self::Production, Self::Dev];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Dev => "dev",
        }
    }

    /// `production` / `prod` / `dev` (any case).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "production" | "prod" => Some(Self::Production),
            "dev" | "development" => Some(Self::Dev),
            _ => None,
        }
    }

    pub fn host(self) -> &'static str {
        match self {
            Self::Production => "starorg.tools",
            Self::Dev => "dev.starorg.tools",
        }
    }

    pub fn base_url(self) -> String {
        format!("https://{}", self.host())
    }
}

/// `[sync]` in `config.toml`. Missing keys take the defaults (a config written before this
/// existed = Game.log import on, everything that talks to StarOrg off).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncPrefs {
    pub environment: Environment,
    pub import_game_log: bool,
    pub push_blueprints: bool,
    pub pull_blueprints: bool,
    pub pull_fleet: bool,
    pub auto_sync: bool,
}

impl Default for SyncPrefs {
    fn default() -> Self {
        Self {
            environment: Environment::Production,
            import_game_log: true,
            push_blueprints: false,
            pull_blueprints: false,
            pull_fleet: false,
            auto_sync: false,
        }
    }
}

impl SyncPrefs {
    /// From `config.toml`; defaults when it is missing or unreadable (the reader must never
    /// stop a launch hook).
    pub fn load(dirs: &AppDirs) -> Self {
        crate::config::Config::load(dirs)
            .map(|c| c.sync)
            .unwrap_or_default()
    }

    /// Anything that sends to or reads from StarOrg Tools on its own.
    pub fn any_network(&self) -> bool {
        self.push_blueprints || self.pull_blueprints || self.pull_fleet
    }
}

/// The post-exit hook's note to the app: these channels want a sync.
pub fn mark_pending(dirs: &AppDirs, channels: &[String]) -> std::io::Result<()> {
    let mut all = read_pending(dirs);
    all.extend(channels.iter().cloned());
    let text: String = all.iter().map(|c| format!("{c}\n")).collect();
    fsutil::write_atomic(&dirs.data.join(PENDING_FILE), text.as_bytes())
}

pub fn read_pending(dirs: &AppDirs) -> BTreeSet<String> {
    std::fs::read_to_string(dirs.data.join(PENDING_FILE))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && l.len() <= 32
                && l.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .map(str::to_string)
        .collect()
}

/// Read and delete the note.
pub fn take_pending(dirs: &AppDirs) -> BTreeSet<String> {
    let p = read_pending(dirs);
    let _ = std::fs::remove_file(dirs.data.join(PENDING_FILE));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_the_two_deployments() {
        assert_eq!(Environment::Production.base_url(), "https://starorg.tools");
        assert_eq!(Environment::Dev.base_url(), "https://dev.starorg.tools");
        assert_eq!(Environment::parse("PROD"), Some(Environment::Production));
        assert_eq!(Environment::parse("dev"), Some(Environment::Dev));
        assert_eq!(Environment::parse("staging"), None);
        assert_eq!(Environment::default(), Environment::Production);
    }

    #[test]
    fn defaults_keep_the_log_reader_and_send_nothing() {
        let p = SyncPrefs::default();
        assert!(p.import_game_log);
        assert!(!p.any_network() && !p.auto_sync);
        // A partial [sync] table keeps the other defaults.
        let p: SyncPrefs = toml::from_str("environment = \"dev\"\npull_fleet = true\n").unwrap();
        assert_eq!(p.environment, Environment::Dev);
        assert!(p.import_game_log && p.pull_fleet && !p.push_blueprints);
    }

    #[test]
    fn prefs_round_trip_through_config_toml() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        assert_eq!(SyncPrefs::load(&dirs), SyncPrefs::default());
        let mut c = crate::config::Config::load(&dirs).unwrap();
        c.sync.import_game_log = false;
        c.sync.environment = Environment::Dev;
        c.save(&dirs).unwrap();
        let text = std::fs::read_to_string(dirs.config_toml()).unwrap();
        assert!(text.contains("[sync]"), "{text}");
        let p = SyncPrefs::load(&dirs);
        assert!(!p.import_game_log);
        assert_eq!(p.environment, Environment::Dev);
    }

    #[test]
    fn pending_note_collects_and_is_taken_once() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        mark_pending(&dirs, &["LIVE".into()]).unwrap();
        mark_pending(&dirs, &["PTU".into(), "LIVE".into()]).unwrap();
        let p = take_pending(&dirs);
        assert_eq!(p.into_iter().collect::<Vec<_>>(), ["LIVE", "PTU"]);
        assert!(take_pending(&dirs).is_empty());
    }
}
