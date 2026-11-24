//! The limited write path: `TriggerGameAction` (sc-telemetry `server.py` `/api/trigger` and
//! `docs/telemetry/api-spec.md` §3.2-3.3, §8.2).
//!
//! The game accepts only digital input actions from its own allowlist, which
//! `ListTriggerableActions` returns (and returns empty until triggers are enabled in the game
//! and the player has spawned). On top of that a [`TriggerGate`] refuses every press until
//! the user switches triggers on (separately from reading telemetry), any action the game did
//! not list, a hold on an action that is not holdable, a repeat inside [`DEBOUNCE`], and a
//! [`DANGEROUS`] action without an explicit confirmation.

use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Prefix of the game's `TriggerGameActionStatus` value names.
pub const STATUS_PREFIX: &str = "TRIGGER_GAME_ACTION_STATUS_";

/// Longest hold accepted, in ms (server.py: 1..10000).
pub const MAX_HOLD_MS: u32 = 10_000;

/// Gate-side debounce per action (server.py `TRIGGER_DEBOUNCE_S`; the game's own is under 1 s).
pub const DEBOUNCE: Duration = Duration::from_secs(1);

/// Irreversible or ship-losing actions: refused unless the press is confirmed (server.py
/// `DANGEROUS`). `v_power_toggle` powers the whole ship off when it is on; the thruster-off
/// pair kills flight control in flight.
pub const DANGEROUS: &[&str] = &[
    "v_self_destruct",
    "v_eject",
    "v_emergency_exit",
    "v_power_set_off",
    "v_power_toggle",
    "v_power_set_thrusters_off",
    "v_power_toggle_thrusters",
    "v_toggle_qdrive_engagement",
];

pub fn is_dangerous(name: &str) -> bool {
    DANGEROUS.contains(&name)
}

/// One entry of `ListTriggerableActions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerableAction {
    pub action_name: String,
    /// `other` when the game gave none.
    pub category: String,
    pub holdable: bool,
}

/// The `actions` list of a `ListTriggerableActions` response as JSON (proto field names, or
/// the camelCase a probe tool writes) to clean entries: no duplicates, no empty names, a
/// category always set (server.py `normalise_actions`).
pub fn normalise_actions(raw: &serde_json::Value) -> Vec<TriggerableAction> {
    let mut out: Vec<TriggerableAction> = Vec::new();
    for a in raw.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(a) = a.as_object() else { continue };
        let name = a.get("action_name").or_else(|| a.get("actionName"));
        let Some(name) = name.and_then(|n| n.as_str()).filter(|n| !n.is_empty()) else {
            continue;
        };
        if out.iter().any(|o| o.action_name == name) {
            continue;
        }
        let category = a
            .get("category")
            .and_then(|c| c.as_str())
            .filter(|c| !c.is_empty())
            .unwrap_or("other");
        out.push(TriggerableAction {
            action_name: name.to_owned(),
            category: category.to_owned(),
            holdable: a.get("holdable") == Some(&serde_json::Value::Bool(true)),
        });
    }
    out
}

/// `TriggerGameActionStatus` without its prefix. Serialized as the short name (`OK`,
/// `UNKNOWN_ACTION`, ...); a value this crate does not know keeps its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerStatus {
    Ok,
    UnknownAction,
    MapInactive,
    RateLimited,
    Failed,
    HoldNotAllowed,
    Filtered,
    Unspecified,
    Other(String),
}

impl TriggerStatus {
    /// From a value name, with or without [`STATUS_PREFIX`].
    pub fn from_name(name: &str) -> Self {
        match name.strip_prefix(STATUS_PREFIX).unwrap_or(name) {
            "OK" => Self::Ok,
            "UNKNOWN_ACTION" => Self::UnknownAction,
            "MAP_INACTIVE" => Self::MapInactive,
            "RATE_LIMITED" => Self::RateLimited,
            "FAILED" => Self::Failed,
            "HOLD_NOT_ALLOWED" => Self::HoldNotAllowed,
            "FILTERED" => Self::Filtered,
            "UNSPECIFIED" => Self::Unspecified,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Ok => "OK",
            Self::UnknownAction => "UNKNOWN_ACTION",
            Self::MapInactive => "MAP_INACTIVE",
            Self::RateLimited => "RATE_LIMITED",
            Self::Failed => "FAILED",
            Self::HoldNotAllowed => "HOLD_NOT_ALLOWED",
            Self::Filtered => "FILTERED",
            Self::Unspecified => "UNSPECIFIED",
            Self::Other(s) => s,
        }
    }
}

impl Serialize for TriggerStatus {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// What the game answered to one press.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerOutcome {
    pub status: TriggerStatus,
    pub message: String,
}

/// Why a press was refused before it reached the game.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum Refusal {
    #[error("game action triggers are off")]
    Disabled,
    #[error("action names are 1-64 characters of a-z, 0-9 and _")]
    InvalidName,
    #[error("a hold is 1-{MAX_HOLD_MS} ms")]
    InvalidHold,
    #[error("the game does not list this action as triggerable")]
    NotListed,
    #[error("this action cannot be held")]
    HoldNotAllowed,
    #[error("this action needs a confirmation")]
    NeedsConfirm,
    #[error("pressed too soon after the last press")]
    Debounced,
}

/// `^[a-z0-9_]{1,64}$` (server.py `ACTION_RE`).
pub fn valid_action_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Name and hold checks that need no allowlist.
pub fn validate(name: &str, hold_ms: Option<u32>) -> Result<(), Refusal> {
    if !valid_action_name(name) {
        return Err(Refusal::InvalidName);
    }
    if hold_ms.is_some_and(|h| !(1..=MAX_HOLD_MS).contains(&h)) {
        return Err(Refusal::InvalidHold);
    }
    Ok(())
}

/// The app-side guard in front of `TriggerGameAction`. Starts off; holds the last action list
/// from the game and the time of each action's last accepted press.
#[derive(Debug, Clone, Default)]
pub struct TriggerGate {
    enabled: bool,
    actions: HashMap<String, TriggerableAction>,
    last: HashMap<String, Instant>,
}

impl TriggerGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The user's switch. Turning it off also forgets the action list.
    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.actions.clear();
        }
    }

    /// The game's latest `ListTriggerableActions` answer. An empty answer keeps the previous
    /// list (the game answers empty until the player has spawned; server.py keeps the last
    /// non-empty list the same way).
    pub fn set_actions(&mut self, actions: &[TriggerableAction]) {
        if actions.is_empty() {
            return;
        }
        self.actions = actions
            .iter()
            .map(|a| (a.action_name.clone(), a.clone()))
            .collect();
    }

    /// Forget the action list (a new stream, or none): the next game's answer fills it.
    pub fn clear_actions(&mut self) {
        self.actions.clear();
    }

    pub fn actions(&self) -> Vec<&TriggerableAction> {
        let mut v: Vec<_> = self.actions.values().collect();
        v.sort_by(|a, b| a.action_name.cmp(&b.action_name));
        v
    }

    /// Check one press; on success it counts as pressed at `now` (for the debounce).
    pub fn check(
        &mut self,
        name: &str,
        hold_ms: Option<u32>,
        confirmed: bool,
        now: Instant,
    ) -> Result<(), Refusal> {
        if !self.enabled {
            return Err(Refusal::Disabled);
        }
        validate(name, hold_ms)?;
        let a = self.actions.get(name).ok_or(Refusal::NotListed)?;
        if hold_ms.is_some() && !a.holdable {
            return Err(Refusal::HoldNotAllowed);
        }
        if is_dangerous(name) && !confirmed {
            return Err(Refusal::NeedsConfirm);
        }
        if self
            .last
            .get(name)
            .is_some_and(|t| now.saturating_duration_since(*t) < DEBOUNCE)
        {
            return Err(Refusal::Debounced);
        }
        self.last.insert(name.to_owned(), now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalise_like_server_py() {
        let raw = json!([
            {"action_name": "v_lights", "category": "lights_controller"},
            {"actionName": "v_invoke_ping", "category": "spaceship_radar", "holdable": true},
            {"action_name": "v_lights", "category": "dup"},
            {"action_name": "", "category": "x"},
            {"action_name": 5},
            "junk",
            {"action_name": "emote_wave", "category": "", "holdable": "yes"},
        ]);
        let got = normalise_actions(&raw);
        let names: Vec<(&str, &str, bool)> = got
            .iter()
            .map(|a| (a.action_name.as_str(), a.category.as_str(), a.holdable))
            .collect();
        assert_eq!(
            names,
            [
                ("v_lights", "lights_controller", false),
                ("v_invoke_ping", "spaceship_radar", true),
                ("emote_wave", "other", false),
            ]
        );
        assert!(normalise_actions(&json!({"actions": []})).is_empty());
        assert!(normalise_actions(&serde_json::Value::Null).is_empty());
    }

    #[test]
    fn status_names() {
        assert_eq!(
            TriggerStatus::from_name("TRIGGER_GAME_ACTION_STATUS_OK"),
            TriggerStatus::Ok
        );
        assert_eq!(
            TriggerStatus::from_name("MAP_INACTIVE"),
            TriggerStatus::MapInactive
        );
        let other = TriggerStatus::from_name("TRIGGER_GAME_ACTION_STATUS_NEW_THING");
        assert_eq!(other.as_str(), "NEW_THING");
        assert_eq!(serde_json::to_value(&other).unwrap(), json!("NEW_THING"));
    }

    #[test]
    fn gate_refuses_until_enabled_and_listed() {
        let mut g = TriggerGate::new();
        let t = Instant::now();
        assert_eq!(g.check("v_lights", None, false, t), Err(Refusal::Disabled));
        g.set_enabled(true);
        assert_eq!(g.check("v_lights", None, false, t), Err(Refusal::NotListed));
        g.set_actions(&crate::mock::actions());
        g.set_actions(&[]);
        assert_eq!(g.check("v_lights", None, false, t), Ok(()));
        assert_eq!(g.check("v_lights", None, false, t), Err(Refusal::Debounced));
        assert_eq!(g.check("v_lights", None, false, t + DEBOUNCE), Ok(()));
        assert_eq!(
            g.check("v_lights_on", Some(100), false, t),
            Err(Refusal::HoldNotAllowed)
        );
        assert_eq!(g.check("v_invoke_ping", Some(1500), false, t), Ok(()));
        assert_eq!(
            g.check("v_invoke_ping", Some(0), false, t),
            Err(Refusal::InvalidHold)
        );
        assert_eq!(
            g.check("v_self_destruct", None, false, t),
            Err(Refusal::NeedsConfirm)
        );
        assert_eq!(g.check("v_self_destruct", None, true, t), Ok(()));
        assert_eq!(
            g.check("V-Lights", None, false, t),
            Err(Refusal::InvalidName)
        );
        g.set_enabled(false);
        assert_eq!(
            g.check("v_lights_off", None, false, t),
            Err(Refusal::Disabled)
        );
        g.set_enabled(true);
        assert_eq!(
            g.check("v_lights_off", None, false, t),
            Err(Refusal::NotListed)
        );
    }
}
