//! Ships and ground vehicles (spec 5.2): identity, resolved default loadout, flight
//! controller (`IFCSParams`), armour.

use super::Loc;
use super::json::{array, at, attach_def, component, num, referenced_name, text, vec3};
use serde_json::Value;
use starorg_core::model::ships::Thrusters;
use starorg_core::model::{
    Armor, Component, Flight, Hull, Loadout, LoadoutItem, Missile, Ship, ShipLoc, Totals, Weapon,
    is_placeholder_name, items::round_to,
};
use std::collections::{BTreeMap, HashMap};

/// What loadout resolution needs to know about a ship item.
#[derive(Debug, Clone, Default)]
pub struct ItemBrief {
    pub short: String,
    pub item_type: Option<String>,
    pub sub_type: Option<String>,
    pub size: Option<f64>,
}

/// Lookup tables built from the parsed ship items (keys: lower-case short names).
#[derive(Default)]
pub struct ShipContext<'a> {
    pub items: HashMap<String, ItemBrief>,
    pub components: HashMap<String, &'a Component>,
    pub weapons: HashMap<String, &'a Weapon>,
    pub missiles: HashMap<String, &'a Missile>,
    pub flight: HashMap<String, Flight>,
}

/// Flight envelope from a flight controller entity. Angular rates come from
/// `maxAngularVelocity` (x = pitch, y = roll, z = yaw); `speedProfile.angularVelocity` is
/// only read when the live block is missing.
pub fn parse_flight(short: &str, entity: &Value) -> Option<Flight> {
    let ifcs = component(entity, "IFCSParams")?;
    let angular = ifcs
        .get("maxAngularVelocity")
        .filter(|v| v.is_object())
        .or_else(|| at(ifcs, "speedProfile.angularVelocity"));
    let axis = |k: &str| angular.and_then(|a| num(a.get(k)));
    Some(Flight {
        controller: short.to_string(),
        scm_speed: num(ifcs.get("scmSpeed")),
        max_speed: num(ifcs.get("maxSpeed")),
        boost_speed_forward: num(ifcs.get("boostSpeedForward")),
        boost_speed_backward: num(ifcs.get("boostSpeedBackward")),
        pitch: axis("x"),
        roll: axis("y"),
        yaw: axis("z"),
    })
}

struct Entry<'v> {
    port: &'v str,
    item: &'v str,
    parent: Option<usize>,
}

/// Flatten `SEntityComponentDefaultLoadoutParams` recursively (parents before children).
fn loadout_entries(entity: &Value) -> Vec<Entry<'_>> {
    fn walk<'v>(lo: Option<&'v Value>, parent: Option<usize>, out: &mut Vec<Entry<'v>>) {
        for e in array(lo.and_then(|l| l.get("entries"))) {
            let item = text(e.get("entityClassName"))
                .or_else(|| referenced_name(e.get("entityClassReference")));
            let port = text(e.get("itemPortName"));
            let (Some(item), Some(port)) = (item, port) else {
                continue;
            };
            out.push(Entry { port, item, parent });
            let me = out.len() - 1;
            walk(e.get("loadout"), Some(me), out);
        }
    }
    let mut out = Vec::new();
    walk(
        component(entity, "SEntityComponentDefaultLoadoutParams").and_then(|c| c.get("loadout")),
        None,
        &mut out,
    );
    out
}

const GUN_TYPES: [&str; 5] = [
    "WeaponGun",
    "WeaponMining",
    "TractorBeam",
    "EMP",
    "WeaponDefensive",
];

/// Count identical siblings: equal items (ignoring `count`) merge into one with a count.
fn count_duplicates(items: Vec<LoadoutItem>) -> Vec<LoadoutItem> {
    let mut out: Vec<LoadoutItem> = Vec::new();
    for it in items {
        if let Some(twin) = out.iter_mut().find(|o| {
            let mut a = (*o).clone();
            a.count = None;
            a == it
        }) {
            twin.count = Some(twin.count() + 1.0);
        } else {
            out.push(it);
        }
    }
    out
}

fn sum(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let present: Vec<f64> = values.flatten().collect();
    (!present.is_empty()).then(|| round_to(present.iter().sum(), 2))
}

/// Resolve the default loadout into the categorised summary, totals, flight and armour.
pub fn parse_loadout(
    entity: &Value,
    cx: &ShipContext,
) -> (
    Option<Loadout>,
    Option<Totals>,
    Option<Flight>,
    Option<Armor>,
) {
    let entries = loadout_entries(entity);
    let mut lo = Loadout::default();
    let mut flight = None;
    let mut thrusters = Thrusters::default();
    let mut turrets = 0.0;
    let mut racks = 0.0;
    let (mut weapons, mut missiles, mut shields, mut power, mut coolers) =
        (vec![], vec![], vec![], vec![], vec![]);
    let (mut qd, mut qed, mut jd, mut radars, mut ls, mut cms, mut fuel, mut qfuel, mut cargo) = (
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    for e in &entries {
        let key = e.item.to_ascii_lowercase();
        let Some(brief) = cx.items.get(&key) else {
            continue;
        };
        // A gun under a gun is a stale AI duplicate (spec 5.2).
        if let Some(p) = e.parent
            && cx
                .items
                .get(&entries[p].item.to_ascii_lowercase())
                .and_then(|b| b.item_type.as_deref())
                .is_some_and(|t| GUN_TYPES.contains(&t))
        {
            continue;
        }
        let comp = cx.components.get(&key);
        let base = LoadoutItem {
            item: brief.short.clone(),
            size: comp
                .and_then(|c| c.item.size)
                .map(|s| s as f64)
                .or(brief.size),
            ..LoadoutItem::default()
        };
        let parent_item = e.parent.map(|p| entries[p].item.to_string());
        match brief.item_type.as_deref().unwrap_or("") {
            "Shield" => shields.push(base),
            "PowerPlant" => power.push(base),
            "Cooler" => coolers.push(base),
            "QuantumDrive" => qd.push(base),
            "QuantumInterdictionGenerator" => qed.push(base),
            "JumpDrive" => jd.push(base),
            "Radar" => radars.push(base),
            "Armor" => lo.armor = Some(base),
            "FlightController" => {
                if flight.is_none() {
                    flight = cx.flight.get(&key).cloned();
                }
            }
            "LifeSupportGenerator" => ls.push(base),
            "FuelTank" | "ExternalFuelTank" => fuel.push(base),
            "QuantumFuelTank" => qfuel.push(base),
            "CargoGrid" => cargo.push(base),
            t @ ("MainThruster" | "ManneuverThruster") => {
                let retro = brief.sub_type.as_deref() == Some("Retro")
                    || e.port.to_ascii_lowercase().contains("retro");
                let slot = if retro {
                    &mut thrusters.retro
                } else if t == "MainThruster" {
                    &mut thrusters.main
                } else {
                    &mut thrusters.maneuver
                };
                *slot = Some(slot.unwrap_or(0.0) + 1.0);
            }
            "Turret" | "TurretBase" | "UtilityTurret" => turrets += 1.0,
            "MissileLauncher" | "GroundVehicleMissileLauncher" | "BombLauncher" => racks += 1.0,
            "Missile" => {
                let m = cx.missiles.get(&key);
                missiles.push(LoadoutItem {
                    size: m.and_then(|m| m.item.size).map(|s| s as f64).or(base.size),
                    rack: parent_item,
                    damage: m.and_then(|m| m.damage).map(|d| d.total()),
                    ..base
                });
            }
            "WeaponDefensive" => {
                let ammo = comp.and_then(|c| c.detail.ammo.clone());
                let capacity = comp.and_then(|c| c.stats.get("maxAmmoCount").copied());
                cms.push(LoadoutItem {
                    ammo,
                    capacity,
                    ..base
                });
            }
            t if GUN_TYPES.contains(&t) => {
                let w = cx.weapons.get(&key);
                weapons.push(LoadoutItem {
                    size: w.and_then(|w| w.item.size).map(|s| s as f64).or(base.size),
                    item_type: Some(t.to_string()),
                    mount: parent_item,
                    dps: w.and_then(|w| w.dps),
                    ..base
                });
            }
            _ => {}
        }
    }
    lo.weapons = count_duplicates(weapons);
    lo.missiles = count_duplicates(missiles);
    lo.shields = count_duplicates(shields);
    lo.power_plants = count_duplicates(power);
    lo.coolers = count_duplicates(coolers);
    lo.quantum_drives = count_duplicates(qd);
    lo.quantum_enforcement = count_duplicates(qed);
    lo.jump_drives = count_duplicates(jd);
    lo.radars = count_duplicates(radars);
    lo.life_support = count_duplicates(ls);
    lo.countermeasures = count_duplicates(cms);
    lo.fuel_tanks = count_duplicates(fuel);
    lo.quantum_fuel_tanks = count_duplicates(qfuel);
    lo.cargo_grids = count_duplicates(cargo);
    if thrusters != Thrusters::default() {
        lo.thrusters = Some(thrusters);
    }
    lo.turrets = (turrets > 0.0).then_some(turrets);
    lo.missile_racks = (racks > 0.0).then_some(racks);
    if lo.is_empty() {
        return (None, None, flight, None);
    }
    let comp_of = |i: &LoadoutItem| cx.components.get(&i.item.to_ascii_lowercase());
    let stat = |items: &[LoadoutItem], field: &str| {
        sum(items.iter().map(|i| {
            comp_of(i)
                .and_then(|c| c.stats.get(field))
                .map(|v| v * i.count())
        }))
    };
    let count_of = |items: &[LoadoutItem]| {
        (!items.is_empty()).then(|| items.iter().map(LoadoutItem::count).sum::<f64>())
    };
    let armor_comp = lo.armor.as_ref().and_then(comp_of);
    let totals = Totals {
        weapon_dps: sum(lo.weapons.iter().map(|w| w.dps.map(|d| d * w.count()))),
        weapons: count_of(&lo.weapons),
        missiles: count_of(&lo.missiles),
        missile_damage: sum(lo.missiles.iter().map(|m| m.damage.map(|d| d * m.count()))),
        shield_health: stat(&lo.shields, "MaxShieldHealth"),
        shield_regen: stat(&lo.shields, "MaxShieldRegen"),
        armor_health: armor_comp.and_then(|c| c.health),
        power_output: stat(&lo.power_plants, "powerOutput"),
        fuel: stat(&lo.fuel_tanks, "standardCargoUnits"),
        quantum_fuel: stat(&lo.quantum_fuel_tanks, "standardCargoUnits"),
        cargo_scu: stat(&lo.cargo_grids, "scu"),
    };
    let armor = armor_comp.map(|c| Armor {
        item: c.item.short_name().to_string(),
        health: c.health,
        deflection: c.detail.deflection.clone(),
        damage_multiplier: c.detail.damage_multiplier.clone(),
    });
    (Some(lo), Some(totals), flight, armor)
}

fn hull_resistances(table: Option<&Value>) -> Option<BTreeMap<String, f64>> {
    let obj = table?.as_object()?;
    let out: BTreeMap<String, f64> = obj
        .iter()
        .filter_map(|(k, v)| {
            let kind = k.strip_suffix("Resistance")?;
            let m = num(v.get("Multiplier"))?;
            (m != 1.0).then(|| (kind.to_lowercase(), m))
        })
        .collect();
    (!out.is_empty()).then_some(out)
}

pub fn parse_ship(
    name: &str,
    id: &str,
    path: &str,
    entity: &Value,
    loc: &Loc,
    cx: &ShipContext,
) -> Option<Ship> {
    let p = component(entity, "VehicleComponentParams")?;
    let keys = ShipLoc {
        name: text(p.get("vehicleName")).map(str::to_string),
        desc: text(p.get("vehicleDescription")).map(str::to_string),
        career: text(p.get("vehicleCareer")).map(str::to_string),
        role: text(p.get("vehicleRole")).map(str::to_string),
    };
    let display = keys.name.as_deref().and_then(|k| loc.text(k));
    let (loadout, totals, flight, armor) = parse_loadout(entity, cx);
    let health = component(entity, "SHealthComponentParams");
    let hull = Hull {
        damage_normalization: num(p.get("vehicleHullDamageNormalizationValue")),
        resistances: hull_resistances(health.and_then(|h| h.get("DamageResistances"))),
    };
    Some(Ship {
        record_name: name.to_string(),
        id: id.to_string(),
        path: path.to_string(),
        loc: keys,
        manufacturer: referenced_name(p.get("manufacturer")).map(str::to_string),
        movement_class: text(p.get("movementClass")).map(str::to_string),
        crew_size: num(p.get("crewSize")),
        size: num(attach_def(entity).get("Size")),
        is_gravlev: p.get("isGravlevVehicle") == Some(&Value::Bool(true)),
        bounding_box: vec3(p.get("maxBoundingBoxSize")),
        hull: (hull != Hull::default()).then_some(hull),
        loadout,
        totals,
        flight,
        armor,
        is_template: is_placeholder_name(display),
    })
}
