//! Session context from Game.log's header and connection lines (sc-telemetry `gamelog.py`
//! `SessionState`): build, channel, shard and region, star system, login queue, backend hub,
//! disconnects. The server address and port, `@session` and `@env_session` ids are never
//! read into this state at all (sc-telemetry kept them behind a guarded API; we do not need
//! them).

use super::{Ev, EventKind, between};
use serde::Serialize;

/// Regions in shard names (`ptu_use1b_...`).
const REGIONS: &[(&str, &str)] = &[
    ("apse2", "Asia-Pacific SE (Sydney)"),
    ("apne1", "Asia-Pacific NE (Tokyo)"),
    ("use1", "US East"),
    ("usw2", "US West"),
    ("euw1", "EU West"),
    ("euc1", "EU Central"),
    ("ape1", "Asia-Pacific East"),
];

/// Disconnect reasons that are the player's own doing (quit, exit to menu); anything else
/// is a drop.
const EXPECTED_DISCONNECT: &[&str] = &[
    "player requested disconnect",
    "disconnect light",
    "disconnectlight",
    "nub destroyed",
    "before game shutdown",
];

/// One disconnect can log a line per channel (Replicant + GameClient); the first wins.
const DISCONNECT_DEDUP_MS: i64 = 2000;

/// The session as the panel shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SessionView {
    /// `LIVE`, `PTU`... from the `Executable:` header line.
    pub channel: Option<String>,
    pub build: Option<String>,
    pub shard: Option<String>,
    pub region_code: Option<String>,
    pub region_name: Option<String>,
    /// `USE1B · 020`.
    pub shard_short: Option<String>,
    /// Star system, or `Main menu`.
    pub system: Option<String>,
    pub joined_ms: Option<i64>,
    pub log_started: Option<String>,
    pub queue_position: Option<u32>,
    pub queue_eta_s: Option<i64>,
    /// `up` or `down` (backend hub link), `None` before the first line.
    pub hub: Option<String>,
    pub disconnects: u32,
    pub last_disconnect_reason: Option<String>,
    pub last_disconnect_expected: Option<bool>,
}

#[derive(Debug, Default)]
pub struct SessionState {
    v: SessionView,
    last_dc: Option<i64>,
    /// `uptime_secs` of the last disconnect counted (for the join history; the raw line is
    /// never kept).
    pub(crate) last_dc_uptime_s: Option<f64>,
}

/// `.../StarCitizen/PTU/Bin64/StarCitizen.exe` -> `PTU` (either slash).
pub fn channel_from_exe(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    let i = parts.iter().position(|p| p.eq_ignore_ascii_case("bin64"))?;
    (i > 0).then(|| parts[i - 1].to_ascii_uppercase())
}

/// `ptu_use1b_13248297_020` -> (region code, region name, `USE1B · 020`).
pub fn parse_shard(shard: &str) -> (Option<String>, Option<String>, Option<String>) {
    let parts: Vec<&str> = shard.split('_').collect();
    if parts.len() < 2 {
        return (None, None, Some(shard.to_ascii_uppercase()));
    }
    let code = parts[1].to_ascii_lowercase();
    let name = REGIONS
        .iter()
        .find(|(pre, _)| code.starts_with(pre))
        .map(|(_, n)| (*n).to_owned())
        .unwrap_or_else(|| code.to_ascii_uppercase());
    let short = if parts.len() >= 3 {
        format!("{} · {}", code.to_ascii_uppercase(), parts[parts.len() - 1])
    } else {
        code.to_ascii_uppercase()
    };
    (Some(code), Some(name), Some(short))
}

fn system_name(raw: &str) -> String {
    if raw.to_ascii_lowercase().starts_with("frontend") {
        "Main menu".into()
    } else {
        raw.into()
    }
}

/// 1234.5 -> `20m 34s`; 7300 -> `2h 01m`.
pub fn fmt_secs(s: f64) -> Option<String> {
    if !s.is_finite() || s < 0.0 {
        return None;
    }
    let s = s as u64;
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    Some(if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {sec:02}s")
    } else {
        format!("{sec}s")
    })
}

impl SessionState {
    pub fn view(&self) -> SessionView {
        self.v.clone()
    }

    fn set_shard(&mut self, shard: Option<&str>) {
        self.v.shard = shard.map(str::to_owned);
        let (code, name, short) = shard.map(parse_shard).unwrap_or_default();
        self.v.region_code = code;
        self.v.region_name = name;
        self.v.shard_short = short;
    }

    fn shard_label(&self) -> String {
        let s = self.v.shard.as_deref().unwrap_or("?");
        match &self.v.region_name {
            Some(r) => format!("{s} ({r})"),
            None => s.to_owned(),
        }
    }

    /// A session line -> its events (`Some`, maybe empty), or `None` when the line is not
    /// one of ours. Never carries an address, a port or a session id.
    pub fn apply(&mut self, body: &str, t: Option<i64>) -> Option<Vec<Ev>> {
        let mut out = Vec::new();
        if let Some(when) = body.strip_prefix("Log started on ") {
            *self = Self::default();
            self.v.log_started = Some(when.trim().to_owned());
            out.push((EventKind::LogStarted, t, "Game started".into()));
            return Some(out);
        }
        if let Some(v) = body.strip_prefix("FileVersion:") {
            self.v.build = v.split_whitespace().next().map(str::to_owned);
            return Some(out);
        }
        if let Some(p) = body.strip_prefix("Executable:") {
            if let Some(ch) = channel_from_exe(p.trim()) {
                self.v.channel = Some(ch);
            }
            return Some(out);
        }
        if body.contains("<Join PU> ") {
            let shard = between(body, "shard[", "]").filter(|s| !s.is_empty());
            self.set_shard(shard);
            self.v.joined_ms = t;
            self.v.queue_position = None;
            self.v.queue_eta_s = None;
            out.push((EventKind::SessionJoin, t, self.shard_label()));
            return Some(out);
        }
        if let Some(rest) = body
            .find("<Update Shard Id> New Shard Id: ")
            .map(|i| &body[i + "<Update Shard Id> New Shard Id: ".len()..])
        {
            let id = rest.split_whitespace().next().unwrap_or("");
            let id = id.trim_end_matches('.');
            if id.is_empty()
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return None;
            }
            if self.v.shard.as_deref() != Some(id) {
                let had = self.v.shard.is_some();
                self.set_shard(Some(id));
                if had {
                    out.push((EventKind::SessionShardChange, t, self.shard_label()));
                }
            }
            return Some(out);
        }
        if let Some(rest) = body.strip_prefix("Loading screen for ")
            && rest.contains(" closed after")
            && let Some((sys, _)) = rest.split_once(" : ")
        {
            let sys = system_name(sys.trim());
            if self.v.system.as_deref() != Some(sys.as_str()) {
                if sys == "Main menu" && (self.v.joined_ms.is_some() || self.v.shard.is_some()) {
                    // Back at the menu: the shard session is over.
                    let left = self.shard_label();
                    self.set_shard(None);
                    self.v.joined_ms = None;
                    out.push((EventKind::SessionLeave, t, left));
                }
                self.v.system = Some(sys.clone());
                out.push((EventKind::SessionSystem, t, sys));
            }
            return Some(out);
        }
        if body.contains("<AccountLoginQueueStatus> ") && body.contains("queue name ") {
            let pos = between(body, " - position ", " ").and_then(|p| p.parse::<u32>().ok());
            let eta = body
                .split_once(" - eta (seconds) ")
                .and_then(|(_, e)| e.split_whitespace().next())
                .and_then(|e| e.parse::<i64>().ok());
            let (Some(pos), Some(eta)) = (pos, eta) else {
                return None;
            };
            let moved = self.v.queue_position != Some(pos);
            self.v.queue_position = Some(pos);
            self.v.queue_eta_s = Some(eta.max(0));
            if moved {
                let mut d = format!("position {pos}");
                if let Some(e) = fmt_secs(eta.max(0) as f64) {
                    d.push_str(&format!(", ETA {e}"));
                }
                out.push((EventKind::SessionQueue, t, d));
            }
            return Some(out);
        }
        if let Some(rest) = body
            .find("<Hub> ")
            .map(|i| &body[i + "<Hub> ".len()..])
            .filter(|r| r.starts_with("connected") || r.starts_with("disconnected"))
        {
            let up = rest.starts_with("connected");
            let code = between(rest, "[", "]").and_then(|c| c.parse::<i64>().ok());
            let was = self.v.hub.clone();
            self.v.hub = Some(if up { "up" } else { "down" }.into());
            if !up && was.as_deref() != Some("down") {
                let d = match code {
                    Some(c) => format!("lost (status {c})"),
                    None => "lost".into(),
                };
                out.push((EventKind::SessionHubDown, t, d));
            } else if up && was.as_deref() == Some("down") {
                out.push((EventKind::SessionHubUp, t, "reconnected".into()));
            }
            return Some(out);
        }
        if body.contains("<Channel Disconnected> cause=") {
            let reason = between(body, "reason=\"", "\"").unwrap_or("");
            let rules = between(body, "gamerules=\"", "\"").unwrap_or("");
            if rules.ends_with("_Frontend") {
                return Some(out);
            }
            let reason = if reason.is_empty() { "unknown" } else { reason };
            if let (Some(last), Some(now)) = (self.last_dc, t)
                && (now - last).abs() < DISCONNECT_DEDUP_MS
            {
                return Some(out);
            }
            self.last_dc = t;
            let low = reason.to_ascii_lowercase();
            let expected = EXPECTED_DISCONNECT.iter().any(|x| low.contains(x));
            self.v.disconnects += 1;
            self.v.last_disconnect_reason = Some(reason.to_owned());
            self.v.last_disconnect_expected = Some(expected);
            let up_s = body.split_once("uptime_secs=").and_then(|(_, u)| {
                let n: String = u
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                n.parse::<f64>().ok().filter(|x| x.is_finite() && *x >= 0.0)
            });
            self.last_dc_uptime_s = up_s;
            let up = up_s.and_then(fmt_secs);
            let d = match up {
                Some(u) => format!("{reason} (after {u})"),
                None => reason.to_owned(),
            };
            let kind = if expected {
                EventKind::SessionDisconnect
            } else {
                EventKind::SessionDropped
            };
            out.push((kind, t, d));
            return Some(out);
        }
        None
    }
}
