//! People seen in Game.log (sc-telemetry `people.py`, the local part only: no org lookup and
//! no network): handles named in Game.log lines, grouped by how they relate to you.
//!
//! | Relation | Lines |
//! |---|---|
//! | `crew` | `You have joined channel '<ship> : <owner>'.`; `<player> has joined/left the channel '<ship> : <owner>'.` |
//! | `party` | party toasts (`New Member Joined`, `Member Left`, `New Party Leader`, `Party` connected/disconnected, `You have joined` + leader), `Initiated by [party leader] <player>`, `party-launch from leader[<player>]`, `<player> has started to QT to your current location.` |
//! | `party_likely` | `Quantum Travel Calibration Started/Complete By <player>` |
//! | `nearby` | `<Vehicle Destroyed> ... killer '<player>' [<id>]` (the victim only once its id was seen as a killer) |
//!
//! Self-only lines (`Handle[...]`, `handle[...]`, `destinations received for player [...]`,
//! the current character's login line, `ClearDriver: Local client node [<id>]`) build your own
//! identity: you are never listed as someone else.
//!
//! **Privacy.** Handles live in memory only, for the current Game.log, and only while the
//! People switch is on. They are never put into events, the Activity log, the diagnostics text
//! (the host redacts every name held here) or any file. Tests use synthetic handles only.

use super::{parse_ts, split_ts};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// People remembered per Game.log (least recently seen nearby-only person dropped first).
pub const PEOPLE_CAP: usize = 300;
/// Vehicle Destroyed player id -> handle entries.
pub const ID_MAP_CAP: usize = 2000;
/// The notification joiner gives up after this many physical lines.
pub const TOAST_MAX_LINES: usize = 5;
pub const NAME_MAX: usize = 64;

/// Strongest first.
pub const RELATIONS: [&str; 4] = ["crew", "party", "party_likely", "nearby"];

const NOTE_START: &str = "<SHUDEvent_OnNotification> Added notification \"";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Person {
    pub name: String,
    /// The strongest relation (`crew`, `party`, `party_likely`, `nearby`).
    pub relation: String,
    pub relations: Vec<String>,
    /// `crew`, `party`, `party?`, `nearby`.
    pub label: String,
    /// Which kinds of line named them (`voice_channel`, `party_toast`, `qt_calibration`...).
    pub sources: Vec<String>,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
    pub count: u32,
    /// Ship of a voice channel they were in.
    pub ship: Option<String>,
    /// Left the party or channel.
    pub left: bool,
    pub leader: bool,
    /// A valid RSI handle (`[A-Za-z0-9_-]`, 2 to 60); other names (with spaces) are shown only.
    pub valid_handle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PeopleCounts {
    pub crew: u32,
    pub party: u32,
    pub party_likely: u32,
    pub nearby: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeopleView {
    pub counts: PeopleCounts,
    /// Strongest relation first, then most recently seen.
    pub people: Vec<Person>,
    /// Your own handle as Game.log names it (shown as "you"), once known.
    pub self_name: Option<String>,
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    relations: Vec<&'static str>,
    sources: Vec<&'static str>,
    first: Option<i64>,
    last: Option<i64>,
    count: u32,
    ship: Option<String>,
    left: bool,
    leader: bool,
    /// Insertion / touch order for eviction.
    touched: u64,
}

#[derive(Debug, Default)]
pub struct PeopleState {
    self_handles: HashSet<String>,
    self_display: Option<String>,
    self_ids: HashSet<String>,
    people: HashMap<String, Entry>,
    ids: HashMap<String, String>,
    id_order: VecDeque<String>,
    victims: HashSet<String>,
    victim_order: VecDeque<String>,
    toast: Option<(Vec<String>, Option<i64>)>,
    tick: u64,
}

/// AI and unnamed actors: empty, `none`, `PU_*`, `AI_*`, `Citizen-<n>`, or a trailing entity
/// id (`_` + 6 digits or more). Never a blanket prefix rule: real handles contain `_`.
pub fn is_npc(name: &str) -> bool {
    let n = name.trim();
    if n.is_empty() || n.eq_ignore_ascii_case("none") {
        return true;
    }
    let low = n.to_ascii_lowercase();
    if low.starts_with("pu_") || low.starts_with("ai_") {
        return true;
    }
    if let Some(d) = low.strip_prefix("citizen-")
        && !d.is_empty()
        && d.bytes().all(|c| c.is_ascii_digit())
    {
        return true;
    }
    n.rsplit_once('_')
        .is_some_and(|(_, id)| id.len() >= 6 && id.bytes().all(|c| c.is_ascii_digit()))
}

/// A display name from a log field: printable, at most [`NAME_MAX`] characters, no quotes,
/// not an NPC.
pub fn clean_name(name: &str) -> Option<String> {
    let n = name.trim();
    let ok = !n.is_empty()
        && n.chars().count() <= NAME_MAX
        && !n.chars().any(|c| c.is_control() || c == '\'' || c == '"')
        && !is_npc(n);
    ok.then(|| n.to_owned())
}

pub fn valid_handle(name: &str) -> bool {
    (2..=60).contains(&name.len())
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn label(rel: &str) -> &'static str {
    match rel {
        "crew" => "crew",
        "party" => "party",
        "party_likely" => "party?",
        _ => "nearby",
    }
}

fn rank(rel: &str) -> usize {
    RELATIONS
        .iter()
        .position(|r| *r == rel)
        .unwrap_or(RELATIONS.len())
}

/// `<p> has joined the party.` -> `<p>` (`.` optional).
fn strip_suffix<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let s = s.strip_suffix('.').unwrap_or(s);
    let suffix = suffix.strip_suffix('.').unwrap_or(suffix);
    s.strip_suffix(suffix).filter(|p| !p.is_empty())
}

/// `'<ship> : <owner>'` (the quote marks included, an optional `.` after).
fn channel_parts(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_suffix('.').unwrap_or(s);
    let inner = s.strip_prefix('\'')?.strip_suffix('\'')?;
    let (ship, owner) = inner.rsplit_once(" : ")?;
    (!ship.is_empty() && !owner.contains('\'')).then_some((ship, owner))
}

/// (relation, name, source, ship, left, leader)
type Hit<'a> = (
    &'static str,
    &'a str,
    &'static str,
    Option<&'a str>,
    Option<bool>,
    bool,
);

/// Joined toast text (title, then body lines) -> people named in it. Pure.
pub fn toast_hits(text: &str) -> Vec<Hit<'_>> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let Some(&title) = lines.first() else {
        return Vec::new();
    };
    if let Some(rest) = title.strip_prefix("Quantum Travel Calibration ") {
        let p = rest
            .strip_prefix("Started By ")
            .or_else(|| rest.strip_prefix("Complete By "));
        return p
            .map(|p| vec![("party_likely", p, "qt_calibration", None, None, false)])
            .unwrap_or_default();
    }
    if let Some(rest) = title.strip_prefix("You have joined channel ") {
        return channel_parts(rest)
            .map(|(ship, owner)| vec![("crew", owner, "voice_channel", Some(ship), None, false)])
            .unwrap_or_default();
    }
    if title == "You have joined" && lines.len() == 2 {
        // Real logs only ever carry the literal word "party" here: never a name.
        let l = lines[1];
        if !l.eq_ignore_ascii_case("party")
            && !l.eq_ignore_ascii_case("the party")
            && valid_handle(l)
        {
            return vec![("party", l, "party_toast", None, None, true)];
        }
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, &ln) in lines.iter().enumerate() {
        let channel = ln
            .split_once(" has joined the channel ")
            .map(|(p, r)| (p, r, false))
            .or_else(|| {
                ln.split_once(" has left the channel ")
                    .map(|(p, r)| (p, r, true))
            });
        if let Some((p, rest, left)) = channel
            && !p.is_empty()
            && let Some((ship, owner)) = channel_parts(rest)
        {
            out.push(("crew", p, "voice_channel", Some(ship), Some(left), false));
            out.push(("crew", owner, "voice_channel", Some(ship), None, false));
            continue;
        }
        if i > 0
            && title == "New Member Joined"
            && let Some(p) = strip_suffix(ln, " has joined the party")
        {
            out.push(("party", p, "party_toast", None, Some(false), false));
            continue;
        }
        if i > 0
            && title == "Member Left"
            && let Some(p) = strip_suffix(ln, " has left the party")
        {
            out.push(("party", p, "party_toast", None, Some(true), false));
            continue;
        }
        let by_leader = ln
            .strip_prefix("Initiated by party leader ")
            .or_else(|| ln.strip_prefix("Join queue canceled by party leader "))
            .map(|p| p.strip_suffix('.').unwrap_or(p))
            .filter(|p| !p.is_empty());
        let new_leader = (i > 0 && title == "New Party Leader")
            .then(|| strip_suffix(ln, " is now party leader"))
            .flatten();
        if let Some(p) = new_leader.or(by_leader) {
            out.push(("party", p, "party_toast", None, None, true));
            continue;
        }
        if let Some(p) = strip_suffix(ln, " has started to QT to your current location") {
            out.push(("party", p, "qt_incoming", None, None, false));
            continue;
        }
        if let Some(p) = ln
            .strip_prefix("Initiated by ")
            .map(|p| p.strip_suffix('.').unwrap_or(p))
            .filter(|p| !p.is_empty())
            && (i > 0 || lines.len() > 1 && lines[1].starts_with("Party Launch"))
        {
            out.push(("party", p, "party_launch", None, None, true));
            continue;
        }
        if i > 0 && title == "Party" {
            let p = strip_suffix(ln, " disconnected").or_else(|| strip_suffix(ln, " connected"));
            if let Some(p) = p {
                out.push(("party", p, "party_toast", None, None, false));
            }
        }
    }
    out
}

/// A tagged log line (`[Notice] <Tag>`, optionally after `[SPAM <n>]`): it ends a toast.
fn tagged(body: &str) -> bool {
    let b = body.trim_start();
    let b = match b.strip_prefix("[SPAM ") {
        Some(r) => r.split_once(']').map_or(r, |(n, rest)| {
            if n.bytes().all(|c| c.is_ascii_digit()) {
                rest.trim_start()
            } else {
                r
            }
        }),
        None => b,
    };
    let Some(r) = b.strip_prefix('[') else {
        return false;
    };
    let Some((lvl, rest)) = r.split_once(']') else {
        return false;
    };
    !lvl.is_empty()
        && lvl.chars().all(|c| c.is_alphanumeric() || c == '_')
        && rest.starts_with(" <")
}

/// The toast text up to `" [<n>] to queue` (`None` while the end has not arrived).
fn toast_end(text: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(i) = text[from..].find("\" [") {
        let at = from + i;
        let rest = &text[at + 3..];
        if let Some(close) = rest.find(']')
            && close > 0
            && rest[..close].bytes().all(|c| c.is_ascii_digit())
            && rest[close + 1..].starts_with(" to queue")
        {
            return Some(&text[..at]);
        }
        from = at + 1;
    }
    None
}

impl PeopleState {
    /// New Game.log / `Log started`: identity and every person are forgotten.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The panel's Clear: every person goes; your own identity stays.
    pub fn clear(&mut self) {
        self.people.clear();
        self.ids.clear();
        self.id_order.clear();
        self.victims.clear();
        self.victim_order.clear();
        self.toast = None;
    }

    /// Shard join, change or leave: nearby-only people are history; crew and party stay.
    pub fn clear_nearby(&mut self) {
        self.people
            .retain(|_, p| p.relations.iter().any(|r| *r != "nearby"));
        self.victims.clear();
        self.victim_order.clear();
    }

    /// Every name held (yours too), for redaction.
    pub fn all_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.people.values().map(|p| p.name.clone()).collect();
        v.extend(self.self_display.iter().cloned());
        v
    }

    fn is_self(&self, name: Option<&str>, pid: Option<&str>) -> bool {
        name.is_some_and(|n| self.self_handles.contains(&n.to_lowercase()))
            || pid.is_some_and(|p| self.self_ids.contains(p))
    }

    fn add_self(&mut self, handle: Option<&str>, geid: Option<&str>) {
        if let Some(h) = handle.and_then(clean_name) {
            let k = h.to_lowercase();
            self.people.remove(&k);
            self.self_handles.insert(k);
            self.self_display.get_or_insert(h);
        }
        if let Some(g) = geid.filter(|g| !g.is_empty() && g.bytes().all(|c| c.is_ascii_digit())) {
            self.self_ids.insert(g.to_owned());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn see(
        &mut self,
        rel: &'static str,
        name: &str,
        source: &'static str,
        t: Option<i64>,
        ship: Option<&str>,
        left: Option<bool>,
        leader: bool,
    ) {
        let Some(n) = clean_name(name) else { return };
        if self.is_self(Some(&n), None) {
            return;
        }
        self.tick += 1;
        let k = n.to_lowercase();
        if !self.people.contains_key(&k) {
            self.people.insert(
                k.clone(),
                Entry {
                    name: n,
                    relations: Vec::new(),
                    sources: Vec::new(),
                    first: t,
                    last: t,
                    count: 0,
                    ship: None,
                    left: false,
                    leader: false,
                    touched: self.tick,
                },
            );
            while self.people.len() > PEOPLE_CAP {
                let victim = self
                    .people
                    .iter()
                    .filter(|(kk, p)| **kk != k && p.relations.iter().all(|r| *r == "nearby"))
                    .min_by_key(|(_, p)| p.touched)
                    .or_else(|| {
                        self.people
                            .iter()
                            .filter(|(kk, _)| **kk != k)
                            .min_by_key(|(_, p)| p.touched)
                    })
                    .map(|(kk, _)| kk.clone());
                match victim {
                    Some(v) => {
                        self.people.remove(&v);
                    }
                    None => break,
                }
            }
        }
        let Some(p) = self.people.get_mut(&k) else {
            return;
        };
        if !p.relations.contains(&rel) {
            p.relations.push(rel);
            p.relations.sort_by_key(|r| rank(r));
        }
        if !p.sources.contains(&source) {
            p.sources.push(source);
        }
        p.count += 1;
        p.touched = self.tick;
        if let Some(t) = t {
            p.last = Some(p.last.map_or(t, |l| l.max(t)));
            p.first = Some(p.first.map_or(t, |f| f.min(t)));
        }
        if let Some(s) = ship {
            p.ship = Some(s.chars().take(NAME_MAX).collect());
        }
        if left.is_some() || rel == "party" || rel == "crew" {
            p.left = left.unwrap_or(false);
        }
        if leader {
            p.leader = true;
        }
    }

    /// One raw Game.log line (any line; cheap substring checks skip the rest). True when the
    /// table changed.
    pub fn feed(&mut self, line: &str) -> bool {
        let (ts, body) = split_ts(line);
        let before = (self.tick, self.self_handles.len(), self.self_ids.len());
        if let Some((mut parts, tts)) = self.toast.take() {
            if tagged(body) || body.starts_with([' ', '\t']) || ts.is_none() {
                // Abandoned (a queue dump or another tagged line): nothing from it.
            } else {
                parts.push(body.to_owned());
                self.try_toast(parts, tts);
                return self.changed(before);
            }
        }
        if let Some(i) = body.find(NOTE_START) {
            let first = body[i + NOTE_START.len()..].to_owned();
            self.try_toast(vec![first], ts.and_then(parse_ts));
            return self.changed(before);
        }
        let t = ts.and_then(parse_ts);
        if body.contains("<Vehicle Destroyed>") {
            self.vehicle_destroyed(body, t);
            return self.changed(before);
        }
        if let Some(p) = super::between(body, "party-launch from leader[", "]") {
            let p = p.to_owned();
            self.see("party", &p, "party_launch", t, None, None, true);
            return self.changed(before);
        }
        let mut handle = super::between(body, "User Login Success - Handle[", "]")
            .or_else(|| super::between(body, "Fully authenticated : handle[", "]"))
            .or_else(|| super::between(body, "destinations received for player [", "]"))
            .map(str::to_owned);
        let mut geid = None;
        if body.contains("<AccountLoginCharacterStatus_Character> Character: ")
            && body.contains(" - state ")
        {
            geid = super::between(body, " geid ", " ").map(str::to_owned);
            handle = super::between(body, " - name ", " - state").map(|s| s.trim().to_owned());
        } else if let Some(g) = super::between(body, "ClearDriver: Local client node [", "]") {
            geid = Some(g.to_owned());
        }
        if handle.is_some() || geid.is_some() {
            self.add_self(handle.as_deref(), geid.as_deref());
        }
        self.changed(before)
    }

    fn changed(&self, before: (u64, usize, usize)) -> bool {
        before != (self.tick, self.self_handles.len(), self.self_ids.len())
    }

    fn try_toast(&mut self, parts: Vec<String>, t: Option<i64>) {
        let text = parts.join("\n");
        let Some(end) = toast_end(&text) else {
            if parts.len() < TOAST_MAX_LINES {
                self.toast = Some((parts, t));
            }
            return;
        };
        let body = end.trim_end_matches(|c: char| c == ':' || c.is_whitespace());
        // `text` is ours, not `self`'s: the hits can borrow it while `see` changes `self`.
        for (rel, name, source, ship, left, leader) in toast_hits(body) {
            self.see(rel, name, source, t, ship, left, leader);
        }
    }

    fn remember_id(&mut self, kid: &str, killer: &str) {
        if self.ids.insert(kid.to_owned(), killer.to_owned()).is_none() {
            self.id_order.push_back(kid.to_owned());
            while self.id_order.len() > ID_MAP_CAP {
                if let Some(old) = self.id_order.pop_front() {
                    self.ids.remove(&old);
                }
            }
        }
    }

    fn vehicle_destroyed(&mut self, body: &str, t: Option<i64>) {
        let Some(owner) = super::between(body, "(owner player [", "]") else {
            return;
        };
        let Some(rest) = body.split_once(" killer '").map(|x| x.1) else {
            return;
        };
        let Some((killer, after)) = rest.split_once('\'') else {
            return;
        };
        let kid = super::between(after, "[", "]").unwrap_or("");
        let (owner, killer, kid) = (owner.to_owned(), killer.to_owned(), kid.to_owned());
        if !killer.is_empty() && !is_npc(&killer) && !kid.is_empty() && kid != "0" {
            if self.is_self(Some(&killer), Some(&kid)) {
                self.self_ids.insert(kid);
            } else {
                self.remember_id(&kid, &killer);
                self.see("nearby", &killer, "vehicle_destroyed", t, None, None, false);
                if self.victims.remove(&kid) {
                    self.victim_order.retain(|v| *v != kid);
                }
            }
        }
        if !owner.is_empty() && owner != "0" && !self.self_ids.contains(&owner) {
            if let Some(h) = self.ids.get(&owner).cloned() {
                self.see("nearby", &h, "vehicle_destroyed", t, None, None, false);
            } else if self.victims.insert(owner.clone()) {
                self.victim_order.push_back(owner);
                while self.victim_order.len() > ID_MAP_CAP {
                    if let Some(old) = self.victim_order.pop_front() {
                        self.victims.remove(&old);
                    }
                }
            }
        }
    }

    pub fn view(&self) -> PeopleView {
        let mut c = PeopleCounts::default();
        let mut rows: Vec<&Entry> = self.people.values().collect();
        rows.sort_by(|a, b| {
            rank(a.relations[0])
                .cmp(&rank(b.relations[0]))
                .then_with(|| b.last.cmp(&a.last))
                .then_with(|| a.name.cmp(&b.name))
        });
        let people = rows
            .into_iter()
            .map(|p| {
                let rel = p.relations[0];
                match rel {
                    "crew" => c.crew += 1,
                    "party" => c.party += 1,
                    "party_likely" => c.party_likely += 1,
                    _ => c.nearby += 1,
                }
                Person {
                    name: p.name.clone(),
                    relation: rel.into(),
                    relations: p.relations.iter().map(|r| (*r).to_owned()).collect(),
                    label: label(rel).into(),
                    sources: p.sources.iter().map(|s| (*s).to_owned()).collect(),
                    first_ms: p.first,
                    last_ms: p.last,
                    count: p.count,
                    ship: p.ship.clone(),
                    left: p.left,
                    leader: p.leader,
                    valid_handle: valid_handle(&p.name),
                }
            })
            .collect::<Vec<_>>();
        c.total = people.len() as u32;
        PeopleView {
            counts: c,
            people,
            self_name: self.self_display.clone(),
        }
    }
}
