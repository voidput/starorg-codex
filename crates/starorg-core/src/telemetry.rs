//! Live telemetry (experimental): the opt-in gate, settings and setup help.
//!
//! Star Citizen 5.0 can host a local gRPC service, `sc.game.telemetry.v1.GameTelemetryService`,
//! on `127.0.0.1:<port>` (plaintext, no authentication). The app may read its stream and,
//! only when the user turned triggers on as well, press allowlisted game inputs
//! (`TriggerGameAction`). This module holds what decides whether that is offered and how the
//! game is switched on; the client lives elsewhere.
//!
//! - **Off by default.** `[experimental] telemetry = true` in `config.toml` turns it on;
//!   `telemetry_triggers` is a second, separate opt-in ([`Settings`]), and
//!   `telemetry_record` ("Record sessions") a third: recordings of the stream in the app's
//!   data folder ([`crate::storage::telemetry_recordings_dir`]).
//! - **5.0 or newer only** ([`eligible`]): PTU 5.0 today, LIVE by itself once LIVE reports 5.0.
//!   An unknown game version is not offered.
//! - **Loopback only** ([`endpoint`]; the client refuses any other host): nothing leaves the
//!   computer.
//! - **The schema is the user's own.** It is read at run time from the channel's
//!   `Bin64/StarCitizen.exe` ([`game_exe`]); no CIG file is shipped.
//! - The game needs `server.game.telemetry.enabled = 1` in `USER.cfg`
//!   ([`crate::usercfg::TELEMETRY`], written with the ledger rules of the language line by
//!   [`configure_user_cfg`]) and two variables in the RSI Launcher's environment, which we
//!   never set: [`env_help`] says how.

use crate::appdirs::AppDirs;
use crate::build::GameVersion;
use crate::config::Config;
use crate::discovery::Channel;
use crate::paths::{ResolveError, find_existing_ci};
use crate::usercfg::{self, AdoptOutcome, CfgError, TELEMETRY, TELEMETRY_TRIGGERS, UndoOutcome};
use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

const TABLE: &str = "experimental";
const KEY_ENABLED: &str = "telemetry";
const KEY_PORT: &str = "telemetry_port";
const KEY_TRIGGERS: &str = "telemetry_triggers";
const KEY_RECORD: &str = "telemetry_record";

/// The port the game is told to use when the user sets none (`GRPC_CLIENT_SERVER_PORT`).
pub const DEFAULT_PORT: u16 = 50051;
/// The game accepts ports 1024 to 65535.
pub const MIN_PORT: u16 = 1024;
/// The first game version with the service.
pub const MIN_VERSION: GameVersion = GameVersion {
    major: 5,
    minor: 0,
    patch: 0,
};

/// `[experimental]` telemetry settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// `telemetry`: read the live stream.
    pub enabled: bool,
    /// `telemetry_port`: [`DEFAULT_PORT`] when absent or outside 1024 to 65535.
    pub port: u16,
    /// `telemetry_triggers`: allow pressing allowlisted game inputs. Has no effect while
    /// `enabled` is off.
    pub triggers: bool,
    /// `telemetry_record`: "Record sessions", write the live stream to a recording. Has no
    /// effect while `enabled` is off; independent of `triggers`.
    pub record: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            triggers: false,
            record: false,
        }
    }
}

impl Settings {
    /// Triggers count only while telemetry itself is on.
    pub fn triggers_active(&self) -> bool {
        self.enabled && self.triggers
    }

    /// Recording counts only while telemetry itself is on.
    pub fn record_active(&self) -> bool {
        self.enabled && self.record
    }
}

/// Read `[experimental]`; anything missing or of the wrong type is the default.
pub fn settings(cfg: &Config) -> Settings {
    let d = Settings::default();
    let Some(t) = cfg.extra.get(TABLE).and_then(|t| t.as_table()) else {
        return d;
    };
    let flag = |k: &str| t.get(k).and_then(|v| v.as_bool());
    Settings {
        enabled: flag(KEY_ENABLED).unwrap_or(d.enabled),
        port: t
            .get(KEY_PORT)
            .and_then(|v| v.as_integer())
            .and_then(|p| u16::try_from(p).ok())
            .filter(|&p| p >= MIN_PORT)
            .unwrap_or(d.port),
        triggers: flag(KEY_TRIGGERS).unwrap_or(d.triggers),
        record: flag(KEY_RECORD).unwrap_or(d.record),
    }
}

/// Write `s` into `[experimental]`, keeping the table's other keys. The caller saves the
/// config. A port outside 1024 to 65535 is refused.
pub fn set_settings(cfg: &mut Config, s: &Settings) -> Result<(), String> {
    if s.port < MIN_PORT {
        return Err(format!(
            "port {} is below {MIN_PORT}; the game accepts 1024 to 65535",
            s.port
        ));
    }
    let entry = cfg
        .extra
        .entry(TABLE)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::Table::new());
    }
    let t = entry.as_table_mut().expect("made a table above");
    t.insert(KEY_ENABLED.into(), toml::Value::Boolean(s.enabled));
    t.insert(KEY_PORT.into(), toml::Value::Integer(i64::from(s.port)));
    t.insert(KEY_TRIGGERS.into(), toml::Value::Boolean(s.triggers));
    t.insert(KEY_RECORD.into(), toml::Value::Boolean(s.record));
    Ok(())
}

/// Why a channel is not offered live telemetry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum NotEligible {
    /// No build manifest, or a branch without a version number.
    #[error("the game version of this channel is unknown; live telemetry needs 5.0 or newer")]
    UnknownVersion,
    #[error("this channel runs {version}; live telemetry needs 5.0 or newer")]
    TooOld { version: GameVersion },
}

/// Whether `channel` can use live telemetry: its build reports game version 5.0 or newer.
pub fn eligible(channel: &Channel) -> Result<(), NotEligible> {
    eligible_version(channel.build.as_ref().and_then(|b| b.game_version))
}

/// [`eligible`] for a game version alone (`None` = unknown), for callers that hold a build's
/// version but no discovered [`Channel`] (the app's mock backend).
pub fn eligible_version(version: Option<GameVersion>) -> Result<(), NotEligible> {
    match version {
        None => Err(NotEligible::UnknownVersion),
        Some(v) if v < MIN_VERSION => Err(NotEligible::TooOld { version: v }),
        Some(_) => Ok(()),
    }
}

/// `Bin64/StarCitizen.exe` of the channel, in its on-disk spelling (`None` when absent). The
/// schema is read from this file at run time.
pub fn game_exe(channel: &Channel) -> Result<Option<PathBuf>, ResolveError> {
    find_existing_ci(&channel.dir, &["Bin64", "StarCitizen.exe"])
}

/// Where the game's service listens: always `127.0.0.1`.
pub fn endpoint(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

/// What [`configure_user_cfg`] did to each line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CfgChange {
    /// `server.game.telemetry.enabled = 1`: adopted when on, released when off.
    pub telemetry: LineChange,
    /// `server.game.telemetry.trigger.actions.enabled = 1`: adopted only with triggers on.
    pub triggers: LineChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "change", rename_all = "kebab-case")]
pub enum LineChange {
    Adopted { outcome: AdoptOutcome },
    Released { outcome: UndoOutcome },
}

/// Bring the channel's `USER.cfg` in line with `s`: with telemetry on, rely on (and write
/// when missing) the telemetry line, plus the triggers line only when triggers are on too;
/// otherwise let go of the lines, which reverts each one when no other channel relies on it
/// and it is still ours. `replace_other` lets an explicit other value (for example `= 0`) be
/// replaced; the previous line comes back on undo. `channel_ref` is `<install-id>/<CHANNEL>`.
pub fn configure_user_cfg(
    dirs: &AppDirs,
    channel_dir: &Path,
    channel_ref: &str,
    s: &Settings,
    replace_other: bool,
) -> Result<CfgChange, CfgError> {
    if !s.enabled {
        // Off: the triggers line first, the reverse of the order the lines were adopted in.
        let triggers = usercfg::drop_line(dirs, channel_dir, channel_ref, &TELEMETRY_TRIGGERS)?;
        let telemetry = usercfg::drop_line(dirs, channel_dir, channel_ref, &TELEMETRY)?;
        return Ok(CfgChange {
            telemetry: LineChange::Released { outcome: telemetry },
            triggers: LineChange::Released { outcome: triggers },
        });
    }
    let telemetry = LineChange::Adopted {
        outcome: usercfg::adopt(dirs, channel_dir, channel_ref, &TELEMETRY, replace_other)?,
    };
    // Triggers only on top of a telemetry line that is really there.
    let telemetry_set = matches!(
        telemetry,
        LineChange::Adopted {
            outcome: AdoptOutcome::AlreadySet | AdoptOutcome::Written
        }
    );
    let triggers = if s.triggers_active() && telemetry_set {
        LineChange::Adopted {
            outcome: usercfg::adopt(
                dirs,
                channel_dir,
                channel_ref,
                &TELEMETRY_TRIGGERS,
                replace_other,
            )?,
        }
    } else {
        LineChange::Released {
            outcome: usercfg::drop_line(dirs, channel_dir, channel_ref, &TELEMETRY_TRIGGERS)?,
        }
    };
    Ok(CfgChange {
        telemetry,
        triggers,
    })
}

/// The operating system the help is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HelpOs {
    Windows,
    /// Wine, LUG Helper, Lutris or Steam Proton.
    Linux,
}

impl HelpOs {
    /// The system this build runs on (Linux help for every non-Windows system).
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// How to give the RSI Launcher the two variables the game needs (`GRPC_CLIENT_SERVER_ENABLED`
/// and `GRPC_CLIENT_SERVER_PORT`). The game inherits them from the launcher, so a launcher
/// already running must be quit fully. We never change the user's environment or registry.
pub fn env_help(os: HelpOs, port: u16) -> String {
    let warn = "Never set GRPC_CLIENT_SERVER_EXTERNAL: it would open the game's service, \
                which has no password, to your whole network.";
    match os {
        HelpOs::Windows => format!(
            "The game reads two environment variables from the RSI Launcher that starts it.\n\
             \n\
             1. Open PowerShell or Command Prompt (no administrator needed) and run:\n\
             \n\
             \x20   setx GRPC_CLIENT_SERVER_ENABLED 1\n\
             \x20   setx GRPC_CLIENT_SERVER_PORT {port}\n\
             \n\
             \x20  Each line prints \"SUCCESS: Specified value was saved.\"\n\
             \n\
             2. Quit the RSI Launcher fully: close its window, then right-click its icon in the \
             system tray (near the clock) and choose Quit. Start it again. If in doubt, sign out \
             of Windows and back in.\n\
             \n\
             To check, in a new PowerShell window:\n\
             \n\
             \x20   [Environment]::GetEnvironmentVariable(\"GRPC_CLIENT_SERVER_PORT\", \"User\")\n\
             \n\
             It prints {port}.\n\
             \n\
             To undo: reg delete HKCU\\Environment /F /V GRPC_CLIENT_SERVER_ENABLED and \
             reg delete HKCU\\Environment /F /V GRPC_CLIENT_SERVER_PORT, then restart the \
             launcher the same way.\n\
             \n\
             {warn}\n"
        ),
        HelpOs::Linux => format!(
            "The game reads two environment variables from the RSI Launcher that starts it. \
             Wine passes the Linux environment through, so add them to whatever starts the \
             launcher:\n\
             \n\
             - LUG Helper or a plain Wine launch script: near the top, before the line that \
             runs wine:\n\
             \n\
             \x20   export GRPC_CLIENT_SERVER_ENABLED=1\n\
             \x20   export GRPC_CLIENT_SERVER_PORT={port}\n\
             \n\
             - Lutris: right-click Star Citizen > Configure > System options > Environment \
             variables, add GRPC_CLIENT_SERVER_ENABLED = 1 and GRPC_CLIENT_SERVER_PORT = {port}, \
             Save.\n\
             \n\
             - Steam (Proton): Properties > Launch options:\n\
             \n\
             \x20   GRPC_CLIENT_SERVER_ENABLED=1 GRPC_CLIENT_SERVER_PORT={port} %command%\n\
             \n\
             - Anything else: put the two variables in front of the command that starts the \
             launcher.\n\
             \n\
             Then quit the launcher completely and start it again the way you just changed. \
             To undo, remove what you added.\n\
             \n\
             {warn}\n"
        ),
    }
}

#[cfg(test)]
mod tests;
