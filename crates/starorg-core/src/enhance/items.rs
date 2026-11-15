//! Stat lines and tag elements for ship components, weapons and missiles (spec 5.3-5.6).
//!
//! Every function here is pure: model record in, lines out. Lines follow the format table
//! of spec 5.1; a stat that is missing or zero is left out, a line with nothing left is
//! left out.

use super::format::{
    SEP, by_size, d1, d2, distance, distance_range, int, line, nz, pct, pct_signed, sig2,
};
use super::profile::Profile;
use super::tags::Element;
use crate::model::items::DAMAGE_TYPES;
use crate::model::{Component, Damage, FireKind, FireMode, Missile, Weapon, WeaponClass};

/// Short damage-type labels used in stat lines.
pub fn damage_label(kind: &str) -> &'static str {
    match kind {
        "physical" => "Phys",
        "energy" => "Energy",
        "distortion" => "Dist",
        "thermal" => "Therm",
        "biochemical" => "Bio",
        "stun" => "Stun",
        _ => "",
    }
}

/// Component categories with their own lines (spec 5.3).
pub const MAIN_CATEGORIES: [&str; 6] = [
    "shields",
    "quantumDrives",
    "radars",
    "powerPlants",
    "coolers",
    "missileRacks",
];
/// Categories that get the common lines only, when at least two have data.
pub const COMMON_ONLY: [&str; 9] = [
    "thrusters",
    "fuelTanks",
    "quantumFuelTanks",
    "armor",
    "lifeSupport",
    "jumpDrives",
    "emp",
    "quantumEnforcement",
    "batteries",
];
/// Categories that carry a `[class-size-grade]` tag; racks get a size-only tag.
pub const TAGGED: [&str; 5] = [
    "shields",
    "powerPlants",
    "coolers",
    "quantumDrives",
    "radars",
];

struct Lines<'p> {
    p: &'p Profile,
    cat: &'static str,
    out: Vec<String>,
}

impl<'p> Lines<'p> {
    fn new(p: &'p Profile, cat: &'static str) -> Self {
        Self {
            p,
            cat,
            out: Vec::new(),
        }
    }

    fn push(&mut self, name: &str, l: Option<String>) {
        if let Some(l) = l
            && self.p.line_on(self.cat, name)
        {
            self.out.push(l);
        }
    }
}

/// `Signatures: EM 500  |  IR 0`: each is shown when the other is not zero; the line goes
/// when both are.
fn signatures(em: Option<f64>, ir: Option<f64>) -> Option<String> {
    let (e, i) = (em.unwrap_or(0.0), ir.unwrap_or(0.0));
    (e != 0.0 || i != 0.0).then(|| format!("Signatures: EM {}{SEP}IR {}", int(e), int(i)))
}

/// Power draw: `int`, except that a draw below 1 PU/s keeps one decimal (`0.1`), so a
/// real draw never prints as `0`.
pub fn power_text(v: f64) -> String {
    if v.abs() < 0.95 { d1(v) } else { int(v) }
}

/// Common lines (spec 5.3): power draw, signatures, health and distortion.
fn common(
    l: &mut Lines,
    power: Option<f64>,
    em: Option<f64>,
    ir: Option<f64>,
    health: Option<f64>,
    distortion: Option<f64>,
) -> usize {
    let before = l.out.len();
    l.push(
        "power",
        nz(power).map(|v| format!("Power draw: {} PU/s", power_text(v))),
    );
    l.push("signatures", signatures(em, ir));
    l.push(
        "health",
        line([
            nz(health).map(|v| format!("Component HP: {}", int(v))),
            nz(distortion).map(|v| format!("Max distortion: {}", int(v))),
        ]),
    );
    l.out.len() - before
}

fn stat(c: &Component, k: &str) -> Option<f64> {
    nz(c.stats.get(k).copied())
}

/// Stat lines of a ship component, `None` when it gets no block.
pub fn component_lines(c: &Component, p: &Profile) -> Option<Vec<String>> {
    let mut l = Lines::new(p, "components");
    match c.category.as_str() {
        "shields" => {
            l.push(
                "shield",
                line([
                    stat(c, "MaxShieldHealth").map(|v| format!("Shield HP: {}", int(v))),
                    stat(c, "MaxShieldRegen").map(|v| format!("Regen: {} HP/s", int(v))),
                ]),
            );
            l.push(
                "delay",
                line([
                    stat(c, "DownedRegenDelay").map(|v| format!("Delay: {}s when down", d1(v))),
                    stat(c, "DamagedRegenDelay").map(|v| format!("{}s when damaged", d1(v))),
                ])
                .map(|s| {
                    if s.starts_with("Delay:") {
                        s
                    } else {
                        format!("Delay: {s}")
                    }
                }),
            );
            if let Some(res) = &c.detail.resistance {
                let parts = DAMAGE_TYPES.iter().filter_map(|t| {
                    let r = res.get(*t)?;
                    (r.min != 0.0 || r.max != 0.0).then(|| {
                        format!(
                            "{} {}–{}",
                            damage_label(t),
                            int(r.min * 100.0),
                            pct(r.max * 100.0)
                        )
                    })
                });
                let parts: Vec<String> = parts.collect();
                l.push(
                    "resist",
                    (!parts.is_empty()).then(|| format!("Resist: {}", parts.join(SEP))),
                );
            }
        }
        "quantumDrives" => {
            l.push(
                "speed",
                line([
                    stat(c, "driveSpeed").map(|v| format!("Speed: {} Mm/s", int(v / 1e6))),
                    stat(c, "spoolUpTime").map(|v| format!("Spool: {}s", d1(v))),
                    stat(c, "cooldownTime").map(|v| format!("Cooldown: {}s", d1(v))),
                ]),
            );
            l.push(
                "fuel",
                stat(c, "quantumFuelRequirement").map(|v| format!("Fuel: {} per Gm", sig2(v))),
            );
            l.push(
                "accel",
                line([
                    stat(c, "stageOneAccelRate").map(|v| format!("stage 1 {}", int(v))),
                    stat(c, "stageTwoAccelRate").map(|v| format!("stage 2 {}", int(v))),
                ])
                .map(|s| format!("Accel: {s}")),
            );
            let needs = match (
                stat(c, "minCalibrationRequirement"),
                stat(c, "maxCalibrationRequirement"),
            ) {
                (Some(a), Some(b)) => Some(format!("Needs {}–{}", int(a), int(b))),
                (None, Some(b)) => Some(format!("Needs {}", int(b))),
                _ => None,
            };
            l.push(
                "calibration",
                line([
                    stat(c, "calibrationRate").map(|v| format!("Calibration: {}/s", int(v))),
                    needs,
                ])
                .map(|s| {
                    if s.starts_with("Calibration:") {
                        s
                    } else {
                        format!("Calibration: {s}")
                    }
                }),
            );
        }
        "powerPlants" => l.push(
            "output",
            stat(c, "powerOutput").map(|v| format!("Output: {} PU/s", int(v))),
        ),
        "coolers" => l.push(
            "cooling",
            stat(c, "coolantOutput").map(|v| format!("Cooling: {} CR/s", int(v))),
        ),
        "radars" => {
            // The detection line is left out: `signatureDetection[]` has eight positional
            // entries and M2a found no reliable label for them (spec 5.3).
            l.push(
                "ping",
                stat(c, "cooldownTime").map(|v| format!("Ping cooldown: {}s", d1(v))),
            );
        }
        "missileRacks" => {
            let n = stat(c, "missileCount");
            l.push(
                "slots",
                n.and_then(|n| by_size([(stat(c, "missileSize"), n)]))
                    .map(|s| format!("Slots: {s}")),
            );
            return (!l.out.is_empty()).then_some(l.out);
        }
        cat if COMMON_ONLY.contains(&cat) => {
            let n = common(&mut l, c.power, c.em, c.ir, c.health, c.distortion_max);
            return (n >= 2).then_some(l.out);
        }
        _ => return None,
    }
    common(&mut l, c.power, c.em, c.ir, c.health, c.distortion_max);
    (!l.out.is_empty()).then_some(l.out)
}

/// Tag elements of a component; `class` is read by the caller from the description.
pub fn component_tag(c: &Component, class: Option<String>) -> Option<Vec<Element>> {
    if c.category == "missileRacks" {
        return Some(c.item.size.map(Element::Size).into_iter().collect());
    }
    if !TAGGED.contains(&c.category.as_str()) {
        return None;
    }
    let mut v = Vec::new();
    if let Some(cl) = class {
        v.push(Element::Class(cl));
    }
    v.extend(c.item.size.map(Element::Size));
    v.extend(c.item.grade.map(Element::Grade));
    Some(v)
}

/// Ship weapons and FPS tools that deal no damage (spec 5.4).
pub fn is_tool(w: &Weapon) -> bool {
    matches!(
        w.item.item_type.as_deref(),
        Some("WeaponMining" | "TractorBeam" | "TowingBeam" | "SalvageHead")
    )
}

/// Modes that deal damage: a beam with damage, or a mode with alpha and a rate.
fn damage_modes(w: &Weapon) -> Vec<&FireMode> {
    w.modes
        .iter()
        .filter(|m| m.dps.is_some_and(|d| d > 0.0))
        .collect()
}

/// `Damage: 225.4 Phys`, `Damage: 120 Phys + 30 Energy`, `Damage: 49.5 ×8 Energy`.
fn damage_text(per_shot: &Damage, m: &FireMode) -> Option<String> {
    let mult = m.damage_multiplier.unwrap_or(1.0);
    let pellets = m.pellets.unwrap_or(1.0);
    let mut parts = per_shot.parts();
    parts.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (t, v) = *parts.first()?;
    if pellets > 1.0 {
        return Some(format!(
            "{} ×{} {}",
            d1(per_shot.total() * mult),
            int(pellets),
            damage_label(t)
        ));
    }
    if parts.len() == 1 {
        return Some(format!("{} {}", d1(v * mult), damage_label(t)));
    }
    let (t2, v2) = parts[1];
    Some(format!(
        "{} {} + {} {}",
        d1(v * mult),
        damage_label(t),
        d1(v2 * mult),
        damage_label(t2)
    ))
}

fn mode_list(w: &Weapon, head: &FireMode) -> String {
    let mut labels = vec![head.label()];
    for m in damage_modes(w) {
        let l = m.label();
        if !labels.contains(&l) {
            labels.push(l);
        }
    }
    labels.join(" / ")
}

/// Mining laser modifier labels, by DataCore field name.
const MINING: [(&str, &str); 8] = [
    ("laserInstability", "Instability"),
    ("resistanceModifier", "Resistance"),
    ("optimalChargeWindowSizeModifier", "Optimal window"),
    ("optimalChargeWindowRateModifier", "Charge rate"),
    ("shatterdamageModifier", "Shatter damage"),
    ("catastrophicChargeWindowRateModifier", "Overcharge rate"),
    ("inertMaterialsModifier", "Inert materials"),
    ("clusterFactorModifier", "Clustering"),
];

/// Stat lines of a ship or FPS weapon.
pub fn weapon_lines(w: &Weapon, p: &Profile) -> Option<Vec<String>> {
    let cat = match w.class {
        WeaponClass::Ship => "ship_weapons",
        WeaponClass::Fps => "fps_weapons",
    };
    let mut l = Lines::new(p, cat);
    let head = w.headline().filter(|h| h.dps.is_some_and(|d| d > 0.0));
    if is_tool(w) {
        if w.class == WeaponClass::Fps {
            return None;
        }
        if let Some(m) = &w.mining_modifiers {
            let parts: Vec<String> = MINING
                .iter()
                .filter_map(|(k, label)| {
                    nz(m.get(*k).copied()).map(|v| format!("{label} {}", pct_signed(v)))
                })
                .collect();
            l.push(
                "modifiers",
                (!parts.is_empty()).then(|| format!("Modifiers: {}", parts.join(SEP))),
            );
        }
        common(&mut l, w.power, w.em, w.ir, w.health, None);
        return (!l.out.is_empty()).then_some(l.out);
    }
    let Some(head) = head else {
        // FPS gadgets and other items with no damage-dealing mode get no block.
        if w.class == WeaponClass::Fps {
            return None;
        }
        common(&mut l, w.power, w.em, w.ir, w.health, None);
        return (l.out.len() >= 2).then_some(l.out);
    };
    let ammo = w.ammo.as_ref();
    if head.kind == FireKind::Beam {
        let dmg = head.beam_damage.and_then(|d| {
            let mut parts = d.parts();
            parts.sort_by(|a, b| b.1.total_cmp(&a.1));
            parts.first().map(|(t, _)| damage_label(t))
        });
        l.push(
            "damage",
            head.dps.map(|v| match dmg {
                Some(t) => format!("DPS: {} {t}", d1(v)),
                None => format!("DPS: {}", d1(v)),
            }),
        );
        l.push("rate", Some(format!("Mode: {}", mode_list(w, head))));
        l.push(
            "velocity",
            nz(head.zero_damage_range).map(|r| format!("Range: {}", distance(r))),
        );
    } else {
        let per_shot = ammo.and_then(|a| a.damage).filter(|d| !d.is_empty());
        l.push(
            "damage",
            line([
                per_shot
                    .as_ref()
                    .and_then(|d| damage_text(d, head))
                    .map(|t| format!("Damage: {t}")),
                nz(head.dps).map(|v| format!("DPS: {}", d1(v))),
            ]),
        );
        l.push(
            "rate",
            line([
                nz(head.effective_rate()).map(|r| format!("Fire rate: {} RPM", int(r))),
                Some(format!("Mode: {}", mode_list(w, head))),
            ]),
        );
        l.push(
            "velocity",
            line([
                nz(ammo.and_then(|a| a.speed)).map(|v| format!("Velocity: {} m/s", int(v))),
                nz(ammo.and_then(|a| a.range)).map(|r| format!("Range: {}", distance(r))),
            ]),
        );
    }
    match w.class {
        WeaponClass::Ship => {
            let cap = w
                .capacitor
                .filter(|c| nz(c.shots).is_some() || nz(c.cost_per_shot).is_some());
            match cap {
                Some(c) if ammo.and_then(|a| a.capacity).filter(|v| *v > 0.0).is_none() => {
                    // `requestedRegenPerSec` is in the same unit as the cost per shot;
                    // `maxRegenPerSec` counts shots.
                    let regen = match (nz(c.requested_regen_per_second), nz(c.regen_cooldown)) {
                        (Some(r), Some(t)) => Some(format!("Regen: {}/s after {}s", int(r), d1(t))),
                        (Some(r), None) => Some(format!("Regen: {}/s", int(r))),
                        _ => None,
                    };
                    l.push(
                        "ammo",
                        line([
                            nz(c.cost_per_shot).map(|v| format!("Capacitor: {} per shot", d1(v))),
                            regen,
                        ]),
                    );
                }
                _ => l.push(
                    "ammo",
                    nz(ammo.and_then(|a| a.capacity)).map(|v| format!("Ammo: {}", int(v))),
                ),
            }
            common(&mut l, w.power, w.em, w.ir, w.health, None);
        }
        WeaponClass::Fps => {
            let falloff = ammo.and_then(|a| {
                let dom = a.damage.and_then(|d| d.dominant())?;
                a.falloff.as_ref()?.get(dom).copied()
            });
            l.push(
                "falloff",
                falloff.filter(|f| f.per_meter != 0.0).map(|f| {
                    let mut s = format!(
                        "Full damage to {} m{SEP}then -{} per m",
                        int(f.min_distance),
                        d2(f.per_meter)
                    );
                    if f.floor > 0.0 {
                        s.push_str(&format!("{SEP}Min: {}", d1(f.floor)));
                    }
                    s
                }),
            );
            l.push(
                "magazine",
                line([
                    nz(ammo.and_then(|a| a.capacity)).map(|v| format!("Magazine: {}", int(v))),
                    nz(w.mass).map(|v| format!("Weight: {} kg", d1(v))),
                ]),
            );
        }
    }
    (!l.out.is_empty()).then_some(l.out)
}

/// Tag elements of a ship weapon (spec 5.4). `None`: no tag at all.
pub fn weapon_tag(w: &Weapon, p: &Profile) -> Option<Vec<Element>> {
    if w.class != WeaponClass::Ship {
        return None;
    }
    let size = w.item.size.map(Element::Size);
    if is_tool(w) || w.item.item_type.as_deref() == Some("WeaponDefensive") {
        return p.tags.tool_size_tag.then(|| size.into_iter().collect());
    }
    let rocket = w.item.sub_type.as_deref() == Some("Rocket");
    let mut v = Vec::new();
    if !rocket
        && w.dps.is_some_and(|d| d > 0.0)
        && let Some(t) = &w.damage_type
    {
        v.push(Element::Damage(t.clone()));
    }
    v.extend(size);
    Some(v)
}

/// Stat lines of a missile or bomb (spec 5.6).
pub fn missile_lines(m: &Missile, p: &Profile) -> Option<Vec<String>> {
    let mut l = Lines::new(p, "missiles");
    let dmg = m.damage.filter(|d| !d.is_empty()).map(|d| {
        let parts = d.parts();
        if parts.len() == 1 {
            format!("Damage: {} {}", d1(parts[0].1), damage_label(parts[0].0))
        } else {
            let each: Vec<String> = parts
                .iter()
                .map(|(t, v)| format!("{} {}", d1(*v), damage_label(t)))
                .collect();
            format!("Damage: {} ({})", d1(d.total()), each.join(" + "))
        }
    });
    l.push(
        "damage",
        line([
            dmg,
            nz(m.explosion_radius).map(|r| format!("Blast radius: {} m", int(r))),
        ]),
    );
    if !m.is_bomb {
        l.push(
            "tracking",
            line([
                m.tracking_signal
                    .as_deref()
                    .and_then(tracking_label)
                    .map(|t| format!("Tracking: {t}")),
                nz(m.lock_time).map(|t| format!("Lock: {}s", d1(t))),
            ]),
        );
        l.push(
            "lock_range",
            match (nz(m.lock_range_min), nz(m.lock_range_max)) {
                (Some(a), Some(b)) => Some(format!("Lock range: {}", distance_range(a, b))),
                (None, Some(b)) => Some(format!("Lock range: {}", distance(b))),
                _ => None,
            },
        );
    }
    l.push(
        "speed",
        line([
            nz(m.speed).map(|v| format!("Speed: {} m/s", int(v))),
            nz(m.max_lifetime).map(|v| format!("Lifetime: {}s", d1(v))),
        ]),
    );
    l.push(
        "health",
        nz(m.health).map(|v| format!("Component HP: {}", int(v))),
    );
    (!l.out.is_empty()).then_some(l.out)
}

pub fn tracking_label(t: &str) -> Option<&'static str> {
    match t {
        "Infrared" => Some("Infrared"),
        "Electromagnetic" => Some("Electromagnetic"),
        "CrossSection" => Some("Cross-section"),
        _ => None,
    }
}

pub fn missile_tag(m: &Missile) -> Vec<Element> {
    let mut v = Vec::new();
    if m.is_bomb {
        v.push(Element::Bomb);
    } else if let Some(t) = &m.tracking_signal {
        v.push(Element::Tracking(t.clone()));
    }
    v.extend(m.item.size.map(Element::Size));
    v
}
