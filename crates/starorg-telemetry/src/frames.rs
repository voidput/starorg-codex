//! What the UI gets per frame: a small, stable [`Snapshot`] of the headline panels (vitals,
//! ship speed, fuel, hull, shields, master mode, gear) plus the raw frame as JSON, at most
//! [`UI_HZ`] times a second (the game streams about 20 Hz).
//!
//! The snapshot reads the proto3 JSON frame by path (sc-telemetry `server.py` `SERIES`,
//! `_num` and `extract_sample`), so a field the build lacks is `None`, not an error. Numbers
//! are rounded to 4 decimals; a bool, a non-number, NaN or the `FLT_MAX` "no reference body"
//! sentinel (|v| > 1e30; api-spec §4) is `None`.

use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, Instant};

/// Most frames a second handed to the UI.
pub const UI_HZ: u32 = 10;

/// The interval matching [`UI_HZ`].
pub const UI_INTERVAL: Duration = Duration::from_millis(1000 / UI_HZ as u64);

/// One frame for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Frame {
    /// Counts frames received from the game on this connection (skipped ones included).
    pub seq: u64,
    /// Unix time in ms when the frame arrived.
    pub received_ms: u64,
    pub snapshot: Snapshot,
    /// The whole frame ([`crate::json::to_json`]).
    pub raw: Value,
}

/// Headline values. `ship` is `None` outside a vehicle.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Snapshot {
    pub vitals: Vitals,
    pub ship: Option<Ship>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Vitals {
    /// 0..100.
    pub health: Option<f64>,
    /// 0..1 (the other vitals use 0..100; api-spec §4).
    pub oxygen: Option<f64>,
    /// Beats per minute.
    pub heart_rate: Option<f64>,
    /// °C (`body_temperature.current`; its `max` is always 100 and not a limit).
    pub body_temperature: Option<f64>,
    /// G felt by the player.
    pub gravity: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Ship {
    pub name: Option<String>,
    pub class_name: Option<String>,
    /// m/s.
    pub speed: Option<f64>,
    /// 0..1.
    pub hydrogen_fuel: Option<f64>,
    /// 0..1.
    pub quantum_fuel: Option<f64>,
    /// Hull, 0..1 (`vehicle.state.health`).
    pub hull: Option<f64>,
    /// Shield level, 0..1.
    pub shield: Option<f64>,
    /// Per-face shield levels, 0..1 (layout by count; sc-telemetry `shield_faces.json`).
    pub shield_faces: Vec<f64>,
    /// Current G (`g_meter.current`; its `max` is the session peak, not a limit).
    pub g: Option<f64>,
    /// Boost reserves, 0..1.
    pub boost: Option<f64>,
    /// Vertical speed, m/s.
    pub vsi: Option<f64>,
    /// `VEHICLE_MASTER_MODE_*` without the prefix: `NAVIGATION`, `SCM`, ...
    pub master_mode: Option<String>,
    /// `VEHICLE_COMPONENT_DEPLOYMENT_STATE_*` without the prefix: `RETRACTED`, `DEPLOYED`, ...
    pub gear: Option<String>,
    pub lights: Option<bool>,
}

fn get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |v, k| v.as_object()?.get(*k))
}

/// server.py `_num`.
fn num(v: &Value, path: &[&str]) -> Option<f64> {
    let x = get(v, path)?.as_f64()?;
    if !x.is_finite() || x.abs() > 1e30 {
        return None;
    }
    Some((x * 10_000.0).round() / 10_000.0)
}

fn text(v: &Value, path: &[&str]) -> Option<String> {
    get(v, path)?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn enum_short(v: &Value, path: &[&str], prefix: &str) -> Option<String> {
    let s = text(v, path)?;
    Some(s.strip_prefix(prefix).map_or(s.clone(), str::to_owned))
}

impl Snapshot {
    /// From a frame as JSON ([`crate::json::to_json`]).
    pub fn from_json(f: &Value) -> Self {
        let vitals = Vitals {
            health: num(f, &["actor", "vitals", "health", "current"]),
            oxygen: num(f, &["actor", "vitals", "oxygen", "current"]),
            heart_rate: num(f, &["actor", "vitals", "heart_rate"]),
            body_temperature: num(f, &["actor", "vitals", "body_temperature", "current"]),
            gravity: num(f, &["actor", "vitals", "gravity"]),
        };
        let ship = get(f, &["vehicle", "state"])
            .filter(|v| v.is_object())
            .map(|vs| {
                let ins = |k: &str| num(vs, &["flight", "instruments", k]);
                Ship {
                    name: text(vs, &["name"]),
                    class_name: text(vs, &["class_name"]),
                    speed: ins("speed"),
                    hydrogen_fuel: ins("hydrogen_fuel"),
                    quantum_fuel: ins("quantum_fuel"),
                    hull: num(vs, &["health"]),
                    shield: num(vs, &["shields", "level"]),
                    shield_faces: get(vs, &["shields", "faces"])
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(|x| num(x, &[])).collect())
                        .unwrap_or_default(),
                    g: num(vs, &["flight", "instruments", "g_meter", "current"]),
                    boost: num(vs, &["flight", "instruments", "boost", "reserves"]),
                    vsi: ins("vertical_speed_indicator"),
                    master_mode: enum_short(vs, &["master_mode", "mode"], "VEHICLE_MASTER_MODE_"),
                    gear: enum_short(
                        vs,
                        &["flight", "state", "gear"],
                        "VEHICLE_COMPONENT_DEPLOYMENT_STATE_",
                    ),
                    lights: get(vs, &["lights_enabled"]).and_then(Value::as_bool),
                }
            });
        Self { vitals, ship }
    }
}

/// Lets a frame through at most once per interval. A frame that is held back is kept by the
/// caller and sent at [`Throttle::next_at`], so the latest state always reaches the UI.
#[derive(Debug, Clone)]
pub struct Throttle {
    interval: Duration,
    last: Option<Instant>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// Whether a frame may go out at `now`; if so, `now` counts as the last send.
    pub fn admit(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|l| now.saturating_duration_since(l) < self.interval)
        {
            return false;
        }
        self.last = Some(now);
        true
    }

    /// When the next frame may go out.
    pub fn next_at(&self, now: Instant) -> Instant {
        self.last.map_or(now, |l| (l + self.interval).max(now))
    }
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(UI_INTERVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{json, mock};
    use serde_json::json;

    #[test]
    fn snapshot_from_mock_frame() {
        let s = mock::schema();
        let state = mock::MockState {
            gear_deployed: true,
            ..Default::default()
        };
        let j = json::to_json(&mock::frame(&s, 20.0, &state));
        let snap = Snapshot::from_json(&j);
        assert!(
            snap.vitals
                .health
                .is_some_and(|h| (60.0..=100.0).contains(&h))
        );
        assert_eq!(snap.vitals.oxygen.map(|o| o <= 1.0), Some(true));
        let ship = snap.ship.unwrap();
        assert_eq!(ship.name.as_deref(), Some("Mock Gladius"));
        assert_eq!(ship.master_mode.as_deref(), Some("SCM"));
        assert_eq!(ship.gear.as_deref(), Some("DEPLOYED"));
        assert_eq!(ship.quantum_fuel, Some(0.82));
        assert_eq!(ship.shield_faces.len(), 4);
        assert_eq!(ship.lights, Some(true));
    }

    #[test]
    fn num_rules() {
        let f = json!({
            "actor": {"vitals": {"heart_rate": 60.123456, "gravity": true,
                                  "health": {"current": "x"}}},
            "vehicle": {"state": {"flight": {"instruments": {"speed": 3.4028235e38}}}}
        });
        let snap = Snapshot::from_json(&f);
        assert_eq!(snap.vitals.heart_rate, Some(60.1235));
        assert_eq!(snap.vitals.gravity, None);
        assert_eq!(snap.vitals.health, None);
        assert_eq!(snap.ship.unwrap().speed, None);
        assert_eq!(Snapshot::from_json(&json!({})), Snapshot::default());
    }

    #[test]
    fn throttle_ten_hz() {
        let mut t = Throttle::default();
        let t0 = Instant::now();
        assert!(t.admit(t0));
        assert!(!t.admit(t0 + Duration::from_millis(50)));
        assert_eq!(t.next_at(t0 + Duration::from_millis(50)), t0 + UI_INTERVAL);
        assert!(t.admit(t0 + UI_INTERVAL));
        // 20 Hz in for one second: at most 10 out.
        let mut t = Throttle::default();
        let out = (0..20)
            .filter(|i| t.admit(t0 + Duration::from_millis(50 * i)))
            .count();
        assert_eq!(out, 10);
    }
}
