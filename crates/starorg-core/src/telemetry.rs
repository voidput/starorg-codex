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
//!   data folder ([`crate::storage::telemetry_recordings_dir`]); so are `telemetry_gamelog`
//!   (read the channel's `Game.log` for events) and `telemetry_people` (list the handles named
//!   there; needs `telemetry_gamelog`).
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
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

const TABLE: &str = "experimental";
const KEY_ENABLED: &str = "telemetry";
const KEY_PORT: &str = "telemetry_port";
const KEY_TRIGGERS: &str = "telemetry_triggers";
const KEY_RECORD: &str = "telemetry_record";
const KEY_GAMELOG: &str = "telemetry_gamelog";
const KEY_PEOPLE: &str = "telemetry_people";

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
    /// `telemetry_gamelog`: read the channel's `Game.log` for the Events panel (session,
    /// location, quantum travel, deaths and respawns). Counts only with `enabled` on.
    pub gamelog: bool,
    /// `telemetry_people`: keep the handles Game.log names for the People panel (memory
    /// only). Counts only with `gamelog` active.
    pub people: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            triggers: false,
            record: false,
            gamelog: false,
            people: false,
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

    /// Game.log events count only while telemetry itself is on.
    pub fn gamelog_active(&self) -> bool {
        self.enabled && self.gamelog
    }

    /// People count only while Game.log events are on.
    pub fn people_active(&self) -> bool {
        self.gamelog_active() && self.people
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
        gamelog: flag(KEY_GAMELOG).unwrap_or(d.gamelog),
        people: flag(KEY_PEOPLE).unwrap_or(d.people),
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
    t.insert(KEY_GAMELOG.into(), toml::Value::Boolean(s.gamelog));
    t.insert(KEY_PEOPLE.into(), toml::Value::Boolean(s.people));
    Ok(())
}

const KEY_NOTIFICATIONS: &str = "telemetry_notifications";
const KEY_SPEECH: &str = "telemetry_speech";
const KEY_ALERTS: &str = "telemetry_alerts";

/// Rule ids of the alert overrides (`starorg_telemetry::alerts`).
pub fn valid_rule_id(id: &str) -> bool {
    (1..=48).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// How alerts reach the user: their own choice, separate from Metrics and actions, so turning
/// Metrics on never turns on toasts or sounds. `telemetry_notifications` in `[experimental]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Notifications {
    /// No alert is evaluated (the default).
    #[default]
    Off,
    /// Toasts and the screen's Alerts panel.
    InApp,
    /// Also a short tone generated in the app (and speech when that is on too).
    InAppSound,
}

impl Notifications {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::InApp => "in_app",
            Self::InAppSound => "in_app_sound",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [Self::Off, Self::InApp, Self::InAppSound]
            .into_iter()
            .find(|n| n.as_str() == s)
    }
}

/// The user's change to one default alert rule; `None` keeps the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct RuleOverride {
    pub enabled: Option<bool>,
    /// Least seconds between two alerts of the rule.
    pub cooldown_s: Option<f64>,
    /// The threshold of a threshold rule.
    pub value: Option<f64>,
    /// Where a threshold rule arms again (hysteresis).
    pub rearm: Option<f64>,
}

impl RuleOverride {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Alert settings in `[experimental]`: `telemetry_notifications`, `telemetry_speech` and the
/// per-rule overrides in `[experimental.telemetry_alerts.<rule id>]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlertSettings {
    pub notifications: Notifications,
    /// Speak the callout (Web Speech, the computer's own voices only). Off by default; counts
    /// only with [`Notifications::InAppSound`].
    pub speech: bool,
    /// By rule id; empty overrides are not kept.
    pub overrides: std::collections::BTreeMap<String, RuleOverride>,
}

/// Read the alert settings; a value of the wrong type is the default, an override entry that
/// is not a table or has a bad rule id is skipped.
pub fn alert_settings(cfg: &Config) -> AlertSettings {
    let mut a = AlertSettings::default();
    let Some(t) = cfg.extra.get(TABLE).and_then(|t| t.as_table()) else {
        return a;
    };
    if let Some(n) = t
        .get(KEY_NOTIFICATIONS)
        .and_then(|v| v.as_str())
        .and_then(Notifications::parse)
    {
        a.notifications = n;
    }
    a.speech = t.get(KEY_SPEECH).and_then(|v| v.as_bool()).unwrap_or(false);
    let Some(rules) = t.get(KEY_ALERTS).and_then(|v| v.as_table()) else {
        return a;
    };
    for (id, v) in rules {
        let Some(r) = v.as_table().filter(|_| valid_rule_id(id)) else {
            continue;
        };
        let num = |k: &str| {
            r.get(k)
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                .filter(|x| x.is_finite())
        };
        let o = RuleOverride {
            enabled: r.get("enabled").and_then(|v| v.as_bool()),
            cooldown_s: num("cooldown_s"),
            value: num("value"),
            rearm: num("rearm"),
        };
        if !o.is_empty() {
            a.overrides.insert(id.clone(), o);
        }
    }
    a
}

/// Write `a` into `[experimental]`, keeping the table's other keys (the overrides table is
/// replaced as a whole). The caller saves the config. Values are not checked against the
/// rules here (`starorg_telemetry::alerts::apply_overrides` does that).
pub fn set_alert_settings(cfg: &mut Config, a: &AlertSettings) {
    let entry = cfg
        .extra
        .entry(TABLE)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::Table::new());
    }
    let t = entry.as_table_mut().expect("made a table above");
    t.insert(
        KEY_NOTIFICATIONS.into(),
        toml::Value::String(a.notifications.as_str().into()),
    );
    t.insert(KEY_SPEECH.into(), toml::Value::Boolean(a.speech));
    let mut rules = toml::Table::new();
    for (id, o) in a.overrides.iter().filter(|(_, o)| !o.is_empty()) {
        let mut r = toml::Table::new();
        if let Some(b) = o.enabled {
            r.insert("enabled".into(), toml::Value::Boolean(b));
        }
        for (k, v) in [
            ("cooldown_s", o.cooldown_s),
            ("value", o.value),
            ("rearm", o.rearm),
        ] {
            if let Some(x) = v.filter(|x| x.is_finite()) {
                r.insert(k.into(), toml::Value::Float(x));
            }
        }
        rules.insert(id.clone(), toml::Value::Table(r));
    }
    if rules.is_empty() {
        t.remove(KEY_ALERTS);
    } else {
        t.insert(KEY_ALERTS.into(), toml::Value::Table(rules));
    }
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
