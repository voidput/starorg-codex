//! Ammo, ship components, weapons and missiles (spec 5.3 to 5.6).
//!
//! Field reads follow `sc-gamedata`'s parsers (the producer of the published dumps), so
//! both sources compute the same numbers.

use super::Loc;
use super::json::{
    array, at, attach_def, component, num, num_at, referenced_name, text, text_at, type_of,
};
use serde_json::Value;
use starorg_core::model::items::{SequenceEntry, burst_sustained_rate, sequence_fire_rate};
use starorg_core::model::{
    Ammo, Capacitor, Component, Damage, Falloff, FireKind, FireMode, ItemIdentity, LocKeys, MinMax,
    Missile, SignatureDetection, Weapon, WeaponAmmo, WeaponClass, is_placeholder_name,
    items::DAMAGE_TYPES,
};
use std::collections::BTreeMap;

/// DataCore field names of the damage types, in [`DAMAGE_TYPES`] order.
const DAMAGE_FIELDS: [&str; 6] = [
    "DamagePhysical",
    "DamageEnergy",
    "DamageDistortion",
    "DamageThermal",
    "DamageBiochemical",
    "DamageStun",
];

/// A `{DamagePhysical: …, …}` struct; zero components left out; `None` when all zero.
pub fn damage_info(v: Option<&Value>) -> Option<Damage> {
    let v = v?;
    v.as_object()?;
    let mut d = Damage::default();
    for (kind, field) in DAMAGE_TYPES.iter().zip(DAMAGE_FIELDS) {
        if let Some(x) = num(v.get(field)).filter(|x| *x != 0.0) {
            d.set(kind, x);
        }
    }
    (!d.is_empty()).then_some(d)
}

/// The first `DamageInfo` with any non-zero component, depth first (spec 5.4).
pub fn find_damage(v: &Value) -> Option<Damage> {
    match v {
        Value::Array(a) => a.iter().find_map(find_damage),
        Value::Object(o) => {
            if type_of(v) == Some("DamageInfo")
                && let Some(d) = damage_info(Some(v))
            {
                return Some(d);
            }
            o.values().find_map(find_damage)
        }
        _ => None,
    }
}

fn falloff(v: Option<&Value>) -> Option<BTreeMap<String, Falloff>> {
    let v = v?;
    let mut out = BTreeMap::new();
    for (kind, field) in DAMAGE_TYPES.iter().zip(DAMAGE_FIELDS) {
        let per_meter = num_at(v, &format!("damageDropPerMeter.{field}")).unwrap_or(0.0);
        if per_meter <= 0.0 {
            continue;
        }
        out.insert(
            kind.to_string(),
            Falloff {
                min_distance: num_at(v, &format!("damageDropMinDistance.{field}")).unwrap_or(0.0),
                per_meter,
                floor: num_at(v, &format!("damageDropMinDamage.{field}")).unwrap_or(0.0),
            },
        );
    }
    (!out.is_empty()).then_some(out)
}

pub fn parse_ammo(name: &str, id: &str, path: &str, v: &Value) -> Ammo {
    let damage = v
        .get("projectileParams")
        .and_then(find_damage)
        .or_else(|| find_damage(v));
    let mut a = Ammo {
        name: starorg_core::model::short_name(name).to_string(),
        record_name: name.to_string(),
        id: id.to_string(),
        path: path.to_string(),
        size: num(v.get("size")),
        speed: num(v.get("speed")),
        lifetime: num(v.get("lifetime")),
        range: None,
        damage,
        damage_total: None,
        falloff: falloff(at(v, "projectileParams.damageDropParams")),
    };
    a.derive();
    a
}

/// Identity of an entity record: `AttachDef`, loc keys, template flag.
pub fn identity(name: &str, id: &str, path: &str, entity: &Value, loc: &Loc) -> ItemIdentity {
    let def = attach_def(entity);
    let keys = LocKeys {
        name: text_at(def, "Localization.Name").map(str::to_string),
        short_name: text_at(def, "Localization.ShortName").map(str::to_string),
        desc: text_at(def, "Localization.Description").map(str::to_string),
    };
    let short = starorg_core::model::short_name(name);
    let display = keys.name.as_deref().and_then(|k| loc.text(k));
    let is_template = is_placeholder_name(display) || short.to_ascii_lowercase().ends_with("_prop");
    ItemIdentity {
        record_name: name.to_string(),
        id: id.to_string(),
        path: path.to_string(),
        loc: keys,
        is_template,
        item_type: text(def.get("Type")).map(str::to_string),
        sub_type: text(def.get("SubType"))
            .filter(|s| *s != "UNDEFINED")
            .map(str::to_string),
        size: num(def.get("Size")).map(|x| x as i64),
        grade: num(def.get("Grade")).map(|x| x as i64),
        manufacturer: referenced_name(def.get("Manufacturer")).map(str::to_ascii_uppercase),
    }
}

/// Draw, output and signatures from the resource network "Online" state.
#[derive(Debug, Default, Clone)]
pub struct Resources {
    pub power: Option<f64>,
    pub em: Option<f64>,
    pub ir: Option<f64>,
    pub generation: BTreeMap<String, f64>,
}

fn rate_of(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    num_at(v, "resourceAmountPerSecond.standardResourceUnits")
        .or_else(|| num_at(v, "resourceAmountPerSecond.units"))
}

pub fn resources(entity: &Value) -> Resources {
    let Some(res) = component(entity, "ItemResourceComponentParams") else {
        return Resources::default();
    };
    let states = array(res.get("states"));
    let Some(state) = states
        .iter()
        .find(|s| text(s.get("name")) == Some("Online"))
        .or(states.first())
    else {
        return Resources::default();
    };
    let mut generation = BTreeMap::new();
    let mut consumption = BTreeMap::new();
    for delta in array(state.get("deltas")) {
        for (side, bucket) in [
            ("generation", &mut generation),
            ("consumption", &mut consumption),
        ] {
            let block = delta.get(side);
            if let (Some(resource), Some(rate)) =
                (block.and_then(|b| text(b.get("resource"))), rate_of(block))
            {
                *bucket.entry(resource.to_string()).or_insert(0.0) += rate;
            }
        }
    }
    Resources {
        power: consumption.get("Power").copied(),
        em: num_at(state, "signatureParams.EMSignature.nominalSignature"),
        ir: num_at(state, "signatureParams.IRSignature.nominalSignature"),
        generation,
    }
}

pub fn health(entity: &Value) -> Option<f64> {
    component(entity, "SHealthComponentParams").and_then(|c| num(c.get("Health")))
}

pub fn mass(entity: &Value) -> Option<f64> {
    component(entity, "SEntityPhysicsControllerParams").and_then(|c| num_at(c, "PhysType.Mass"))
}

fn pick(stats: &mut BTreeMap<String, f64>, v: &Value, fields: &[&str]) {
    for f in fields {
        if let Some(x) = num_at(v, f) {
            stats.insert(f.rsplit('.').next().unwrap_or(f).to_string(), x);
        }
    }
}

fn ranges(v: Option<&Value>) -> Option<BTreeMap<String, MinMax>> {
    let mut out = BTreeMap::new();
    for (i, e) in array(v).iter().enumerate() {
        if let (Some(kind), Some(min), Some(max)) =
            (DAMAGE_TYPES.get(i), num(e.get("Min")), num(e.get("Max")))
        {
            out.insert(kind.to_string(), MinMax { min, max });
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `{DamagePhysical: 0.75, …}` as a lower-case map.
fn damage_map(v: Option<&Value>) -> Option<BTreeMap<String, f64>> {
    let obj = v?.as_object()?;
    let out: BTreeMap<String, f64> = obj
        .iter()
        .filter_map(|(k, v)| Some((k.strip_prefix("Damage")?.to_lowercase(), v.as_f64()?)))
        .collect();
    (!out.is_empty()).then_some(out)
}

fn ports(entity: &Value) -> &[Value] {
    component(entity, "SItemPortContainerComponentParams")
        .map(|c| array(c.get("Ports")))
        .unwrap_or(&[])
}

/// Count and largest size of the ports accepting `ty`.
fn ports_of_type(entity: &Value, ty: &str) -> (usize, Option<f64>) {
    let matching: Vec<&Value> = ports(entity)
        .iter()
        .filter(|p| {
            array(p.get("Types"))
                .iter()
                .any(|t| text(t.get("Type")) == Some(ty))
        })
        .collect();
    let size = matching
        .iter()
        .filter_map(|p| num(p.get("MaxSize")))
        .reduce(f64::max);
    (matching.len(), size)
}

const UNBOUNDED: f64 = 1e30;
/// Interior dimensions at or above this are "unbounded" sentinels.
const SENTINEL_DIMENSION: f64 = 900.0;
/// One SCU is a 1.25 m cube (spec 5.2: ÷ 1.953125).
const CUBIC_METRES_PER_SCU: f64 = 1.953125;

/// SCU of an `InventoryContainer` record from its interior dimensions.
pub fn container_scu(v: &Value) -> Option<f64> {
    let d = v.get("interiorDimensions")?;
    let (x, y, z) = (num(d.get("x"))?, num(d.get("y"))?, num(d.get("z"))?);
    if [x, y, z].iter().any(|v| *v >= SENTINEL_DIMENSION) || x * y * z == 0.0 {
        return None;
    }
    let volume = (x * y * z * 1000.0).round() / 1000.0;
    Some(((volume / CUBIC_METRES_PER_SCU) * 100.0).round() / 100.0)
}

/// Categorise a ship item and read its stats. `container_scu` resolves a cargo grid's
/// container short name to SCU. Order is fixed (spec 5.1): missiles before guns,
/// countermeasures before weapons, quantum fuel tanks before fuel tanks.
pub fn parse_component(
    item: ItemIdentity,
    entity: &Value,
    container_scu: &dyn Fn(&str) -> Option<f64>,
) -> Option<Component> {
    let ty = item.item_type.clone();
    let is_type = |names: &[&str]| ty.as_deref().is_some_and(|t| names.contains(&t));
    let mut stats = BTreeMap::new();
    let mut c = Component::default();
    let category = if let Some(s) = component(entity, "SCItemShieldGeneratorParams") {
        pick(
            &mut stats,
            s,
            &[
                "MaxShieldHealth",
                "MaxShieldRegen",
                "DecayRatio",
                "DownedRegenDelay",
                "DamagedRegenDelay",
            ],
        );
        c.detail.resistance = ranges(s.get("ShieldResistance"));
        c.detail.absorption = ranges(s.get("ShieldAbsorption"));
        "shields"
    } else if let Some(s) = component(entity, "SCItemQuantumDriveParams") {
        pick(
            &mut stats,
            s,
            &[
                "quantumFuelRequirement",
                "jumpRange",
                "params.driveSpeed",
                "params.cooldownTime",
                "params.stageOneAccelRate",
                "params.stageTwoAccelRate",
                "params.spoolUpTime",
                "params.calibrationRate",
                "params.minCalibrationRequirement",
                "params.maxCalibrationRequirement",
            ],
        );
        if stats.get("jumpRange").is_some_and(|r| *r > UNBOUNDED) {
            stats.remove("jumpRange");
        }
        "quantumDrives"
    } else if let Some(s) = component(entity, "SCItemRadarComponentParams") {
        pick(
            &mut stats,
            s,
            &[
                "detectionLifetime",
                "physicalRadarRange",
                "pingProperties.cooldownTime",
            ],
        );
        let det: Vec<SignatureDetection> = array(s.get("signatureDetection"))
            .iter()
            .map(|d| SignatureDetection {
                sensitivity: num(d.get("sensitivity")),
                piercing: num(d.get("piercing")),
                passive: d.get("permitPassiveDetection") == Some(&Value::Bool(true)),
                active: d.get("permitActiveDetection") == Some(&Value::Bool(true)),
            })
            .collect();
        c.detail.signature_detection = (!det.is_empty()).then_some(det);
        "radars"
    } else if let Some(s) = component(entity, "SCItemMissileParams") {
        pick(
            &mut stats,
            s,
            &[
                "targetingParams.lockTime",
                "targetingParams.lockRangeMax",
                "explosionParams.maxRadius",
                "GCSParams.linearSpeed",
                "maxLifetime",
            ],
        );
        "missiles"
    } else if is_type(&["WeaponDefensive"]) {
        if let Some(a) = component(entity, "SAmmoContainerComponentParams") {
            pick(&mut stats, a, &["maxAmmoCount", "maxRestockCount"]);
            c.detail.ammo = referenced_name(a.get("ammoParamsRecord")).map(str::to_string);
        }
        "countermeasures"
    } else if let Some(s) = component(entity, "SCItemWeaponComponentParams") {
        pick(
            &mut stats,
            s,
            &["connectionParams.simplifiedHeatParams.overheatTemperature"],
        );
        if let Some(a) = component(entity, "SAmmoContainerComponentParams") {
            pick(&mut stats, a, &["maxAmmoCount"]);
        }
        "weapons"
    } else if is_type(&["PowerPlant"]) {
        if let Some(x) = resources(entity).generation.get("Power") {
            stats.insert("powerOutput".into(), *x);
        }
        "powerPlants"
    } else if is_type(&["Cooler"]) {
        if let Some(x) = resources(entity).generation.get("Coolant") {
            stats.insert("coolantOutput".into(), *x);
        }
        "coolers"
    } else if is_type(&["LifeSupportGenerator"]) {
        "lifeSupport"
    } else if let Some(s) = component(entity, "SCItemThrusterParams") {
        pick(
            &mut stats,
            s,
            &["thrustCapacity", "fuelBurnRatePer10KNewton"],
        );
        c.detail.thruster_type = text(s.get("thrusterType")).map(str::to_string);
        "thrusters"
    } else if component(entity, "SCItemFuelTankParams").is_some() {
        if let Some(r) = component(entity, "ResourceContainer") {
            pick(&mut stats, r, &["capacity.standardCargoUnits"]);
        }
        if is_type(&["QuantumFuelTank"]) {
            "quantumFuelTanks"
        } else {
            "fuelTanks"
        }
    } else if let Some(s) = component(entity, "SCItemVehicleArmorParams") {
        pick(
            &mut stats,
            s,
            &[
                "signalInfrared",
                "signalElectromagnetic",
                "signalCrossSection",
            ],
        );
        c.detail.damage_multiplier = damage_map(s.get("damageMultiplier"));
        c.detail.deflection = damage_map(at(s, "armorDeflection.deflectionValue"));
        "armor"
    } else if component(entity, "SCItemMissileRackParams").is_some() {
        let (count, size) = ports_of_type(entity, "Missile");
        stats.insert("missileCount".into(), count as f64);
        if let Some(s) = size {
            stats.insert("missileSize".into(), s);
        }
        "missileRacks"
    } else if let Some(s) = component(entity, "SCItemInventoryContainerComponentParams")
        .filter(|_| is_type(&["CargoGrid"]))
    {
        let name = referenced_name(s.get("containerParams"));
        if let Some(scu) = name.and_then(container_scu) {
            stats.insert("scu".into(), scu);
        }
        c.detail.container = name.map(str::to_string);
        "cargoGrids"
    } else if component(entity, "SCItemJumpDriveParams").is_some() {
        "jumpDrives"
    } else if component(entity, "SCItemQuantumInterdictionGeneratorParams").is_some() {
        "quantumEnforcement"
    } else if let Some(s) = component(entity, "SCItemEMPParams") {
        for (k, v) in s.as_object().into_iter().flatten() {
            if let Some(x) = v.as_f64() {
                stats.insert(k.clone(), x);
            }
        }
        "emp"
    } else if let Some(s) = component(entity, "SSCItemSelfDestructComponentParams") {
        pick(&mut stats, s, &["damage", "minRadius", "radius", "time"]);
        "selfDestruct"
    } else if is_type(&["Battery"]) {
        if let Some(r) = component(entity, "ResourceContainer") {
            pick(&mut stats, r, &["capacity.standardCargoUnits"]);
        }
        "batteries"
    } else if is_type(&["Turret"]) && item.path.contains("/weapon_mounts/") {
        let (count, size) = ports_of_type(entity, "WeaponGun");
        stats.insert("gunCount".into(), count as f64);
        if let Some(s) = size {
            stats.insert("gunSize".into(), s);
        }
        "mounts"
    } else {
        return None;
    };
    let r = resources(entity);
    c.item = item;
    c.category = category.to_string();
    c.power = r.power;
    c.em = r.em;
    c.ir = r.ir;
    c.health = health(entity);
    c.mass = mass(entity);
    c.distortion_max = component(entity, "SDistortionParams").and_then(|d| num(d.get("Maximum")));
    c.stats = stats;
    Some(c)
}

fn launch_stats(action: &Value, mode: &mut FireMode) {
    let launch = action.get("launchParams").unwrap_or(&Value::Null);
    mode.fire_rate = num(action.get("fireRate"));
    mode.heat_per_shot = num(action.get("heatPerShot"));
    mode.pellets = num(launch.get("pelletCount"));
    mode.damage_multiplier = num(launch.get("damageMultiplier"));
}

/// One fire action (spec 5.4 table). `per_shot` is the ammo's total damage.
pub fn parse_fire_mode(action: &Value, per_shot: Option<f64>) -> Option<FireMode> {
    let ty = type_of(action)?;
    let default_name = ty
        .strip_prefix("SWeaponActionFire")
        .unwrap_or(ty)
        .strip_suffix("Params")
        .unwrap_or(ty);
    let mut m = FireMode {
        name: text(action.get("name")).unwrap_or(default_name).to_string(),
        ..FireMode::default()
    };
    match ty {
        "SWeaponActionFireSingleParams" | "SWeaponActionFireRapidParams" => {
            m.kind = if ty.contains("Rapid") {
                FireKind::Rapid
            } else {
                FireKind::Single
            };
            launch_stats(action, &mut m);
        }
        "SWeaponActionFireBurstParams" => {
            m.kind = FireKind::Burst;
            launch_stats(action, &mut m);
            m.shot_count = num(action.get("shotCount"));
            let cooldown = num(action.get("cooldownTime")).unwrap_or(0.0);
            if let (Some(rate), Some(n)) = (m.fire_rate, m.shot_count) {
                m.sustained_fire_rate = burst_sustained_rate(rate, n, cooldown);
            }
        }
        "SWeaponActionFireChargedParams" => {
            m.kind = FireKind::Charged;
            launch_stats(action.get("weaponAction").unwrap_or(&Value::Null), &mut m);
            m.charge_time = num(action.get("chargeTime"));
            m.charged_damage_multiplier = num_at(action, "maxChargeModifier.damageMultiplier");
        }
        "SWeaponActionSequenceParams" => {
            m.kind = FireKind::Sequence;
            let entries = array(action.get("sequenceEntries"));
            let first = entries
                .iter()
                .filter_map(|e| e.get("weaponAction"))
                .find(|a| a.is_object());
            if let Some(f) = first {
                launch_stats(f, &mut m);
            }
            let seq: Vec<SequenceEntry> = entries
                .iter()
                .filter_map(|e| {
                    Some(SequenceEntry {
                        delay: num(e.get("delay"))?,
                        delay_in_seconds: text(e.get("unit")) == Some("Seconds"),
                        repetitions: num(e.get("repetitions")).unwrap_or(1.0),
                    })
                })
                .collect();
            m.fire_rate = sequence_fire_rate(&seq).or(m.fire_rate);
            m.name = text(action.get("name"))
                .or_else(|| first.and_then(|f| text(f.get("name"))))
                .unwrap_or("Sequence")
                .to_string();
        }
        "SWeaponActionFireBeamParams" => {
            m.kind = FireKind::Beam;
            m.heat_per_shot = num(action.get("heatPerSecond"));
            m.full_damage_range = num(action.get("fullDamageRange"));
            m.zero_damage_range = num(action.get("zeroDamageRange"));
            m.beam_damage = damage_info(action.get("damagePerSecond"));
        }
        _ => {
            m.kind = FireKind::Other;
            return Some(m);
        }
    }
    m.finish(per_shot);
    Some(m)
}

fn capacitor(params: &Value) -> Option<Capacitor> {
    let r = params.get("weaponRegenConsumerParams")?;
    r.as_object()?;
    let c = Capacitor {
        shots: num(r.get("maxAmmoLoad")),
        regen_per_second: num(r.get("maxRegenPerSec")),
        regen_cooldown: num(r.get("regenerationCooldown")),
        cost_per_shot: num(r.get("regenerationCostPerBullet")),
        requested_regen_per_second: num(r.get("requestedRegenPerSec")),
    };
    c.shots.filter(|s| *s != 0.0).map(|_| c)
}

/// Default magazine of an FPS weapon: the loadout entry whose port name contains
/// `magazine` (spec 5.5).
pub fn default_magazine(entity: &Value) -> Option<&str> {
    let lo = component(entity, "SEntityComponentDefaultLoadoutParams")?;
    array(at(lo, "loadout.entries")).iter().find_map(|e| {
        text(e.get("itemPortName"))
            .filter(|p| p.to_ascii_lowercase().contains("magazine"))
            .and_then(|_| text(e.get("entityClassName")))
    })
}

/// Ammo record short name and magazine size.
pub type AmmoLink = (String, Option<f64>);

/// Ammo record short name and magazine size from an `SAmmoContainerComponentParams`.
pub fn ammo_link(entity: &Value) -> Option<AmmoLink> {
    let c = component(entity, "SAmmoContainerComponentParams")?;
    let name = referenced_name(c.get("ammoParamsRecord"))?;
    Some((name.to_string(), num(c.get("maxAmmoCount"))))
}

/// Mining laser modifiers: every `*Modifier` struct of `MiningLaserModifier` with a value.
fn mining_modifiers(entity: &Value) -> Option<BTreeMap<String, f64>> {
    let m = component(entity, "SEntityComponentMiningLaserParams")?.get("miningLaserModifiers")?;
    let mut out = BTreeMap::new();
    for (k, v) in m.as_object()? {
        if let Some(x) = num_at(v, "FloatModifierMultiplicative.value")
            .or_else(|| num_at(v, "FloatModifierAdditive.value"))
            .or_else(|| num(v.get("value")))
        {
            out.insert(k.clone(), x);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// A weapon. `ammo_of` resolves an ammo short name; `magazine_link` resolves a magazine
/// entity short name to its ammo link (FPS).
pub fn parse_weapon(
    item: ItemIdentity,
    entity: &Value,
    class: WeaponClass,
    ammo_of: &dyn Fn(&str) -> Option<Ammo>,
    magazine_link: &dyn Fn(&str) -> Option<AmmoLink>,
) -> Option<Weapon> {
    let params = component(entity, "SCItemWeaponComponentParams")?;
    let mut link = ammo_link(entity).and_then(|(n, c)| Some((ammo_of(&n)?, c)));
    let mut magazine = None;
    if link.is_none()
        && let Some(mag) = default_magazine(entity)
    {
        magazine = Some(mag.to_string());
        link = magazine_link(mag).and_then(|(n, c)| Some((ammo_of(&n)?, c)));
    }
    let per_shot = link.as_ref().and_then(|(a, _)| a.damage_total);
    let modes: Vec<FireMode> = array(params.get("fireActions"))
        .iter()
        .filter(|a| a.is_object())
        .filter_map(|a| parse_fire_mode(a, per_shot))
        .collect();
    let r = resources(entity);
    let heat = at(params, "connectionParams.simplifiedHeatParams");
    let mut w = Weapon {
        item,
        class,
        power: r.power,
        em: r.em,
        ir: r.ir,
        health: health(entity),
        mass: mass(entity),
        overheat_temperature: heat.and_then(|h| num(h.get("overheatTemperature"))),
        capacitor: capacitor(params),
        ammo: link.map(|(a, cap)| WeaponAmmo {
            record_name: a.record_name,
            magazine,
            speed: a.speed,
            lifetime: a.lifetime,
            range: a.range,
            damage: a.damage,
            damage_per_shot: a.damage_total,
            falloff: a.falloff,
            capacity: cap.filter(|c| *c > 0.0),
        }),
        modes,
        mining_modifiers: mining_modifiers(entity),
        ..Weapon::default()
    };
    w.derive_headline();
    Some(w)
}

pub fn parse_missile(item: ItemIdentity, entity: &Value) -> Option<Missile> {
    let params = component(entity, "SCItemMissileParams")?;
    let damage = damage_info(at(params, "explosionParams.damage"));
    let t = params.get("targetingParams").unwrap_or(&Value::Null);
    let g = params.get("GCSParams").unwrap_or(&Value::Null);
    let is_bomb = item.item_type.as_deref() == Some("Bomb")
        || component(entity, "SCItemBombParams").is_some();
    Some(Missile {
        item,
        is_bomb,
        damage_total: damage.map(|d| d.total()),
        damage_type: damage.and_then(|d| d.dominant()).map(str::to_string),
        damage,
        tracking_signal: text(t.get("trackingSignalType")).map(str::to_string),
        lock_time: num(t.get("lockTime")),
        lock_range_min: num(t.get("lockRangeMin")),
        lock_range_max: num(t.get("lockRangeMax")),
        locking_angle: num(t.get("lockingAngle")),
        speed: num(g.get("linearSpeed")),
        boost_duration: num(g.get("boostPhaseDuration")),
        max_lifetime: num(params.get("maxLifetime")),
        explosion_radius: num_at(params, "explosionParams.maxRadius"),
        health: health(entity),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn find_damage_skips_zero_infos() {
        let v = json!({"a":[{"_Type_":"DamageInfo","DamagePhysical":0},{"x":{"_Type_":"DamageInfo","DamageEnergy":30,"DamagePhysical":120}}]});
        let d = find_damage(&v).unwrap();
        assert_eq!(d.physical, Some(120.0));
        assert_eq!(d.energy, Some(30.0));
    }

    #[test]
    fn container_scu_rule() {
        let c = json!({"interiorDimensions":{"x":2.5,"y":2.5,"z":2.5}});
        assert_eq!(container_scu(&c), Some(8.0));
        assert_eq!(
            container_scu(&json!({"interiorDimensions":{"x":1000,"y":1,"z":1}})),
            None
        );
    }

    #[test]
    fn sequence_action() {
        let a = json!({"_Type_":"SWeaponActionSequenceParams","sequenceEntries":[
            {"delay":0.5,"unit":"Seconds","repetitions":2,"weaponAction":{"_Type_":"SWeaponActionFireSingleParams","name":"Single","fireRate":100}}]});
        let m = parse_fire_mode(&a, Some(10.0)).unwrap();
        assert_eq!(m.kind, FireKind::Sequence);
        assert_eq!(m.fire_rate, Some(120.0));
        assert_eq!(m.name, "Single");
        assert_eq!(m.dps, Some(20.0));
    }

    #[test]
    fn charged_action_reads_nested() {
        let a = json!({"_Type_":"SWeaponActionFireChargedParams","chargeTime":1.5,
            "weaponAction":{"_Type_":"SWeaponActionFireSingleParams","fireRate":60,"launchParams":{"pelletCount":8}}});
        let m = parse_fire_mode(&a, Some(5.0)).unwrap();
        assert_eq!(m.kind, FireKind::Charged);
        assert_eq!(m.alpha, Some(40.0));
        assert_eq!(m.label(), "Charged");
    }
}
