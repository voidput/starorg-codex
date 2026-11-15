//! Ships and ground vehicles (spec 5.2), shaped like `game-ships.json` of the dumps.
//!
//! The default loadout is resolved at extraction time into the categorised summary below
//! (items sorted by their `AttachDef.Type`, identical siblings counted), which is exactly
//! what the ship stat block needs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Loc key references of a vehicle (local only).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipLoc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `VehicleComponentParams.vehicleDescription`: the key the ship generator writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub career: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

impl ShipLoc {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ship {
    pub record_name: String,
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "ShipLoc::is_empty")]
    pub loc: ShipLoc,
    /// Lower-case manufacturer record short name (`aegs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crew_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_gravlev: bool,
    /// `maxBoundingBoxSize`, metres, keys `x`, `y`, `z` (axis order confirmed in M2a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounding_box: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hull: Option<Hull>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loadout: Option<Loadout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totals: Option<Totals>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight: Option<Flight>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor: Option<Armor>,
    /// Placeholder display name: a template, AI or test hull.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_template: bool,
}

impl Ship {
    pub fn short_name(&self) -> &str {
        super::short_name(&self.record_name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hull {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_normalization: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resistances: Option<BTreeMap<String, f64>>,
}

/// One kind of item in the stock loadout; identical items in sibling ports are counted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadoutItem {
    /// Record short name of the item.
    pub item: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    /// Left out when it is 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<f64>,
    /// Weapons: `AttachDef.Type`.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    /// Weapons: the mount or turret the gun sits in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dps: Option<f64>,
    /// Missiles: the rack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rack: Option<String>,
    /// Missiles: damage per missile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage: Option<f64>,
    /// Countermeasures: ammo record and capacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ammo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<f64>,
}

impl LoadoutItem {
    pub fn count(&self) -> f64 {
        self.count.unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thrusters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retro: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maneuver: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loadout {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapons: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missiles: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shields: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub power_plants: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coolers: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantum_drives: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantum_enforcement: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jump_drives: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radars: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor: Option<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub life_support: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub countermeasures: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fuel_tanks: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantum_fuel_tanks: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_grids: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thrusters: Option<Thrusters>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turrets: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missile_racks: Option<f64>,
}

impl Loadout {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_dps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapons: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missiles: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missile_damage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shield_health: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shield_regen: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor_health: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_output: Option<f64>,
    /// Hydrogen fuel, SCU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantum_fuel: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cargo_scu: Option<f64>,
}

/// Flight envelope from the stock flight controller's `IFCSParams` (spec 5.2).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flight {
    /// Item record short name of the flight controller.
    pub controller: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scm_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boost_speed_forward: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boost_speed_backward: Option<f64>,
    /// Degrees per second, from `maxAngularVelocity`: **x = pitch, y = roll, z = yaw**.
    /// Never `speedProfile.angularVelocity` (a 60/200/60 placeholder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll: Option<f64>,
}

/// The stock armour: health and damage multipliers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Armor {
    pub item: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deflection: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_multiplier: Option<BTreeMap<String, f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_ship_deserialises() {
        let j = r#"{"name":"Aegis Gladius","recordName":"EntityClassDefinition.AEGS_Gladius","id":"x",
          "manufacturer":"aegs","career":"Combat","crewSize":1,"size":2,"boundingBox":{"x":17.5,"y":21,"z":5.5},
          "loadout":{"weapons":[{"item":"KLWE_LaserRepeater_S3","name":"n","size":3,"type":"WeaponGun","count":2}],
            "armor":{"item":"ARMR_AEGS_Gladius","size":1},"thrusters":{"main":2},"turrets":3},
          "flight":{"controller":"Controller_Flight_AEGS_Gladius","pitch":68,"yaw":52,"roll":200,"stunThresholdGs":50},
          "armor":{"item":"ARMR_AEGS_Gladius","health":3300,"damageMultiplier":{"physical":0.75}}}"#;
        let s: Ship = serde_json::from_str(j).unwrap();
        assert_eq!(s.short_name(), "AEGS_Gladius");
        let l = s.loadout.unwrap();
        assert_eq!(l.weapons[0].count(), 2.0);
        assert_eq!(l.turrets, Some(3.0));
        let f = s.flight.unwrap();
        assert_eq!(
            (f.pitch, f.yaw, f.roll),
            (Some(68.0), Some(52.0), Some(200.0))
        );
        assert_eq!(s.armor.unwrap().health, Some(3300.0));
        assert!(s.loc.is_empty());
    }
}
