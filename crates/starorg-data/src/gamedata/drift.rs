//! Schema-drift warnings (spec 3.3 step 6).
//!
//! A struct type the extraction reads that is missing from the DataCore, or a category or
//! stat that comes back empty, is a warning naming the stat it feeds. The stat is left
//! out; the build continues; the apply result carries the warning (exit 5).

use super::REQUIRED_TYPES;
use starorg_core::model::{GameData, WeaponClass};
use starorg_core::source::Warning;

pub const KIND: &str = "schema-drift";

fn warn(out: &mut Vec<Warning>, detail: String) {
    out.push(Warning::new(KIND, detail));
}

/// Component categories the generators of spec 5.3 write stat blocks for.
const COMPONENT_CATEGORIES: [(&str, &str); 6] = [
    ("shields", "MaxShieldHealth"),
    ("quantumDrives", "driveSpeed"),
    ("radars", "cooldownTime"),
    ("powerPlants", "powerOutput"),
    ("coolers", "coolantOutput"),
    ("missileRacks", "missileCount"),
];

/// Drift findings for an extracted `GameData`. `present` answers whether a DataCore struct
/// type exists (skipped when `None`).
pub fn check(gd: &GameData, present: Option<&dyn Fn(&str) -> bool>) -> Vec<Warning> {
    let mut out = Vec::new();
    if let Some(present) = present {
        for (ty, feeds) in REQUIRED_TYPES {
            if !present(ty) {
                warn(
                    &mut out,
                    format!("struct {ty} missing from DataCore; {feeds} left out"),
                );
            }
        }
    }
    let c = gd.counts();
    let empties = [
        (c.ammo, "ammo", "weapon damage, velocity and range"),
        (
            c.ship_weapons,
            "ship weapons",
            "ship weapon stat blocks and tags",
        ),
        (c.fps_weapons, "FPS weapons", "FPS weapon stat blocks"),
        (c.missiles, "missiles", "missile stat blocks and tags"),
        (c.ships, "ships", "ship stat blocks"),
        (c.contracts, "contracts", "mission tags and bodies"),
        (c.blueprints, "blueprints", "blueprint lists"),
        (
            c.blueprint_pools,
            "blueprint pools",
            "[BP] tags and blueprint lists",
        ),
        (
            c.reputation_standings,
            "reputation standings",
            "Requires: line",
        ),
    ];
    let empties = empties.into_iter().chain([
        (
            gd.item_descs.len(),
            "item description keys",
            "WHERE TO GET blocks on items",
        ),
        (
            gd.resource_descs.len(),
            "resource description keys",
            "WHERE TO GET blocks on commodities",
        ),
    ]);
    for (n, what, feeds) in empties {
        if n == 0 {
            warn(
                &mut out,
                format!("{what}: query returned nothing; {feeds} left out"),
            );
        }
    }
    for (cat, stat) in COMPONENT_CATEGORIES {
        let list: Vec<_> = gd.components.iter().filter(|x| x.category == cat).collect();
        if list.is_empty() {
            warn(
                &mut out,
                format!("{cat}: query returned nothing; stat blocks left out"),
            );
        } else if !list.iter().any(|x| x.stats.contains_key(stat)) {
            warn(
                &mut out,
                format!("{cat}: no record has {stat}; that line is left out"),
            );
        }
    }
    let player_ships: Vec<_> = gd.ships.iter().filter(|s| !s.is_template).collect();
    if !player_ships.is_empty() {
        let flights: Vec<_> = player_ships
            .iter()
            .filter_map(|s| s.flight.as_ref())
            .collect();
        if flights.is_empty() {
            warn(
                &mut out,
                "ships: no flight controller resolved; speed and Pitch/Yaw/Roll left out".into(),
            );
        } else if !flights
            .iter()
            .any(|f| f.pitch.is_some() && f.yaw.is_some() && f.roll.is_some())
        {
            warn(
                &mut out,
                "ships: IFCSParams.maxAngularVelocity empty; Pitch/Yaw/Roll left out".into(),
            );
        }
        if !player_ships
            .iter()
            .any(|s| s.loadout.as_ref().is_some_and(|l| !l.weapons.is_empty()))
        {
            warn(
                &mut out,
                "ships: no loadout resolved a gun; Guns line left out".into(),
            );
        }
        if !player_ships.iter().any(|s| s.loc.desc.is_some()) {
            warn(
                &mut out,
                "ships: no vehicleDescription key; ship stat blocks left out".into(),
            );
        }
    }
    let ship_guns: Vec<_> = gd
        .weapons
        .iter()
        .filter(|w| w.class == WeaponClass::Ship)
        .collect();
    if !ship_guns.is_empty() && !ship_guns.iter().any(|w| w.dps.is_some()) {
        warn(&mut out, "ship weapons: no DPS computed (fire actions or ammo link changed); Damage/DPS lines left out".into());
    }
    if c.contracts > 0 {
        if !gd.contracts.iter().any(|x| x.title_key.is_some()) {
            warn(
                &mut out,
                "contracts: no Title param found; title tags left out".into(),
            );
        }
        if !gd.contracts.iter().any(|x| !x.blueprint_rewards.is_empty()) {
            warn(
                &mut out,
                "contracts: no BlueprintRewards found; [BP] tags left out".into(),
            );
        }
        if !gd.contracts.iter().any(|x| !x.reputation.is_empty()) {
            warn(
                &mut out,
                "contracts: no reputation rewards resolved; [Rep] tags left out".into(),
            );
        }
    }
    if c.blueprints > 0 && !gd.blueprints.iter().any(|b| b.produces_name_key.is_some()) {
        warn(
            &mut out,
            "blueprints: no produced item name resolved; blueprint lists left out".into(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_gamedata_reports_every_category() {
        let w = check(&GameData::default(), Some(&|t: &str| t != "IFCSParams"));
        assert!(w.iter().all(|w| w.kind == KIND));
        let text: Vec<_> = w.iter().map(|w| w.detail.as_str()).collect();
        assert!(
            text.iter()
                .any(|t| t.starts_with("struct IFCSParams missing")),
            "{text:?}"
        );
        assert!(
            text.iter()
                .any(|t| t.starts_with("ships: query returned nothing"))
        );
        assert!(
            text.iter()
                .any(|t| t.starts_with("shields: query returned nothing"))
        );
        assert!(
            text.iter()
                .any(|t| t.starts_with("contracts: query returned nothing"))
        );
        assert!(
            text.iter()
                .any(|t| t.starts_with("resource description keys: query returned nothing"))
        );
    }
}
