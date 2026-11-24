//! Alerts over telemetry frames: low fuel, shields and faces down, hull, G, health, oxygen...
//! A port of sc-telemetry's `alerts.py` engine (same owner, MIT) and its default rules
//! (`data/alerts.default.json`, embedded). Pure and clock-injected: every entry point takes
//! `now` (seconds on any monotonic clock), so tests drive it with synthetic frames.
//!
//! - **Rules** ([`Rule`]): `threshold` (with `rearm` hysteresis), `transition` (a value enters
//!   a set: annunciators going `MAJOR_*`), `edge` (a boolean flips) and `composite`
//!   (`all`/`any` of conditions). Each has a severity, a scope (any, vehicle, pilot, on foot),
//!   a cooldown, a debounce (the condition must hold this long), an optional `when` guard and
//!   `supersedes` (a stronger alert silences its weaker sibling). sc-telemetry's `event` rules
//!   (Game.log signals) are not ported yet: the frames carry no events.
//! - **Never spam** ([`AlertEngine::evaluate`]): the first frame, the first [`SETTLE_S`] after
//!   the vehicle appears or changes, and the first frame after a gap of more than [`STALE_S`]
//!   are a baseline: a condition already true then is latched and fires only once it has
//!   cleared. A guarded rule takes the current state as its baseline while the guard is false.
//!   Missing data (a field the build lacks, NaN, the `FLT_MAX` sentinel) never fires.
//! - **A cooldown never drops an alert**: if the condition still holds when it ends, the alert
//!   fires then.
//! - **User overrides** ([`apply_overrides`]): per rule `enabled`, `cooldown_s`, and for a
//!   threshold rule `value` and `rearm`, kept in `config.toml`
//!   (`starorg_core::telemetry::AlertSettings`). A bad override is skipped with a warning; the
//!   rule keeps its default.
//!
//! Delivery (toasts, tones, speech) is the app's: this module only says what fired.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub use starorg_core::telemetry::{AlertSettings, Notifications, RuleOverride};

/// The default rules (`{"version": 1, "rules": [...]}`).
pub const DEFAULT_RULES: &str = include_str!("../data/alerts.default.json");

/// Seconds after the vehicle appears or changes during which every condition is a baseline
/// (the game fills a new vehicle in over about half a second).
pub const SETTLE_S: f64 = 1.5;

/// A gap between two frames longer than this makes the next one a baseline.
pub const STALE_S: f64 = 3.0;

/// Longest alert text, in characters.
pub const TEXT_MAX: usize = 160;

/// Longest cooldown an override may set, in seconds.
pub const COOLDOWN_MAX_S: f64 = 3600.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Critical,
}

/// Where a rule applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    #[default]
    Any,
    /// In a vehicle.
    Vehicle,
    /// In a vehicle as its pilot (`actor.state.crew_role` `PILOT`).
    Pilot,
    /// Not in a vehicle.
    OnFoot,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    /// Also matches an enum name by suffix: `DEPLOYED` matches
    /// `VEHICLE_COMPONENT_DEPLOYMENT_STATE_DEPLOYED`.
    #[default]
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
    /// The value equals one of a list.
    #[serde(rename = "in")]
    In,
}

impl Op {
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::In => "in",
        }
    }
}

/// `{path, op, value}`, `{all: [...]}`, `{any: [...]}` or `{not: c}`. A `path` is a dotted
/// frame path or a derived value: `$oxygen` (current / max), `$shield_faces_down` (faces at 0
/// that are not tuned away), `$quantum_seen` (quantum fuel has read above 0 on this vehicle).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cond {
    All {
        all: Vec<Cond>,
    },
    Any {
        any: Vec<Cond>,
    },
    Not {
        not: Box<Cond>,
    },
    Cmp {
        path: String,
        #[serde(default)]
        op: Op,
        value: Value,
    },
}

/// What makes a rule fire, by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// `path op value` becomes true; arms again once the value is past `rearm` (or, without
    /// one, once the condition is false).
    Threshold {
        path: String,
        op: Op,
        value: f64,
        #[serde(default)]
        rearm: Option<f64>,
    },
    /// The value enters `to` (optionally coming from a value in `from`); arms again once it
    /// leaves.
    Transition {
        path: String,
        to: Vec<String>,
        #[serde(default)]
        from: Option<Vec<String>>,
    },
    /// The boolean becomes `to`; arms again once it flips back.
    Edge { path: String, to: bool },
    /// Every condition of `all` and one of `any` hold.
    Composite {
        #[serde(default)]
        all: Vec<Cond>,
        #[serde(default)]
        any: Vec<Cond>,
    },
}

impl Trigger {
    pub fn kind(&self) -> &'static str {
        match self {
            Trigger::Threshold { .. } => "threshold",
            Trigger::Transition { .. } => "transition",
            Trigger::Edge { .. } => "edge",
            Trigger::Composite { .. } => "composite",
        }
    }
}

fn yes() -> bool {
    true
}

/// One alert rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub trigger: Trigger,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub severity: Severity,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub cooldown_s: f64,
    #[serde(default)]
    pub debounce_s: f64,
    /// Toast text: `{value}`, `{pct}` (0..100), `{pct_words}`, `{value:.0f}`.
    pub message: String,
    /// Short and speakable, or none.
    #[serde(default)]
    pub speech: Option<String>,
    /// `info`, `warn`, `critical` or none (no tone).
    #[serde(default)]
    pub sound: Option<String>,
    /// The data source was seen changing in a recording.
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub when: Option<Cond>,
    #[serde(default)]
    pub supersedes: Vec<String>,
    /// Stepping into the scope fires instead of being a baseline (stepping out into vacuum).
    #[serde(default)]
    pub on_entry: bool,
    /// How a threshold reads: `fraction` (0..1, shown as a percentage), `count` or `value`.
    #[serde(default)]
    pub unit: Option<String>,
}

/// One alert that fired.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub rule: String,
    pub severity: Severity,
    pub text: String,
    /// For speech (none when the rule has none, or the app has speech off).
    pub speech: Option<String>,
    /// Tone id (none when the rule has none, or the app plays no sound).
    pub sound: Option<String>,
    /// The threshold rule's value when it fired.
    pub value: Option<f64>,
    /// Unix time in ms of the frame that fired it.
    pub at_ms: u64,
    /// From the Test button, not from a frame.
    pub test: bool,
}

#[derive(Deserialize)]
struct RulesFile {
    rules: Vec<Rule>,
}

/// The default rules.
pub fn defaults() -> Vec<Rule> {
    serde_json::from_str::<RulesFile>(DEFAULT_RULES)
        .expect("data/alerts.default.json parses (tested)")
        .rules
}

fn finite(x: f64, what: &str) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= 1e6 {
        Ok(x)
    } else {
        Err(format!("{what} must be a number"))
    }
}

/// `r` with `o` applied, or why `o` does not fit the rule.
pub fn apply_override(r: &Rule, o: &RuleOverride) -> Result<Rule, String> {
    let mut n = r.clone();
    if let Some(e) = o.enabled {
        n.enabled = e;
    }
    if let Some(c) = o.cooldown_s {
        if !(c.is_finite() && (0.0..=COOLDOWN_MAX_S).contains(&c)) {
            return Err(format!(
                "the cooldown is 0 to {COOLDOWN_MAX_S} seconds, not {c}"
            ));
        }
        n.cooldown_s = c;
    }
    if o.value.is_none() && o.rearm.is_none() {
        return Ok(n);
    }
    let fraction = n.unit.as_deref() == Some("fraction");
    let Trigger::Threshold {
        op, value, rearm, ..
    } = &mut n.trigger
    else {
        return Err("only a threshold rule has a value to change".into());
    };
    let gap = rearm.map(|x| x - *value);
    if let Some(v) = o.value {
        *value = finite(v, "the threshold")?;
        // A new threshold alone keeps the default's hysteresis gap.
        if o.rearm.is_none() {
            *rearm = gap.map(|g| ((v + g) * 10_000.0).round() / 10_000.0);
        }
    }
    if let Some(x) = o.rearm {
        *rearm = Some(finite(x, "the re-arm point")?);
    }
    if fraction {
        let mut both = vec![*value];
        both.extend(*rearm);
        if both.iter().any(|x| !(0.0..=1.0).contains(x)) {
            return Err("a fraction is 0 to 1 (0 to 100 %)".into());
        }
    }
    if let Some(x) = *rearm {
        let ok = match op {
            Op::Lt | Op::Le => x >= *value,
            Op::Gt | Op::Ge => x <= *value,
            _ => true,
        };
        if !ok {
            return Err(format!(
                "the re-arm point {x} must be past the threshold {value} (it arms again on the \
                 way back)"
            ));
        }
    }
    Ok(n)
}

/// The rules with the user's overrides; each override that does not fit is skipped with a
/// warning (`<id>: <why>`), leaving that rule as it was.
pub fn apply_overrides(
    rules: &[Rule],
    overrides: &BTreeMap<String, RuleOverride>,
) -> (Vec<Rule>, Vec<String>) {
    let mut out = rules.to_vec();
    let mut warnings = Vec::new();
    for (id, o) in overrides {
        let Some(r) = out.iter_mut().find(|r| &r.id == id) else {
            warnings.push(format!("{id}: no such alert rule"));
            continue;
        };
        match apply_override(r, o) {
            Ok(n) => *r = n,
            Err(e) => warnings.push(format!("{id}: {e}")),
        }
    }
    (out, warnings)
}

// ---- values and conditions ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Val<'a> {
    Num(f64),
    Bool(bool),
    Str(&'a str),
}

/// A frame value as something comparable: numbers beyond the `FLT_MAX` sentinel are missing;
/// objects and lists are missing too.
fn clean(v: &Value) -> Option<Val<'_>> {
    match v {
        Value::Bool(b) => Some(Val::Bool(*b)),
        Value::Number(n) => n
            .as_f64()
            .filter(|x| x.is_finite() && x.abs() <= 1e30)
            .map(Val::Num),
        Value::String(s) => Some(Val::Str(s)),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct Derived {
    oxygen: Option<f64>,
    faces_down: Option<f64>,
    quantum_seen: bool,
}

fn get<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |v, k| v.as_object()?.get(k))
}

fn resolve<'a>(frame: &'a Value, d: &Derived, path: &str) -> Option<Val<'a>> {
    if let Some(name) = path.strip_prefix('$') {
        return match name {
            "oxygen" => d.oxygen.map(Val::Num),
            "shield_faces_down" => d.faces_down.map(Val::Num),
            "quantum_seen" => Some(Val::Bool(d.quantum_seen)),
            _ => None,
        };
    }
    clean(get(frame, path)?)
}

fn num(frame: &Value, path: &str) -> Option<f64> {
    match clean(get(frame, path)?)? {
        Val::Num(x) => Some(x),
        _ => None,
    }
}

/// `s` is `t`, or ends with `_t` (a proto enum name by its last part).
fn enum_str(s: &str, t: &str) -> bool {
    s == t || (s.len() > t.len() && s.ends_with(t) && s.as_bytes()[s.len() - t.len() - 1] == b'_')
}

fn enum_eq(a: Val<'_>, b: &Value) -> bool {
    match (a, b) {
        (Val::Bool(x), Value::Bool(y)) => x == *y,
        (Val::Num(x), Value::Number(n)) => n.as_f64() == Some(x),
        (Val::Str(s), Value::String(t)) => enum_str(s, t),
        _ => false,
    }
}

fn cmp_num(x: f64, op: Op, y: f64) -> Option<bool> {
    Some(match op {
        Op::Lt => x < y,
        Op::Le => x <= y,
        Op::Gt => x > y,
        Op::Ge => x >= y,
        Op::Eq => x == y,
        Op::Ne => x != y,
        Op::In => return None,
    })
}

/// `None` when the value is missing or the comparison does not apply to it.
fn compare(v: Option<Val<'_>>, op: Op, r: &Value) -> Option<bool> {
    let v = v?;
    match op {
        Op::Eq => Some(enum_eq(v, r)),
        Op::Ne => Some(!enum_eq(v, r)),
        Op::In => Some(r.as_array()?.iter().any(|x| enum_eq(v, x))),
        _ => match (v, r.as_f64()) {
            (Val::Num(x), Some(y)) => cmp_num(x, op, y),
            _ => None,
        },
    }
}

fn all_of(rs: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut missing = false;
    for r in rs {
        match r {
            Some(false) => return Some(false),
            None => missing = true,
            Some(true) => {}
        }
    }
    if missing { None } else { Some(true) }
}

fn any_of(rs: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut missing = false;
    for r in rs {
        match r {
            Some(true) => return Some(true),
            None => missing = true,
            Some(false) => {}
        }
    }
    if missing { None } else { Some(false) }
}

fn cond(c: &Cond, frame: &Value, d: &Derived) -> Option<bool> {
    match c {
        Cond::All { all } => all_of(all.iter().map(|x| cond(x, frame, d))),
        Cond::Any { any } => any_of(any.iter().map(|x| cond(x, frame, d))),
        Cond::Not { not } => cond(not, frame, d).map(|b| !b),
        Cond::Cmp { path, op, value } => compare(resolve(frame, d, path), *op, value),
    }
}

// ---- text ---------------------------------------------------------------------------------

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// 0..100 in English words (speakable without a number normaliser).
pub fn words(n: u32) -> String {
    let n = n.min(100) as usize;
    if n == 100 {
        return "one hundred".into();
    }
    if n < 20 {
        return ONES[n].into();
    }
    let (t, o) = (n / 10, n % 10);
    if o == 0 {
        TENS[t].into()
    } else {
        format!("{} {}", TENS[t], ONES[o])
    }
}

fn plain(x: f64) -> String {
    let s = format!("{:.4}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// Fill `{value}`, `{pct}`, `{pct_words}`, with `:.Nf` for a number of decimals. An unknown
/// field or a missing value reads `?`; anything that is not a placeholder stays as written.
pub fn render(template: &str, value: Option<f64>) -> String {
    let pct = value.map(|v| (v.clamp(0.0, 1.0) * 100.0).round() as u32);
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let inner = &after[..close];
        let (name, spec) = inner.split_once(':').unwrap_or((inner, ""));
        let decimals = spec
            .strip_prefix('.')
            .and_then(|s| s.strip_suffix('f'))
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|d| *d <= 4);
        let num = match name {
            "value" => value,
            "pct" => pct.map(f64::from),
            _ => None,
        };
        let text = match (name, num) {
            ("pct_words", _) => pct.map_or_else(|| "?".into(), words),
            ("value" | "pct", Some(x)) => match decimals {
                Some(d) => format!("{x:.d$}"),
                None => plain(x),
            },
            _ => "?".into(),
        };
        out.push_str(&text);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out.chars().take(TEXT_MAX).collect()
}

fn alert(r: &Rule, value: Option<f64>, at_ms: u64, test: bool) -> Alert {
    let text = render(&r.message, value);
    Alert {
        rule: r.id.clone(),
        severity: r.severity,
        text: if test { format!("TEST: {text}") } else { text },
        speech: r.speech.as_deref().map(|s| render(s, value)),
        sound: r.sound.clone(),
        value: value.map(|v| (v * 10_000.0).round() / 10_000.0),
        at_ms,
        test,
    }
}

// ---- engine -------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RuleState {
    armed: bool,
    /// When the firing condition started holding (debounce).
    pending: Option<f64>,
    last_fire: f64,
    need_base: bool,
    /// Transition: the last value outside the `to` set.
    prev_out: Option<String>,
    /// `None` until the first evaluated frame (startup is never a scope entry).
    in_scope: Option<bool>,
}

impl Default for RuleState {
    fn default() -> Self {
        Self {
            armed: false,
            pending: None,
            last_fire: f64::NEG_INFINITY,
            need_base: true,
            prev_out: None,
            in_scope: None,
        }
    }
}

/// Per-rule state machines over a stream of frames.
#[derive(Debug, Clone)]
pub struct AlertEngine {
    rules: Vec<Rule>,
    state: Vec<RuleState>,
    last_t: Option<f64>,
    vehicle_key: Option<String>,
    settle_until: f64,
    quantum_seen: bool,
    /// Each face's share at full charge (its tuning), for "face down".
    face_shares: Option<Vec<f64>>,
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self::new(defaults())
    }
}

fn scope_ok(scope: Scope, frame: &Value) -> bool {
    let has_v = get(frame, "vehicle.state")
        .and_then(Value::as_object)
        .is_some_and(|m| !m.is_empty());
    match scope {
        Scope::Any => true,
        Scope::Vehicle => has_v,
        Scope::Pilot => {
            has_v
                && matches!(
                    get(frame, "actor.state.crew_role").and_then(clean),
                    Some(Val::Str(s)) if enum_str(s, "PILOT")
                )
        }
        Scope::OnFoot => !has_v,
    }
}

/// -> (active, may arm again, value).
fn active(
    r: &Rule,
    st: &mut RuleState,
    frame: &Value,
    d: &Derived,
) -> (Option<bool>, bool, Option<f64>) {
    match &r.trigger {
        Trigger::Threshold {
            path,
            op,
            value,
            rearm,
        } => {
            let v = resolve(frame, d, path);
            let x = match v {
                Some(Val::Num(x)) => Some(x),
                _ => None,
            };
            let Some(a) = compare(v, *op, &Value::from(*value)) else {
                return (None, false, None);
            };
            match (rearm, x) {
                (None, _) => (Some(a), !a, x),
                (Some(rm), Some(x)) => {
                    let past = match op {
                        Op::Lt | Op::Le => x >= *rm,
                        _ => x <= *rm,
                    };
                    (Some(a), past && !a, Some(x))
                }
                (Some(_), None) => (Some(a), false, None),
            }
        }
        Trigger::Transition { path, to, from } => {
            let Some(v) = resolve(frame, d, path) else {
                return (None, false, None);
            };
            let s = match v {
                Val::Str(s) => s,
                _ => "",
            };
            if !to.iter().any(|t| enum_str(s, t)) || s.is_empty() {
                st.prev_out = Some(s.to_owned());
                return (Some(false), true, None);
            }
            let a = match from {
                None => true,
                Some(f) => st
                    .prev_out
                    .as_deref()
                    .is_some_and(|p| f.iter().any(|x| enum_str(p, x))),
            };
            (Some(a), !a, None)
        }
        Trigger::Edge { path, to } => match resolve(frame, d, path) {
            Some(Val::Bool(b)) => {
                let a = b == *to;
                (Some(a), !a, None)
            }
            _ => (None, false, None),
        },
        Trigger::Composite { all, any } => {
            if all.is_empty() && any.is_empty() {
                return (None, false, None);
            }
            let a = all_of(
                [
                    all_of(all.iter().map(|c| cond(c, frame, d))),
                    if any.is_empty() {
                        Some(true)
                    } else {
                        any_of(any.iter().map(|c| cond(c, frame, d)))
                    },
                ]
                .into_iter(),
            );
            (a, a == Some(false), None)
        }
    }
}

impl AlertEngine {
    pub fn new(rules: Vec<Rule>) -> Self {
        let state = vec![RuleState::default(); rules.len()];
        Self {
            rules,
            state,
            last_t: None,
            vehicle_key: None,
            settle_until: f64::NEG_INFINITY,
            quantum_seen: false,
            face_shares: None,
        }
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// New rules (the user changed one): every rule takes the next frame as its baseline
    /// instead of firing on the current value; a rule kept by id keeps its cooldown.
    pub fn set_rules(&mut self, rules: Vec<Rule>) {
        let mut old: BTreeMap<String, RuleState> = self
            .rules
            .iter()
            .map(|r| r.id.clone())
            .zip(std::mem::take(&mut self.state))
            .collect();
        self.state = rules
            .iter()
            .map(|r| {
                let mut s = old.remove(&r.id).unwrap_or_default();
                s.need_base = true;
                s.pending = None;
                s
            })
            .collect();
        self.rules = rules;
    }

    /// A new stream: everything starts over, as for the very first frame.
    pub fn reset(&mut self) {
        *self = Self::new(std::mem::take(&mut self.rules));
    }

    fn rebaseline(&mut self, now: f64) {
        self.settle_until = now + SETTLE_S;
        for s in &mut self.state {
            s.need_base = true;
            s.pending = None;
        }
    }

    fn derive(&mut self, frame: &Value) -> Derived {
        let mut d = Derived::default();
        if let Some(cur) = num(frame, "actor.vitals.oxygen.current") {
            d.oxygen = Some(match num(frame, "actor.vitals.oxygen.max") {
                Some(mx) if mx > 0.0 => cur / mx,
                _ => cur,
            });
        }
        if let Some(vs) = get(frame, "vehicle.state").filter(|v| v.is_object()) {
            if num(vs, "flight.instruments.quantum_fuel").is_some_and(|q| q > 0.0) {
                self.quantum_seen = true;
            }
            let faces: Option<Vec<f64>> = get(vs, "shields.faces")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
                .and_then(|a| {
                    a.iter()
                        .map(|x| match clean(x) {
                            Some(Val::Num(n)) => Some(n),
                            _ => None,
                        })
                        .collect()
                });
            if let (Some(lvl), Some(fv)) = (num(vs, "shields.level"), faces) {
                if lvl >= 0.99 {
                    // Full charge: each face shows its tuning share.
                    self.face_shares = Some(fv.clone());
                }
                let shares = self.face_shares.as_ref().filter(|s| s.len() == fv.len());
                let down = fv
                    .iter()
                    .enumerate()
                    .filter(|(i, x)| {
                        **x <= 0.01 && lvl > 0.05 && !shares.is_some_and(|s| s[*i] <= 0.05)
                    })
                    .count();
                d.faces_down = Some(down as f64);
            }
        }
        d.quantum_seen = self.quantum_seen;
        d
    }

    /// The alerts `frame` fires at `now` (seconds), stamped `at_ms`.
    pub fn evaluate(&mut self, frame: &Value, now: f64, at_ms: u64) -> Vec<Alert> {
        if !frame.is_object() {
            return Vec::new();
        }
        let vkey = get(frame, "vehicle.state")
            .and_then(Value::as_object)
            .filter(|m| !m.is_empty())
            .map(|m| {
                ["name", "class_name"]
                    .iter()
                    .find_map(|k| m.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
                    .unwrap_or("?")
                    .to_owned()
            });
        let changed = vkey.is_some() && vkey != self.vehicle_key;
        if self.last_t.is_none_or(|t| now - t > STALE_S) || changed {
            // The first frame, a resumed stream, or a vehicle that appeared or changed.
            self.rebaseline(now);
        }
        if changed {
            // Per-vehicle derived state starts over.
            self.vehicle_key = vkey;
            self.quantum_seen = false;
            self.face_shares = None;
        }
        self.last_t = Some(now);
        let d = self.derive(frame);
        let settling = now < self.settle_until;
        let mut out: Vec<(usize, Alert)> = Vec::new();
        for (i, (r, st)) in self.rules.iter().zip(self.state.iter_mut()).enumerate() {
            if !r.enabled {
                st.need_base = true;
                st.pending = None;
                continue;
            }
            let in_scope = scope_ok(r.scope, frame);
            let entered = in_scope && st.in_scope == Some(false);
            st.in_scope = Some(in_scope);
            if !in_scope {
                st.need_base = true;
                st.pending = None;
                continue;
            }
            let (a, rearm_ok, value) = active(r, st, frame, &d);
            let Some(a) = a else {
                st.pending = None;
                continue;
            };
            let guard = r.when.as_ref().map_or(Some(true), |w| cond(w, frame, &d));
            if guard != Some(true) {
                // Gated off: the current state is the baseline.
                st.armed = !a;
                st.pending = None;
                continue;
            }
            if st.need_base || settling {
                st.need_base = false;
                if entered && r.on_entry && !settling {
                    st.armed = true;
                } else {
                    st.armed = !a;
                    st.pending = None;
                    continue;
                }
            }
            if !st.armed {
                if rearm_ok {
                    st.armed = true;
                }
                continue;
            }
            if !a {
                st.pending = None;
                continue;
            }
            let since = *st.pending.get_or_insert(now);
            if now - since < r.debounce_s - 1e-6 {
                continue;
            }
            if now - st.last_fire < r.cooldown_s - 1e-6 {
                // Still armed and pending: fires once the cooldown ends if it still holds.
                continue;
            }
            st.armed = false;
            st.pending = None;
            st.last_fire = now;
            out.push((i, alert(r, value, at_ms, false)));
        }
        // A stronger alert (boost empty) silences its weaker sibling in the same frame.
        let sup: HashSet<&str> = out
            .iter()
            .flat_map(|(i, _)| self.rules[*i].supersedes.iter().map(String::as_str))
            .collect();
        if !sup.is_empty() {
            out.retain(|(_, a)| !sup.contains(a.rule.as_str()));
            for (r, st) in self.rules.iter().zip(self.state.iter_mut()) {
                if sup.contains(r.id.as_str()) && st.armed {
                    st.armed = false;
                    st.pending = None;
                }
            }
        }
        out.into_iter().map(|(_, a)| a).collect()
    }

    /// A test alert of rule `id` (its threshold as the value), whatever its state.
    pub fn test_alert(&self, id: &str, at_ms: u64) -> Option<Alert> {
        let r = self.rules.iter().find(|r| r.id == id)?;
        let v = match r.trigger {
            Trigger::Threshold { value, .. } => Some(value),
            _ => None,
        };
        Some(alert(r, v, at_ms, true))
    }
}

#[cfg(test)]
mod tests;
