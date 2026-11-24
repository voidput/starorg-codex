//! Signals from Game.log's debug lines (sc-telemetry `gamelog.py` `SignalsState`, plus the
//! quantum route and spawn parsers of its `gamelog-parser-plan.md`):
//!
//! - **Nearby ships**: `<Local Route Guard - ...> ... | NOT AUTH | <CLASS>_<id>|CSCItemNavigation::PostInitialize`.
//!   The class of a ship whose navigation starts in your client (your streaming bubble): no
//!   owner, position or hostility, and parked ships count too. De-duplicated per entity id
//!   here; only class names leave. One `nearby_ship` event a second at most.
//! - **Own ship**: every entity in a seat release (`ClearDriver ... releasing control token
//!   for '<CLASS>_<id>'`) or a quantum target selection is yours: never "nearby".
//! - **Quantum**: `Player Selected Quantum Target - Local` (route), `Calculate Route` (fuel
//!   estimate, the projected start location: the best "where am I" the log has), `Quantum
//!   Drive Arrived` (your arrival ends the route; another ship's counts as a sighting).
//! - **Zone**: single-line HUD notices (armistice, monitored space, jurisdictions).
//! - **Vehicle destroyed** nearby (class, reason, soft or hard; the killer's name goes only to
//!   the People panel, never into an event).
//! - **Corpse items**: one item list near you (5 s window), owner unknown. Followed within
//!   60 s by your own `Spawned!`, it is counted as your death.
//! - **Spawns**: `[CSessionManager::OnClientSpawned] Spawned!`: the first after a join is the
//!   login, later ones are respawns.
//!
//! Entity ids are used inside this state only (de-duplication, own-ship exclusion).

use super::{Ev, EventKind, between, display_class, split_entity};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Classes in the public nearby list (most recently seen first).
pub const NEARBY_CAP: usize = 50;
/// Entities remembered per shard (oldest seen dropped first).
pub const ENTITY_CAP: usize = 1000;
/// At most one `nearby_ship` event per second of wall time; a burst becomes "N new ships".
pub const BURST_MS: i64 = 1000;
/// Corpse item lines within this many ms (log time) are one corpse.
pub const CORPSE_WINDOW_MS: i64 = 5000;
/// A corpse list this long before your respawn is counted as your death.
pub const DEATH_WINDOW_MS: i64 = 60_000;
/// A quantum route with no arrival is dropped after this long (there is no abort line).
pub const ROUTE_TIMEOUT_MS: i64 = 10 * 60_000;
const TEXT_MAX: usize = 120;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneView {
    /// `armistice`, `monitored`, `ungoverned`, `jurisdiction` or `other`.
    pub kind: String,
    pub text: String,
    pub since_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NearbyClass {
    pub class: String,
    pub display: String,
    /// Distinct ship instances seen this shard (not people; parked and AI ships included).
    pub count: u32,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OwnShip {
    pub class: String,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QtView {
    /// `idle` or `routed` (a destination picked, no arrival yet).
    pub state: String,
    /// The game's destination code (`Stanton1_Lorville`...).
    pub destination: Option<String>,
    pub fuel_estimate: Option<f64>,
    pub since_ms: Option<i64>,
    /// Where the last quantum trip ended.
    pub last_arrival: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SignalsView {
    pub zone: Option<ZoneView>,
    pub armistice: bool,
    pub jurisdiction: Option<String>,
    /// The projected start of the last route: system, gateway, planet, moon or station.
    pub location: Option<String>,
    pub qt: QtView,
    pub own_ship: Option<OwnShip>,
    pub nearby: Vec<NearbyClass>,
    pub nearby_count: u32,
    pub nearby_classes: u32,
    /// Respawns that followed a corpse list near you (Game.log has no owner in that line).
    pub deaths: u32,
    pub respawns: u32,
    pub vehicles_destroyed: u32,
}

#[derive(Debug, Clone)]
struct Ship {
    cls: String,
    first: Option<i64>,
    last: Option<i64>,
}

#[derive(Debug, Clone)]
struct Corpse {
    start: Option<i64>,
    last: Option<i64>,
    count: u32,
    touched: bool,
}

#[derive(Debug, Clone)]
struct Route {
    dest: String,
    fuel: Option<f64>,
    since: Option<i64>,
}

#[derive(Debug, Default)]
pub struct SignalsState {
    ships: HashMap<String, Ship>,
    /// Entity ids in first-seen order (for the cap).
    order: VecDeque<String>,
    zone: Option<ZoneView>,
    armistice: bool,
    jurisdiction: Option<String>,
    location: Option<String>,
    route: Option<Route>,
    last_arrival: Option<String>,
    own: Option<String>,
    own_seen: HashSet<String>,
    /// New sightings not announced yet: (log time, class, entity id).
    pending: Vec<(Option<i64>, String, String)>,
    corpse: Option<Corpse>,
    /// The last corpse list that closed: (log time, items).
    last_corpse: Option<(Option<i64>, u32)>,
    last_emit: Option<i64>,
    spawns: u32,
    deaths: u32,
    respawns: u32,
    destroyed: u32,
}

/// HUD notice text -> (kind, text, armistice on/off, jurisdiction) or `None` when it is not
/// about a zone (hangar, injuries, calibration...).
pub fn classify_zone(text: &str) -> Option<(&'static str, String, Option<bool>, Option<String>)> {
    let t = text
        .trim_end_matches(|c: char| c.is_whitespace() || c == ':')
        .trim();
    let low = t.to_ascii_lowercase();
    let entering = low.starts_with("entered ") || low.starts_with("entering ");
    let leaving = ["left ", "leaving ", "exited ", "exiting "]
        .iter()
        .any(|p| low.starts_with(p));
    if !(entering || leaving) {
        return None;
    }
    let text = t.chars().take(TEXT_MAX).collect::<String>();
    if low.contains("armistice") {
        let kind = if entering { "armistice" } else { "other" };
        return Some((kind, text, Some(entering), None));
    }
    if low.contains("monitored space") {
        return Some((
            if entering { "monitored" } else { "other" },
            text,
            None,
            None,
        ));
    }
    if low.contains("jurisdiction") && entering {
        let name = t.split_once(' ').map_or("", |(_, r)| r);
        let name = match name.to_ascii_lowercase().find(" jurisdiction") {
            Some(i) => &name[..i],
            None => name,
        }
        .trim();
        let kind = if name.eq_ignore_ascii_case("ungoverned") {
            "ungoverned"
        } else {
            "jurisdiction"
        };
        let j = (!name.is_empty()).then(|| name.chars().take(TEXT_MAX).collect());
        return Some((kind, text, None, j));
    }
    if low.contains("zone") || low.contains("space") || low.contains("jurisdiction") {
        return Some(("other", text, None, None));
    }
    None
}

/// The entity of an ItemNavigation line: `... | NOT AUTH | <CLASS>_<id>[<n>]|<Fn>|...`.
fn nav_entity(body: &str) -> Option<(&str, &str)> {
    let at = body.find(" | NOT ")? + " | NOT ".len();
    let rest = &body[at..];
    let rest = &rest[rest.find(" | ")? + 3..];
    let ent = &rest[..rest.find('|')?];
    let ent = ent.split('[').next().unwrap_or(ent).trim();
    split_entity(ent)
}

fn word(s: &str) -> Option<&str> {
    s.split_whitespace()
        .next()
        .map(|w| w.trim_end_matches([',', '.']))
        .filter(|w| !w.is_empty() && w.len() <= TEXT_MAX)
}

impl SignalsState {
    /// A new shard (join, change, leave): ships, zone and route of the old one are history.
    /// Deaths and respawns are counted for the whole Game.log.
    pub fn reset_shard(&mut self) {
        let (deaths, respawns, destroyed) = (self.deaths, self.respawns, self.destroyed);
        *self = Self::default();
        self.deaths = deaths;
        self.respawns = respawns;
        self.destroyed = destroyed;
    }

    fn own_ids(&self, eid: &str) -> bool {
        self.own_seen.contains(eid)
    }

    fn forget(&mut self, eid: &str) {
        if self.ships.remove(eid).is_some() {
            self.order.retain(|e| e != eid);
        }
        self.pending.retain(|p| p.2 != eid);
    }

    fn mark_own(&mut self, cls: &str, eid: &str) {
        self.own = Some(cls.to_owned());
        self.own_seen.insert(eid.to_owned());
        self.forget(eid);
    }

    fn sight(&mut self, cls: &str, eid: &str, t: Option<i64>) {
        if self.own_ids(eid) {
            return;
        }
        if let Some(s) = self.ships.get_mut(eid) {
            if t.is_some() {
                s.last = t;
            }
            return;
        }
        self.ships.insert(
            eid.to_owned(),
            Ship {
                cls: cls.to_owned(),
                first: t,
                last: t,
            },
        );
        self.order.push_back(eid.to_owned());
        self.pending.push((t, cls.to_owned(), eid.to_owned()));
        while self.order.len() > ENTITY_CAP {
            if let Some(old) = self.order.pop_front() {
                self.ships.remove(&old);
            }
        }
    }

    fn flush_corpse(&mut self) -> Option<Ev> {
        let c = self.corpse.take()?;
        self.last_corpse = Some((c.last, c.count));
        let n = c.count;
        Some((
            EventKind::CorpseItems,
            c.last,
            format!(
                "Corpse nearby: {n} item{} (owner unknown)",
                if n == 1 { "" } else { "s" }
            ),
        ))
    }

    /// A signal line -> its immediate events (`Some`, maybe empty), else `None`.
    pub fn apply(&mut self, body: &str, t: Option<i64>) -> Option<Vec<Ev>> {
        let mut out = Vec::new();
        if body.contains("<Local Route Guard - ") {
            if body.contains("|CSCItemNavigation::PostInitialize")
                && let Some((cls, eid)) = nav_entity(body)
            {
                self.sight(cls, eid, t);
                return Some(out);
            }
            return None;
        }
        if body.contains("<Quantum Drive Arrived - ") {
            let (cls, eid) = nav_entity(body)?;
            let own = self.own_ids(eid);
            let disp = display_class(cls);
            if own {
                let dest = self.route.take().map(|r| r.dest);
                self.last_arrival = dest.clone().or(self.last_arrival.take());
                let d = match dest {
                    Some(d) => format!("You ({disp}) arrived at {d}"),
                    None => format!("You ({disp}) arrived from quantum"),
                };
                out.push((EventKind::QtArrived, t, d));
            } else {
                self.sight(cls, eid, t);
                out.push((
                    EventKind::QtArrived,
                    t,
                    format!("{disp} arrived from quantum"),
                ));
            }
            return Some(out);
        }
        if body.contains("<Player Selected Quantum Target - Local>") {
            let (cls, eid) = nav_entity(body)?;
            let dest = between(body, "Player has selected point ", " ").and_then(word)?;
            self.mark_own(cls, eid);
            if self.route.as_ref().map(|r| r.dest.as_str()) != Some(dest) {
                self.route = Some(Route {
                    dest: dest.to_owned(),
                    fuel: None,
                    since: t,
                });
                out.push((
                    EventKind::QtRoute,
                    t,
                    format!("Quantum route to {dest} ({})", display_class(cls)),
                ));
            }
            return Some(out);
        }
        if body.contains("<Calculate Route") {
            if let Some(rest) = body
                .split_once("Successfully calculated route to ")
                .map(|x| x.1)
            {
                let dest = word(rest)?;
                let fuel = between(rest, " fuel estimate ", " ")
                    .or_else(|| rest.split_once(" fuel estimate ").map(|x| x.1))
                    .and_then(|f| {
                        f.trim_end_matches(|c: char| !c.is_ascii_digit())
                            .parse::<f64>()
                            .ok()
                    })
                    .filter(|f| f.is_finite());
                if let Some(r) = &mut self.route
                    && r.dest == dest
                {
                    r.fuel = fuel;
                }
                return Some(out);
            }
            if let Some(start) = between(body, "Projected Start Location is ", " for route to ") {
                let start = start.trim();
                if !start.is_empty() {
                    self.location = Some(start.chars().take(TEXT_MAX).collect());
                }
                return Some(out);
            }
            return None;
        }
        if let Some(i) = body.find("<SHUDEvent_OnNotification> Added notification \"") {
            let rest = &body[i + "<SHUDEvent_OnNotification> Added notification \"".len()..];
            let text = &rest[..rest.find('"')?];
            let (kind, text, armistice, jurisdiction) = classify_zone(text)?;
            if let Some(a) = armistice {
                self.armistice = a;
            }
            if jurisdiction.is_some() {
                self.jurisdiction = jurisdiction;
            }
            let same = self.zone.as_ref().is_some_and(|z| z.text == text);
            if !same {
                self.zone = Some(ZoneView {
                    kind: kind.into(),
                    text: text.clone(),
                    since_ms: t,
                });
                out.push((EventKind::Zone, t, text));
            }
            return Some(out);
        }
        if body.contains("<Vehicle Control Flow> CVehicleMovementBase::ClearDriver:") {
            let ent = between(body, "releasing control token for '", "'")?;
            let (cls, eid) = split_entity(ent)?;
            self.mark_own(cls, eid);
            out.push((
                EventKind::VehicleReleased,
                t,
                format!("Left the pilot seat of {}", display_class(cls)),
            ));
            return Some(out);
        }
        if body.contains("<Vehicle Destroyed> Vehicle '") {
            let ent = between(body, "<Vehicle Destroyed> Vehicle '", "'")?;
            let cls = between(body, " class '", "'").filter(|c| !c.is_empty())?;
            let reason = between(body, "destroyed: reason [", "]").unwrap_or("unknown");
            let to = between(body, "] -> [", "]");
            let soft = to == Some("1");
            let own = split_entity(ent).is_some_and(|(_, eid)| self.own_ids(eid));
            self.destroyed += 1;
            let what = if soft { "disabled" } else { "destroyed" };
            let who = if own {
                format!("Your {}", display_class(cls))
            } else {
                format!("{} nearby", display_class(cls))
            };
            out.push((
                EventKind::VehicleDestroyed,
                t,
                format!("{who} was {what} ({reason})"),
            ));
            return Some(out);
        }
        if body.contains(
            "<Adding non kept item [CSCActorCorpseUtils::PopulateItemPortForItemRecoveryEntitlement]>",
        ) {
            if let Some(c) = &self.corpse
                && let (Some(start), Some(now)) = (c.start, t)
                && now - start > CORPSE_WINDOW_MS
            {
                out.extend(self.flush_corpse());
            }
            let c = self.corpse.get_or_insert(Corpse {
                start: t,
                last: t,
                count: 0,
                touched: true,
            });
            if c.start.is_none() {
                c.start = t;
            }
            if t.is_some() {
                c.last = t;
            }
            c.count += 1;
            c.touched = true;
            return Some(out);
        }
        if body.contains("[CSessionManager::OnClientSpawned] Spawned!") {
            out.extend(self.flush_corpse());
            self.spawns += 1;
            if self.spawns == 1 {
                self.last_corpse = None;
                out.push((EventKind::Spawned, t, "Spawned".into()));
                return Some(out);
            }
            self.respawns += 1;
            let died = self.last_corpse.take().filter(|(end, _)| match (end, t) {
                (Some(e), Some(now)) => now >= *e && now - e <= DEATH_WINDOW_MS,
                _ => true,
            });
            let d = match died {
                Some((_, n)) => {
                    self.deaths += 1;
                    format!(
                        "Respawned after a death ({n} item{} left on the body)",
                        if n == 1 { "" } else { "s" }
                    )
                }
                None => "Respawned".into(),
            };
            out.push((EventKind::Respawned, t, d));
            return Some(out);
        }
        None
    }

    /// End of a poll: the queued sightings as one `nearby_ship` (rate limited by `now_ms`,
    /// wall time) and a corpse list that got no new line this poll.
    pub fn drain(&mut self, now_ms: i64) -> Vec<Ev> {
        let mut out = Vec::new();
        if let Some(c) = &mut self.corpse {
            if c.touched {
                c.touched = false;
            } else {
                out.extend(self.flush_corpse());
            }
        }
        let due = self.last_emit.is_none_or(|l| now_ms - l >= BURST_MS);
        if !self.pending.is_empty() && due {
            let pend = std::mem::take(&mut self.pending);
            self.last_emit = Some(now_ms);
            let mut classes: Vec<&str> = Vec::new();
            for (_, c, _) in &pend {
                if !classes.contains(&c.as_str()) {
                    classes.push(c);
                }
            }
            let t = pend.iter().filter_map(|p| p.0).max();
            let n = pend.len();
            let d = if n == 1 {
                display_class(classes[0])
            } else {
                let names: Vec<String> = classes.iter().take(3).map(|c| display_class(c)).collect();
                format!(
                    "{n} new ships: {}{}",
                    names.join(", "),
                    if classes.len() > 3 { "…" } else { "" }
                )
            };
            out.push((EventKind::NearbyShip, t, d));
        }
        out
    }

    /// `last_t`: the newest log time, for the route timeout.
    pub fn view(&self, last_t: Option<i64>) -> SignalsView {
        let mut groups: HashMap<&str, NearbyClass> = HashMap::new();
        for s in self.ships.values() {
            let g = groups.entry(&s.cls).or_insert_with(|| NearbyClass {
                class: s.cls.clone(),
                display: display_class(&s.cls),
                count: 0,
                first_ms: s.first,
                last_ms: s.last,
            });
            g.count += 1;
            g.first_ms = match (g.first_ms, s.first) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            g.last_ms = g.last_ms.max(s.last);
        }
        let classes = groups.len() as u32;
        let mut nearby: Vec<NearbyClass> = groups.into_values().collect();
        nearby.sort_by(|a, b| {
            b.last_ms
                .cmp(&a.last_ms)
                .then_with(|| a.class.cmp(&b.class))
        });
        nearby.truncate(NEARBY_CAP);
        let route = self.route.as_ref().filter(|r| match (r.since, last_t) {
            (Some(s), Some(now)) => now - s <= ROUTE_TIMEOUT_MS,
            _ => true,
        });
        SignalsView {
            zone: self.zone.clone(),
            armistice: self.armistice,
            jurisdiction: self.jurisdiction.clone(),
            location: self.location.clone(),
            qt: QtView {
                state: if route.is_some() { "routed" } else { "idle" }.into(),
                destination: route.map(|r| r.dest.clone()),
                fuel_estimate: route.and_then(|r| r.fuel),
                since_ms: route.and_then(|r| r.since),
                last_arrival: self.last_arrival.clone(),
            },
            own_ship: self.own.as_ref().map(|c| OwnShip {
                class: c.clone(),
                display: display_class(c),
            }),
            nearby,
            nearby_count: self.ships.len() as u32,
            nearby_classes: classes,
            deaths: self.deaths,
            respawns: self.respawns,
            vehicles_destroyed: self.destroyed,
        }
    }
}
