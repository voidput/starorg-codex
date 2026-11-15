//! Ship description stat block (spec 5.2).

use super::format::{SEP, by_size, d1, d2, int, line, nz};
use super::items::damage_label;
use super::profile::Profile;
use crate::ini::StockIni;
use crate::model::{LoadoutItem, Ship, loc_key};

/// Record-name tokens of hulls players do not fly: AI and PU spawns, unmanned, derelict,
/// template and display variants (spec 5.2). Such records are skipped when they do not own
/// their loc key.
const NON_PLAYER: [&str; 14] = [
    "ai",
    "pu",
    "unmanned",
    "derelict",
    "template",
    "display",
    "shipboarded",
    "ea",
    "swarm",
    "temp",
    "tutorial",
    "nointerior",
    "wreck",
    "hijacked",
];

/// Item record-name tokens of non-main records: templates, test and balance copies,
/// holograms, props, AI and NPC variants (spec 5.1). Skipped when they do not own their key.
const NON_MAIN_ITEM: [&str; 11] = [
    "template",
    "test",
    "fakehologram",
    "hologram",
    "balance",
    "prop",
    "ai",
    "npc",
    "display",
    "dummy",
    "elim",
];

pub fn is_main_item(short_name: &str) -> bool {
    !short_name
        .split('_')
        .any(|t| NON_MAIN_ITEM.contains(&t.to_ascii_lowercase().as_str()))
}

pub fn is_player_hull(short_name: &str) -> bool {
    !short_name
        .split('_')
        .any(|t| NON_PLAYER.contains(&t.to_ascii_lowercase().as_str()))
}

fn resolve<'a>(stock: &'a StockIni, reference: Option<&str>) -> Option<&'a str> {
    let v = stock.value(loc_key(reference?)?)?.trim();
    (!v.is_empty() && !crate::model::is_placeholder_name(Some(v))).then_some(v)
}

fn sizes(items: &[LoadoutItem]) -> Option<String> {
    by_size(items.iter().map(|i| (i.size, i.count())))
}

/// Stat lines of a ship, `None` when there is nothing to show.
pub fn ship_lines(s: &Ship, stock: &StockIni, p: &Profile) -> Option<Vec<String>> {
    let on = |l: &str| p.line_on("ships", l);
    let mut out = Vec::new();
    let mut push = |name: &str, l: Option<String>| {
        if let Some(l) = l
            && on(name)
        {
            out.push(l);
        }
    };
    push(
        "role",
        line([
            resolve(stock, s.loc.role.as_deref()).map(|r| format!("Role: {r}")),
            resolve(stock, s.loc.career.as_deref()).map(|c| format!("Career: {c}")),
            nz(s.crew_size).map(|c| format!("Crew: {}", int(c))),
        ]),
    );
    // maxBoundingBoxSize: y is length, x is beam, z is height (M2a: the Gladius is
    // x 17.5, y 21, z 5.5 against a 20 m long, 17 m wide hull).
    push(
        "size",
        s.bounding_box.as_ref().and_then(|b| {
            let (l, w, h) = (
                nz(b.get("y").copied())?,
                nz(b.get("x").copied())?,
                nz(b.get("z").copied())?,
            );
            Some(format!("Size: {} × {} × {} m", d1(l), d1(w), d1(h)))
        }),
    );
    let f = s.flight.as_ref();
    push(
        "speed",
        line([
            nz(f.and_then(|f| f.scm_speed)).map(|v| format!("SCM: {} m/s", int(v))),
            nz(f.and_then(|f| f.max_speed)).map(|v| format!("Max: {} m/s", int(v))),
        ]),
    );
    push(
        "boost",
        line([
            nz(f.and_then(|f| f.boost_speed_forward)).map(|v| format!("{} m/s fwd", int(v))),
            nz(f.and_then(|f| f.boost_speed_backward)).map(|v| format!("{} m/s back", int(v))),
        ])
        .map(|s| format!("Boost: {s}")),
    );
    push(
        "rotation",
        line([
            nz(f.and_then(|f| f.pitch)).map(|v| format!("Pitch: {}°/s", int(v))),
            nz(f.and_then(|f| f.yaw)).map(|v| format!("Yaw: {}°/s", int(v))),
            nz(f.and_then(|f| f.roll)).map(|v| format!("Roll: {}°/s", int(v))),
        ]),
    );
    if let Some(lo) = &s.loadout {
        let guns: Vec<LoadoutItem> = lo
            .weapons
            .iter()
            .filter(|w| w.item_type.as_deref() == Some("WeaponGun"))
            .cloned()
            .collect();
        push(
            "weapons",
            line([
                sizes(&guns).map(|g| format!("Guns: {g}")),
                sizes(&lo.missiles).map(|m| format!("Missiles: {m}")),
            ]),
        );
        push(
            "components",
            line([
                sizes(&lo.shields).map(|v| format!("Shields: {v}")),
                sizes(&lo.power_plants).map(|v| format!("Power: {v}")),
                sizes(&lo.coolers).map(|v| format!("Coolers: {v}")),
                sizes(&lo.quantum_drives).map(|v| format!("QD: {v}")),
            ]),
        );
    }
    if let Some(a) = &s.armor {
        push(
            "armor",
            nz(a.health).map(|h| format!("Armor HP: {}", int(h))),
        );
        push(
            "damage_taken",
            a.damage_multiplier.as_ref().and_then(|m| {
                let parts: Vec<String> = ["physical", "energy", "distortion", "thermal"]
                    .iter()
                    .filter_map(|t| {
                        m.get(*t)
                            .map(|v| format!("{} {}x", damage_label(t), d2(*v)))
                    })
                    .collect();
                (!parts.is_empty()).then(|| format!("Damage taken: {}", parts.join(SEP)))
            }),
        );
    }
    let t = s.totals.as_ref();
    push(
        "cargo",
        line([
            nz(t.and_then(|t| t.cargo_scu)).map(|v| format!("Cargo: {} SCU", d1(v))),
            nz(t.and_then(|t| t.fuel)).map(|v| format!("H2 fuel: {} SCU", d1(v))),
            nz(t.and_then(|t| t.quantum_fuel)).map(|v| format!("QT fuel: {} SCU", d1(v))),
        ]),
    );
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_hulls() {
        assert!(is_player_hull("AEGS_Gladius"));
        assert!(is_player_hull("RSI_Constellation_Andromeda_GS"));
        assert!(!is_player_hull("AEGS_Gladius_PU_AI_CIV"));
        assert!(!is_player_hull("CRUS_Spirit_C1_Unmanned_Salvage"));
        assert!(!is_player_hull("ANVL_Valkyrie_AI_DefendShip"));
        assert!(is_player_hull("MISC_Prospector"));
        assert!(is_main_item("GRIN_TractorBeam_S1"));
        assert!(!is_main_item("WEP_TractorBeam_S1_template"));
        assert!(!is_main_item("Mining_Laser_GRIN_Arbor_S1_Test_Active_1"));
        assert!(!is_main_item(
            "GATS_BallisticGatling_Mounted_S1_fps_balance"
        ));
    }
}
