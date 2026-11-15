//! Headline numbers for the item finder's rows (addendum 7.17), from the build's GameData
//! (the extraction cache): ships show size, crew, cargo, SCM and DPS; weapons DPS, alpha,
//! range and speed; components their output and health; missiles damage and lock range.
//! Keyed by record GUID (lower case), so the app joins them onto the worker's list rows.

use super::Stat;
use crate::model::GameData;
use std::collections::HashMap;

fn stat(key: &str, label: &str, value: Option<f64>, unit: Option<&str>) -> Option<Stat> {
    let v = value.filter(|v| v.is_finite())?;
    Some(Stat {
        key: key.into(),
        label: label.into(),
        value: (v * 100.0).round() / 100.0,
        unit: unit.map(str::to_string),
    })
}

/// Component stats shown when present (GameData `stats` keys), in order.
const COMPONENT_STATS: &[(&str, &str, Option<&str>)] = &[
    ("powerOutput", "Output", None),
    ("coolantOutput", "Cooling", None),
    ("MaxShieldHealth", "Shield HP", Some("HP")),
    ("MaxShieldRegen", "Regen", Some("HP/s")),
    ("driveSpeed", "QT speed", Some("m/s")),
    ("scu", "Cargo", Some("SCU")),
    ("missileCount", "Missiles", None),
];

/// Headline stats by record GUID.
pub fn by_guid(gd: &GameData) -> HashMap<String, Vec<Stat>> {
    let mut out: HashMap<String, Vec<Stat>> = HashMap::new();
    let mut put = |id: &str, v: Vec<Option<Stat>>| {
        let v: Vec<Stat> = v.into_iter().flatten().collect();
        if !id.is_empty() && !v.is_empty() {
            out.insert(id.to_lowercase(), v);
        }
    };
    for s in &gd.ships {
        put(
            &s.id,
            vec![
                stat("size", "Size", s.size, None),
                stat("crew", "Crew", s.crew_size, None),
                stat(
                    "cargo",
                    "Cargo",
                    s.totals.as_ref().and_then(|t| t.cargo_scu),
                    Some("SCU"),
                ),
                stat(
                    "scm",
                    "SCM",
                    s.flight.as_ref().and_then(|f| f.scm_speed),
                    Some("m/s"),
                ),
                stat(
                    "dps",
                    "DPS",
                    s.totals.as_ref().and_then(|t| t.weapon_dps),
                    None,
                ),
            ],
        );
    }
    for w in &gd.weapons {
        put(
            &w.item.id,
            vec![
                stat("dps", "DPS", w.dps, None),
                stat("alpha", "Alpha", w.alpha, None),
                stat("range", "Range", w.range, Some("m")),
                stat("speed", "Speed", w.speed, Some("m/s")),
            ],
        );
    }
    for c in &gd.components {
        let mut v: Vec<Option<Stat>> = COMPONENT_STATS
            .iter()
            .map(|(k, l, u)| stat(k, l, c.stats.get(*k).copied(), *u))
            .collect();
        v.push(stat("power", "Power", c.power, None));
        v.push(stat("health", "Health", c.health, Some("HP")));
        put(&c.item.id, v);
    }
    for m in &gd.missiles {
        put(
            &m.item.id,
            vec![
                stat("damage", "Damage", m.damage_total, None),
                stat("lock", "Lock range", m.lock_range_max, Some("m")),
                stat("lockTime", "Lock time", m.lock_time, Some("s")),
            ],
        );
    }
    out
}
