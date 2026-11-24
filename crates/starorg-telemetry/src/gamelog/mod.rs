//! Game.log signals (experimental): what the telemetry API does not give, read from the
//! channel's own `Game.log` on this computer. A Rust port of sc-telemetry's `gamelog.py` and
//! the local part of `people.py` (same owner, MIT; `docs/telemetry/gamelog-signals.md` and
//! `people-and-lookup.md` there).
//!
//! - **Session**: build, channel, shard and region, star system, login queue, backend hub
//!   link, disconnects ([`session`]). Server address, port and session ids are never kept.
//! - **Signals** ([`signals`]): ships streamed in nearby (class names only), quantum route and
//!   arrivals, HUD zone notices (armistice, monitored space, jurisdiction), leaving the pilot
//!   seat, vehicles destroyed nearby, corpse item lists and spawns / respawns.
//! - **People** ([`people`], its own switch): handles named in Game.log lines, grouped by how
//!   they relate to you (crew, party, party?, nearby). Memory only, for the current Game.log;
//!   never in events, the Activity log or the diagnostics text. No network lookup.
//!
//! All of it is the game's unofficial debug logging: formats change between patches, and a
//! line we do not recognise is skipped. [`tail::Tail`] follows the file (only new bytes, a
//! bounded read per poll, rotation and truncation detected). Tests use synthetic lines with
//! fake ids and handles only.

pub mod mock;
pub mod people;
pub mod session;
pub mod signals;
pub mod stints;
pub mod tail;

#[cfg(test)]
mod tests;

use serde::Serialize;
use std::collections::VecDeque;

pub use people::PeopleView;
pub use session::SessionView;
pub use signals::SignalsView;
pub use stints::{LeftReason, Stint};

/// Events kept (oldest dropped first).
pub const EVENTS_CAP: usize = 200;

/// What a [`LogEvent`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// `Log started`: the game was (re)started.
    LogStarted,
    SessionJoin,
    SessionShardChange,
    /// Back at the main menu.
    SessionLeave,
    /// A loading screen for a star system (or the main menu) closed.
    SessionSystem,
    SessionQueue,
    SessionHubDown,
    SessionHubUp,
    /// You disconnected (quit, exit to menu).
    SessionDisconnect,
    /// The connection dropped.
    SessionDropped,
    /// Ships streamed in near you (first sighting; bursts become one event).
    NearbyShip,
    /// You picked a quantum destination.
    QtRoute,
    QtArrived,
    Zone,
    VehicleReleased,
    VehicleDestroyed,
    /// A corpse's item list near you (no owner in the line).
    CorpseItems,
    /// The first spawn of a session (login).
    Spawned,
    Respawned,
}

/// One line of the Events panel. Never carries an entity id, an address or a handle.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEvent {
    /// Increases by one per event of this reader (stable key for the UI).
    pub seq: u64,
    pub kind: EventKind,
    /// Log time, Unix ms (`None` for a line without a time stamp).
    pub t_ms: Option<i64>,
    pub detail: String,
}

/// What the Events and People panels show.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameLogView {
    /// `game_log`, or `mock` (synthetic lines, the mock backend).
    pub source: String,
    /// Game.log is there and was read.
    pub found: bool,
    /// New Game.log files seen since the reader started (the game rewrites it on every start).
    pub rotations: u32,
    /// Log time of the last time-stamped line, Unix ms.
    pub last_line_ms: Option<i64>,
    pub session: SessionView,
    pub signals: SignalsView,
    /// Oldest first, at most [`EVENTS_CAP`].
    pub events: Vec<LogEvent>,
    /// `None` while the People switch is off (no handle is even kept then).
    pub people: Option<PeopleView>,
    /// The shard stint you are on now ([`stints`]); `None` off a shard.
    pub stint: Option<Stint>,
    /// The join history's revision (the host's [`stints::Store`]): when it moves, the
    /// Sessions panel reads the list again. 0 from [`GameLog::view`].
    pub sessions_rev: u64,
}

/// An event before it gets its number.
pub(crate) type Ev = (EventKind, Option<i64>, String);

/// Everything read from one Game.log: folds lines, keeps the bounded event list.
#[derive(Debug)]
pub struct GameLog {
    session: session::SessionState,
    signals: signals::SignalsState,
    people: Option<people::PeopleState>,
    stints: stints::Tracker,
    events: VecDeque<LogEvent>,
    seq: u64,
    version: u64,
    last_t: Option<i64>,
    /// A line that spans two reads.
    partial: Vec<u8>,
}

/// A line longer than this is not a log line we know: it is dropped.
pub const LINE_MAX: usize = 1 << 20;

impl GameLog {
    /// `people`: keep handles for the People panel (its own switch).
    pub fn new(people: bool) -> Self {
        Self {
            session: session::SessionState::default(),
            signals: signals::SignalsState::default(),
            people: people.then(people::PeopleState::default),
            stints: stints::Tracker::default(),
            events: VecDeque::new(),
            seq: 0,
            version: 0,
            last_t: None,
            partial: Vec::new(),
        }
    }

    /// Bumped by every change; the host sends the view when it moved.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// A new Game.log (rotation, truncation, file gone): everything starts over.
    pub fn reset(&mut self) {
        // The old file ended: a stint still open on it ends with it.
        self.stints.end_log();
        self.session = session::SessionState::default();
        self.signals = signals::SignalsState::default();
        if let Some(p) = &mut self.people {
            p.reset();
        }
        self.events.clear();
        self.last_t = None;
        self.partial.clear();
        self.version += 1;
    }

    /// Forget every person seen (the People panel's Clear). Your own handle is kept, so later
    /// lines naming you are still not listed as someone else.
    pub fn clear_people(&mut self) {
        if let Some(p) = &mut self.people {
            p.clear();
            self.version += 1;
        }
    }

    /// Every handle held in memory (yours and other people's), for the diagnostics redaction.
    pub fn handles(&self) -> Vec<String> {
        self.people
            .as_ref()
            .map(|p| p.all_names())
            .unwrap_or_default()
    }

    fn push(&mut self, evs: Vec<Ev>) {
        for (kind, t_ms, detail) in evs {
            self.seq += 1;
            self.events.push_back(LogEvent {
                seq: self.seq,
                kind,
                t_ms,
                detail,
            });
            while self.events.len() > EVENTS_CAP {
                self.events.pop_front();
            }
            self.version += 1;
        }
    }

    /// Raw bytes as read from the file: complete lines are folded, a partial last line waits
    /// for the next read.
    pub fn feed_bytes(&mut self, data: &[u8]) {
        let mut rest = data;
        while let Some(nl) = memchr::memchr(b'\n', rest) {
            let (line, tail) = rest.split_at(nl);
            rest = &tail[1..];
            if self.partial.is_empty() {
                self.fold_line(&String::from_utf8_lossy(line));
            } else {
                let mut l = std::mem::take(&mut self.partial);
                l.extend_from_slice(line);
                if l.len() <= LINE_MAX {
                    self.fold_line(&String::from_utf8_lossy(&l));
                }
            }
        }
        self.partial.extend_from_slice(rest);
        if self.partial.len() > LINE_MAX {
            // Not a log line; the rest of it is skipped as junk when the newline comes.
            self.partial.clear();
        }
    }

    /// One Game.log line (CRLF and a UTF-8 BOM are fine).
    pub fn fold_line(&mut self, line: &str) {
        let line = line
            .trim_end_matches(['\r', '\n'])
            .trim_start_matches('\u{feff}');
        let mut changed = false;
        if let Some(p) = &mut self.people {
            changed |= p.feed(line);
        }
        let (ts, body) = split_ts(line);
        let t = ts.and_then(parse_ts);
        if t.is_some() && t != self.last_t {
            self.last_t = t;
        }
        self.stints.saw(t);
        if let Some(evs) = self.session.apply(body, t) {
            self.stints.observe(&evs, &self.session, t);
            let started = evs.iter().any(|e| e.0 == EventKind::LogStarted);
            if started {
                // A new launch: identity, people and signals of the old one are history.
                if let Some(p) = &mut self.people {
                    p.reset();
                }
                self.signals = signals::SignalsState::default();
            } else if evs.iter().any(|e| {
                matches!(
                    e.0,
                    EventKind::SessionJoin
                        | EventKind::SessionShardChange
                        | EventKind::SessionLeave
                )
            }) {
                self.signals.reset_shard();
                if let Some(p) = &mut self.people {
                    p.clear_nearby();
                }
            }
            self.version += 1;
            self.push(evs);
            return;
        }
        if let Some(evs) = self.signals.apply(body, t) {
            self.version += 1;
            self.push(evs);
            return;
        }
        if changed {
            self.version += 1;
        }
    }

    /// End of a poll: queued nearby sightings (at most one event a second of `now_ms`, wall
    /// time) and a corpse list that got no line this poll.
    pub fn end_poll(&mut self, now_ms: i64) {
        let evs = self.signals.drain(now_ms);
        self.push(evs);
    }

    /// Shard stints that ended since the last call (for the join history), oldest first.
    pub fn drain_stints(&mut self) -> Vec<Stint> {
        self.stints.drain()
    }

    pub fn view(&self, source: &str, found: bool, rotations: u32) -> GameLogView {
        GameLogView {
            source: source.into(),
            found,
            rotations,
            last_line_ms: self.last_t,
            session: self.session.view(),
            signals: self.signals.view(self.last_t),
            events: self.events.iter().cloned().collect(),
            people: self.people.as_ref().map(people::PeopleState::view),
            stint: self.stints.current().cloned(),
            sessions_rev: 0,
        }
    }
}

/// `<2026-11-21T12:34:56.789Z> body` -> (time stamp, body); a line without one is all body.
pub(crate) fn split_ts(line: &str) -> (Option<&str>, &str) {
    if let Some(rest) = line.strip_prefix('<')
        && rest.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && let Some(end) = rest.find('>')
        && end <= 40
    {
        let body = &rest[end + 1..];
        return (Some(&rest[..end]), body.strip_prefix(' ').unwrap_or(body));
    }
    (None, line)
}

/// `2026-11-21T12:34:56.789Z` (UTC, as Game.log writes it) -> Unix ms.
pub fn parse_ts(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if !ts.is_ascii()
        || b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let s = &ts[r];
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())
            .flatten()
    };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let frac = ts[19..].strip_suffix('Z')?;
    let ms = match frac.strip_prefix('.') {
        None if frac.is_empty() => 0,
        None => return None,
        Some(f) if !f.is_empty() && f.bytes().all(|c| c.is_ascii_digit()) => {
            let f3: String = f.chars().chain("00".chars()).take(3).collect();
            f3.parse::<i64>().ok()?
        }
        Some(_) => return None,
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s) * 1000 + ms)
}

/// Days since 1970-01-01 (proleptic Gregorian; H. Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The text between `start` and the next `end` after it.
pub(crate) fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let at = s.find(start)? + start.len();
    let rest = &s[at..];
    Some(&rest[..rest.find(end)?])
}

/// `RSI_Perseus_000000000001` -> (`RSI_Perseus`, `000000000001`): a class name followed by an
/// entity id of six digits or more. `None` without one.
pub(crate) fn split_entity(ent: &str) -> Option<(&str, &str)> {
    let (cls, id) = ent.rsplit_once('_')?;
    let ok_cls = cls.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && cls.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let ok_id = id.len() >= 6 && id.bytes().all(|c| c.is_ascii_digit());
    (ok_cls && ok_id).then_some((cls, id))
}

/// Class name for people: underscores to spaces.
pub(crate) fn display_class(cls: &str) -> String {
    cls.split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
