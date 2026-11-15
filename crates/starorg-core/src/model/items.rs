//! Items: ship components, weapons, missiles and ammo (spec 5.3 to 5.6).
//!
//! Field names follow `game-ship-components.json`, `game-weapons.json` and `game-ammo.json`
//! of the published dumps. Derived numbers (alpha, DPS, sustained burst rate, range) are
//! computed by the functions in this module, by both sources alike.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Round to `places` decimals, half away from zero (as the dumps do).
pub fn round_to(value: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (value * f).round() / f
}

/// Loc key references of a record, as written in DataCore (with `@`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocKeys {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
}

impl LocKeys {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.short_name.is_none() && self.desc.is_none()
    }
}

/// What every item record shares: identity and `AttachDef`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemIdentity {
    /// Full record name, e.g. `EntityClassDefinition.GATS_BallisticGatling_S3`.
    pub record_name: String,
    /// Record GUID.
    #[serde(default)]
    pub id: String,
    /// DataForge path below `libs/foundry/records/`, lower case, no extension (local only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// Loc key references (local only; the dumps carry resolved names instead).
    #[serde(default, skip_serializing_if = "LocKeys::is_empty")]
    pub loc: LocKeys,
    /// True when the record has no real display name (template, stub, prop).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_template: bool,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<i64>,
    /// Manufacturer code, upper case (`AEGS`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
}

impl ItemIdentity {
    pub fn short_name(&self) -> &str {
        super::short_name(&self.record_name)
    }
}

/// Damage by type. Zero components are left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Damage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distortion: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biochemical: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stun: Option<f64>,
}

/// The game's damage types, in the order its positional arrays use.
pub const DAMAGE_TYPES: [&str; 6] = [
    "physical",
    "energy",
    "distortion",
    "thermal",
    "biochemical",
    "stun",
];

impl Damage {
    /// `(type, amount)` for the non-zero components, in [`DAMAGE_TYPES`] order.
    pub fn parts(&self) -> Vec<(&'static str, f64)> {
        [
            self.physical,
            self.energy,
            self.distortion,
            self.thermal,
            self.biochemical,
            self.stun,
        ]
        .into_iter()
        .zip(DAMAGE_TYPES)
        .filter_map(|(v, t)| v.filter(|v| *v != 0.0).map(|v| (t, v)))
        .collect()
    }

    pub fn set(&mut self, kind: &str, value: f64) {
        let slot = match kind {
            "physical" => &mut self.physical,
            "energy" => &mut self.energy,
            "distortion" => &mut self.distortion,
            "thermal" => &mut self.thermal,
            "biochemical" => &mut self.biochemical,
            "stun" => &mut self.stun,
            _ => return,
        };
        *slot = Some(value);
    }

    pub fn is_empty(&self) -> bool {
        self.parts().is_empty()
    }

    /// Sum of all components, rounded to two decimals.
    pub fn total(&self) -> f64 {
        round_to(self.parts().iter().map(|(_, v)| v).sum(), 2)
    }

    /// The largest component; on a tie the first in [`DAMAGE_TYPES`] order.
    pub fn dominant(&self) -> Option<&'static str> {
        let mut best: Option<(&'static str, f64)> = None;
        for (t, v) in self.parts() {
            if best.is_none_or(|(_, b)| v > b) {
                best = Some((t, v));
            }
        }
        best.map(|(t, _)| t)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinMax {
    pub min: f64,
    pub max: f64,
}

/// Radar sensitivity entry (`signatureDetection[]`), positional.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureDetection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piercing: Option<f64>,
    #[serde(default)]
    pub passive: bool,
    #[serde(default)]
    pub active: bool,
}

/// Category-specific facts that are not plain numbers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDetail {
    /// Shield resistance min/max by damage type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resistance: Option<BTreeMap<String, MinMax>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absorption: Option<BTreeMap<String, MinMax>>,
    /// Radar detection, positional (labels are confirmed in M2a, spec 5.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_detection: Option<Vec<SignatureDetection>>,
    /// Armor damage multipliers by damage type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_multiplier: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deflection: Option<BTreeMap<String, f64>>,
    /// Countermeasure ammo record short name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ammo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thruster_type: Option<String>,
    /// Cargo grid inventory container short name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

impl ComponentDetail {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A ship item in one of the categories of `game-ship-components.json`.
///
/// `category` is one of `shields`, `quantumDrives`, `radars`, `countermeasures`,
/// `weapons`, `missiles`, `powerPlants`, `coolers`, `lifeSupport`, `thrusters`,
/// `quantumFuelTanks`, `fuelTanks`, `armor`, `missileRacks`, `cargoGrids`, `jumpDrives`,
/// `quantumEnforcement`, `emp`, `selfDestruct`, `batteries`, `mounts`, `flightControllers`.
/// `stats` holds raw DataCore numbers under their DataCore field names
/// (`MaxShieldHealth`, `driveSpeed`, `powerOutput`, ...): stable names for data patches.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    #[serde(flatten)]
    pub item: ItemIdentity,
    pub category: String,
    /// Power drawn per second while online.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub em: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distortion_max: Option<f64>,
    #[serde(default)]
    pub stats: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "ComponentDetail::is_empty")]
    pub detail: ComponentDetail,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeaponClass {
    #[default]
    Ship,
    Fps,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FireKind {
    Single,
    Rapid,
    Burst,
    Charged,
    Beam,
    Sequence,
    #[default]
    Other,
}

/// One fire action of a weapon (spec 5.4 fire-rate table).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FireMode {
    pub name: String,
    pub kind: FireKind,
    /// Rounds per minute. For a sequence, the rate computed from its entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire_rate: Option<f64>,
    /// Burst only: `60·n / (n·60/fireRate + cooldown)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sustained_fire_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shot_count: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pellets: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_multiplier: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat_per_shot: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charge_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charged_damage_multiplier: Option<f64>,
    /// Damage per trigger pull: ammo damage × pellets × multiplier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dps: Option<f64>,
    /// Beam only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_damage_range: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zero_damage_range: Option<f64>,
    /// Beam only: damage per second by type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beam_damage: Option<Damage>,
}

impl FireMode {
    /// The rate DPS is computed from: the sustained rate for bursts, else `fire_rate`.
    pub fn effective_rate(&self) -> Option<f64> {
        self.sustained_fire_rate.or(self.fire_rate)
    }

    /// Fill `alpha` and `dps` from the ammo's per-shot damage (not for beams, whose DPS is
    /// the sum of `damagePerSecond`).
    pub fn finish(&mut self, per_shot: Option<f64>) {
        if self.kind == FireKind::Beam {
            self.dps = self
                .beam_damage
                .filter(|d| !d.is_empty())
                .map(|d| d.total());
            return;
        }
        let Some(per_shot) = per_shot else { return };
        let alpha = per_shot * self.pellets.unwrap_or(1.0) * self.damage_multiplier.unwrap_or(1.0);
        self.alpha = Some(round_to(alpha, 2));
        if let Some(rate) = self.effective_rate().filter(|r| *r != 0.0) {
            self.dps = Some(round_to(alpha * rate / 60.0, 2));
        }
    }

    /// Mode label for stat blocks: `Semi`, `Auto`, `Burst n`, `Charged`, `Beam`.
    pub fn label(&self) -> String {
        match self.kind {
            FireKind::Single => "Semi".into(),
            FireKind::Rapid => "Auto".into(),
            FireKind::Burst => match self.shot_count {
                Some(n) => format!("Burst {n}"),
                None => "Burst".into(),
            },
            FireKind::Charged => "Charged".into(),
            FireKind::Beam => "Beam".into(),
            FireKind::Sequence | FireKind::Other => self.name.clone(),
        }
    }
}

/// Sustained rate of a burst: `60·n / (n·60/fireRate + cooldown)` (spec 5.4).
pub fn burst_sustained_rate(fire_rate: f64, shot_count: f64, cooldown: f64) -> Option<f64> {
    if fire_rate <= 0.0 || shot_count <= 0.0 {
        return None;
    }
    let seconds = shot_count * 60.0 / fire_rate + cooldown;
    Some(round_to(60.0 * shot_count / seconds, 2))
}

/// One entry of `SWeaponActionSequenceParams.sequenceEntries`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SequenceEntry {
    pub delay: f64,
    /// `delay` is in seconds (true) or is a rate per minute (false).
    pub delay_in_seconds: bool,
    pub repetitions: f64,
}

/// Rate of a sequence: `60·Σrep / Σ(perShot·rep)`, where `perShot` is `delay` in seconds
/// or `60/delay` (spec 5.4). Entries with no positive delay are skipped.
pub fn sequence_fire_rate(entries: &[SequenceEntry]) -> Option<f64> {
    let mut shots = 0.0;
    let mut seconds = 0.0;
    for e in entries {
        if e.delay <= 0.0 {
            continue;
        }
        let per_shot = if e.delay_in_seconds {
            e.delay
        } else {
            60.0 / e.delay
        };
        shots += e.repetitions;
        seconds += per_shot * e.repetitions;
    }
    (shots > 0.0 && seconds > 0.0).then(|| round_to(60.0 * shots / seconds, 2))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capacitor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shots: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regen_per_second: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regen_cooldown: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_per_shot: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_regen_per_second: Option<f64>,
}

/// Damage falloff for one damage type (`damageDropParams`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Falloff {
    pub min_distance: f64,
    pub per_meter: f64,
    pub floor: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ammo {
    /// Record short name.
    #[serde(default)]
    pub name: String,
    pub record_name: String,
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime: Option<f64>,
    /// `speed × lifetime`, rounded to whole metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage: Option<Damage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_total: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub falloff: Option<BTreeMap<String, Falloff>>,
}

impl Ammo {
    /// Fill `range` and `damage_total` from the raw fields.
    pub fn derive(&mut self) {
        self.range = match (self.speed, self.lifetime) {
            (Some(s), Some(l)) if s != 0.0 && l != 0.0 => Some((s * l).round()),
            _ => None,
        };
        self.damage_total = self.damage.filter(|d| !d.is_empty()).map(|d| d.total());
    }
}

/// The ammo a weapon fires, joined in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponAmmo {
    pub record_name: String,
    /// FPS: the default magazine entity the ammo came through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magazine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage: Option<Damage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_per_shot: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub falloff: Option<BTreeMap<String, Falloff>>,
    /// Magazine `maxAmmoCount`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Weapon {
    #[serde(flatten)]
    pub item: ItemIdentity,
    pub class: WeaponClass,
    /// Dominant damage type of the ammo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_type: Option<String>,
    /// Headline mode (highest DPS) numbers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub em: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir: Option<f64>,
    /// Magazine size, or the capacitor's shots for capacitor weapons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ammo_capacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat_per_shot: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overheat_temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacitor: Option<Capacitor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ammo: Option<WeaponAmmo>,
    #[serde(default)]
    pub modes: Vec<FireMode>,
    /// Mining laser modifiers by DataCore field name (local only, spec 5.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mining_modifiers: Option<BTreeMap<String, f64>>,
}

impl Weapon {
    /// The mode with the highest DPS, else the first mode.
    pub fn headline(&self) -> Option<&FireMode> {
        let mut best: Option<&FireMode> = None;
        for m in &self.modes {
            if let Some(d) = m.dps
                && best.and_then(|b| b.dps).is_none_or(|b| b < d)
            {
                best = Some(m);
            }
        }
        best.or(self.modes.first())
    }

    /// Fill the headline numbers from `modes` and `ammo`.
    pub fn derive_headline(&mut self) {
        let head = self.headline().cloned();
        self.dps = head.as_ref().and_then(|m| m.dps);
        self.alpha = head.as_ref().and_then(|m| m.alpha);
        self.fire_rate = head.as_ref().and_then(|m| m.effective_rate());
        self.heat_per_shot = head.as_ref().and_then(|m| m.heat_per_shot);
        self.damage_type = self
            .ammo
            .as_ref()
            .and_then(|a| a.damage)
            .and_then(|d| d.dominant())
            .map(str::to_string);
        self.range = self
            .ammo
            .as_ref()
            .and_then(|a| a.range)
            .or(head.as_ref().and_then(|m| m.zero_damage_range));
        self.speed = self.ammo.as_ref().and_then(|a| a.speed);
        let rounds = self
            .ammo
            .as_ref()
            .and_then(|a| a.capacity)
            .filter(|c| *c > 0.0);
        self.ammo_capacity = rounds.or(self.capacitor.and_then(|c| c.shots));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Missile {
    #[serde(flatten)]
    pub item: ItemIdentity,
    /// `AttachDef.Type` is `Bomb` or the record carries `SCItemBombParams` (local only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_bomb: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage: Option<Damage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_total: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_type: Option<String>,
    /// `Infrared`, `Electromagnetic`, `CrossSection`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking_signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_range_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_range_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locking_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boost_duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lifetime: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explosion_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rapid(rate: f64) -> FireMode {
        FireMode {
            name: "Rapid".into(),
            kind: FireKind::Rapid,
            fire_rate: Some(rate),
            ..FireMode::default()
        }
    }

    /// Spec 13 reference: S5 Behring gatling, 102.5 × 900 RPM = 1,537.5 DPS.
    #[test]
    fn reference_behring_gatling_dps() {
        let mut m = rapid(900.0);
        m.finish(Some(102.5));
        assert_eq!(m.alpha, Some(102.5));
        assert_eq!(m.dps, Some(1537.5));
        assert_eq!(m.label(), "Auto");
    }

    /// Spec 13 reference: 1,251 m/s × 3.2 s = 4,003 m.
    #[test]
    fn reference_ammo_range() {
        let mut a = Ammo {
            speed: Some(1251.0),
            lifetime: Some(3.2),
            ..Ammo::default()
        };
        a.derive();
        assert_eq!(a.range, Some(4003.0));
    }

    #[test]
    fn pellets_and_multiplier() {
        let mut m = FireMode {
            kind: FireKind::Single,
            fire_rate: Some(60.0),
            pellets: Some(8.0),
            damage_multiplier: Some(1.5),
            ..FireMode::default()
        };
        m.finish(Some(10.0));
        assert_eq!(m.alpha, Some(120.0));
        assert_eq!(m.dps, Some(120.0));
        assert_eq!(m.label(), "Semi");
    }

    #[test]
    fn burst_rate() {
        // 3 shots at 900 RPM (0.2 s) + 0.25 s cooldown = 0.45 s per burst -> 400 RPM.
        assert_eq!(burst_sustained_rate(900.0, 3.0, 0.25), Some(400.0));
        assert_eq!(burst_sustained_rate(0.0, 3.0, 0.25), None);
        let mut m = FireMode {
            kind: FireKind::Burst,
            fire_rate: Some(900.0),
            shot_count: Some(3.0),
            sustained_fire_rate: burst_sustained_rate(900.0, 3.0, 0.25),
            ..FireMode::default()
        };
        m.finish(Some(10.0));
        assert_eq!(m.dps, Some(66.67));
        assert_eq!(m.label(), "Burst 3");
    }

    #[test]
    fn sequence_rate() {
        let e = |delay, secs, rep| SequenceEntry {
            delay,
            delay_in_seconds: secs,
            repetitions: rep,
        };
        // 2 shots 0.5 s apart -> 120 RPM.
        assert_eq!(sequence_fire_rate(&[e(0.5, true, 2.0)]), Some(120.0));
        // 600 RPM entry (0.1 s per shot) ×1 plus 0.4 s entry ×1 -> 2 shots in 0.5 s.
        assert_eq!(
            sequence_fire_rate(&[e(600.0, false, 1.0), e(0.4, true, 1.0)]),
            Some(240.0)
        );
        assert_eq!(sequence_fire_rate(&[e(0.0, true, 1.0)]), None);
    }

    #[test]
    fn beam_dps_is_damage_per_second() {
        let mut d = Damage::default();
        d.set("energy", 300.0);
        d.set("thermal", 50.0);
        let mut m = FireMode {
            kind: FireKind::Beam,
            beam_damage: Some(d),
            ..FireMode::default()
        };
        m.finish(Some(999.0));
        assert_eq!(m.dps, Some(350.0));
        assert_eq!(m.alpha, None);
    }

    /// Spec 13: a 300/900 RPM rifle's headline is the higher-DPS mode, never `Semi`.
    #[test]
    fn headline_is_highest_dps() {
        let mut semi = FireMode {
            kind: FireKind::Single,
            fire_rate: Some(300.0),
            ..FireMode::default()
        };
        semi.finish(Some(10.0));
        let mut auto = rapid(900.0);
        auto.finish(Some(10.0));
        let mut w = Weapon {
            modes: vec![semi, auto],
            ..Weapon::default()
        };
        w.derive_headline();
        assert_eq!(w.fire_rate, Some(900.0));
        assert_eq!(w.headline().unwrap().label(), "Auto");
    }

    #[test]
    fn damage_parts_total_dominant() {
        let mut d = Damage::default();
        d.set("physical", 120.0);
        d.set("energy", 30.0);
        d.set("stun", 0.0);
        assert_eq!(d.parts(), vec![("physical", 120.0), ("energy", 30.0)]);
        assert_eq!(d.total(), 150.0);
        assert_eq!(d.dominant(), Some("physical"));
        assert!(Damage::default().is_empty());
    }

    /// A dump record (camelCase, unknown fields) deserialises into the model.
    #[test]
    fn dump_weapon_deserialises() {
        let j = r#"{"name":"Mantis GT-220 Gatling","recordName":"EntityClassDefinition.GATS_BallisticGatling_S3",
            "id":"x","class":"ship","type":"WeaponGun","subType":"Gun","size":3,"grade":1,"manufacturer":"GATS",
            "dps":853.33,"durability":{"health":1},"modes":[{"name":"Rapid","kind":"rapid","fireRate":800,"alpha":64,"dps":853.33}],
            "ammo":{"recordName":"AmmoParams.x","speed":1600,"damage":{"physical":64},"capacity":1000}}"#;
        let w: Weapon = serde_json::from_str(j).unwrap();
        assert_eq!(w.item.short_name(), "GATS_BallisticGatling_S3");
        assert_eq!(w.item.item_type.as_deref(), Some("WeaponGun"));
        assert_eq!(w.item.size, Some(3));
        assert_eq!(w.modes[0].kind, FireKind::Rapid);
        assert_eq!(w.ammo.unwrap().damage.unwrap().physical, Some(64.0));
    }
}
