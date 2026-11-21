//! Combat numbers of an explorer record, from the build's GameData (the extraction cache):
//! a ship's total weapon DPS and its stock guns and missiles, a weapon's headline numbers and
//! fire modes, a missile's damage and lock. Pure; nothing here reads the P4K. Loadout items
//! are named by record short name, so each row resolves to the item's GUID (to link it) and
//! its readable name through a [`Lookup`].

use super::readable::{KnownRecord, LabelKind, Lookup, record_label};
use super::sections::Stat;
use super::sections::stats::{missile_stats, stat};
use crate::model::{GameData, LoadoutItem, Weapon, WeaponClass, short_name};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CombatKind {
    Ship,
    Weapon,
    Missile,
}

/// A record a row names: the raw short name, what it reads as, and its GUID when known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatRef {
    /// Record short name as the loadout names it.
    pub item: String,
    /// Display name, else the short name.
    pub name: String,
    /// Lower case; `None` when no record of that name is known.
    pub guid: Option<String>,
}

/// One kind of stock gun or missile (identical siblings counted).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatRow {
    pub item: CombatRef,
    pub size: Option<f64>,
    pub count: f64,
    /// Guns: the mount or turret they sit in; missiles: the rack.
    pub mount: Option<CombatRef>,
    /// Guns: DPS of one; missiles: damage of one.
    pub each: Option<f64>,
    /// `each` × `count`.
    pub total: Option<f64>,
}

/// One fire mode of a weapon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatMode {
    /// `Semi`, `Auto`, `Burst 3`, `Charged`, `Beam` or the mode's own name.
    pub label: String,
    /// Rounds per minute (a burst's sustained rate when it has one).
    pub rate: Option<f64>,
    pub alpha: Option<f64>,
    pub dps: Option<f64>,
    pub heat_per_shot: Option<f64>,
    pub charge_time: Option<f64>,
    pub pellets: Option<f64>,
}

/// A fact without a number (`Damage type: energy`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatFact {
    pub label: String,
    pub text: String,
}

/// The combat card of one record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Combat {
    pub kind: CombatKind,
    /// Headline numbers, in order.
    pub stats: Vec<Stat>,
    pub facts: Vec<CombatFact>,
    /// Ships: stock guns, highest total DPS first.
    pub weapons: Vec<CombatRow>,
    /// Ships: stock missiles, highest total damage first.
    pub missiles: Vec<CombatRow>,
    /// Weapons: their fire modes.
    pub modes: Vec<CombatMode>,
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Records of the build's items by lower-case short name: weapons, missiles and components
/// (mounts, turrets and racks are components).
fn items(gd: &GameData) -> HashMap<String, KnownRecord> {
    let mut m = HashMap::new();
    let ids = gd
        .weapons
        .iter()
        .map(|w| &w.item)
        .chain(gd.missiles.iter().map(|x| &x.item))
        .chain(gd.components.iter().map(|c| &c.item));
    for it in ids {
        m.entry(it.short_name().to_lowercase())
            .or_insert_with(|| KnownRecord {
                guid: (!it.id.is_empty()).then(|| it.id.to_lowercase()),
                name: it.record_name.clone(),
                kind: LabelKind::Record,
                display: it.loc.name.clone(),
            });
    }
    m
}

/// What a loadout short name reads as. The lookup's own record wins (it may know more names),
/// else the GameData item, else the raw name.
fn reference(l: &dyn Lookup, known: &HashMap<String, KnownRecord>, item: &str) -> CombatRef {
    let rec = known
        .get(&item.to_lowercase())
        .cloned()
        .or_else(|| l.by_name(item))
        .map(|k| k.guid.as_deref().and_then(|g| l.by_guid(g)).unwrap_or(k));
    match rec {
        Some(r) => CombatRef {
            item: item.to_string(),
            name: record_label(l, &r).text,
            guid: r.guid.map(|g| g.to_lowercase()),
        },
        None => CombatRef {
            item: item.to_string(),
            name: short_name(item).to_string(),
            guid: None,
        },
    }
}

fn row(
    l: &dyn Lookup,
    known: &HashMap<String, KnownRecord>,
    it: &LoadoutItem,
    mount: Option<&String>,
    each: Option<f64>,
) -> CombatRow {
    let count = it.count();
    let each = each.filter(|v| v.is_finite());
    CombatRow {
        item: reference(l, known, &it.item),
        size: it.size,
        count,
        mount: mount
            .filter(|m| !m.is_empty())
            .map(|m| reference(l, known, m)),
        each: each.map(round2),
        total: each.map(|e| round2(e * count)),
    }
}

/// Highest total first, then by name.
fn sort_rows(rows: &mut [CombatRow]) {
    rows.sort_by(|a, b| {
        b.total
            .unwrap_or(0.0)
            .total_cmp(&a.total.unwrap_or(0.0))
            .then_with(|| a.item.name.cmp(&b.item.name))
    });
}

fn facts(pairs: &[(&str, Option<&str>)]) -> Vec<CombatFact> {
    pairs
        .iter()
        .filter_map(|(label, text)| {
            let t = text.filter(|t| !t.trim().is_empty())?;
            Some(CombatFact {
                label: label.to_string(),
                text: t.to_string(),
            })
        })
        .collect()
}

fn weapon_combat(w: &Weapon) -> Combat {
    let stats = [
        stat("dps", "DPS", w.dps, None),
        stat("alpha", "Alpha", w.alpha, None),
        stat("rate", "Fire rate", w.fire_rate, Some("rpm")),
        stat("range", "Range", w.range, Some("m")),
        stat("speed", "Speed", w.speed, Some("m/s")),
        stat("ammo", "Ammo", w.ammo_capacity, None),
        stat("power", "Power", w.power, None),
        stat("heat", "Heat/shot", w.heat_per_shot, None),
        stat("em", "EM", w.em, None),
        stat("ir", "IR", w.ir, None),
    ];
    let class = match w.class {
        WeaponClass::Ship => "Ship weapon",
        WeaponClass::Fps => "FPS weapon",
    };
    Combat {
        kind: CombatKind::Weapon,
        stats: stats.into_iter().flatten().collect(),
        facts: facts(&[
            ("Class", Some(class)),
            ("Damage type", w.damage_type.as_deref()),
        ]),
        weapons: vec![],
        missiles: vec![],
        modes: w
            .modes
            .iter()
            .map(|m| CombatMode {
                label: m.label(),
                rate: m.effective_rate().map(round2),
                alpha: m.alpha,
                dps: m.dps,
                heat_per_shot: m.heat_per_shot,
                charge_time: m.charge_time,
                pellets: m.pellets.filter(|p| *p > 1.0),
            })
            .collect(),
    }
}

/// The combat card of the record `guid` (any case), or `None` when it is not a ship, weapon
/// or missile of this GameData, or has nothing to show.
pub fn combat_of(gd: &GameData, l: &dyn Lookup, guid: &str) -> Option<Combat> {
    let g = guid.trim();
    if g.is_empty() {
        return None;
    }
    let is = |id: &str| id.eq_ignore_ascii_case(g);
    if let Some(s) = gd.ships.iter().find(|s| is(&s.id)) {
        let known = items(gd);
        let (mut weapons, mut missiles) = (vec![], vec![]);
        if let Some(lo) = &s.loadout {
            for it in &lo.weapons {
                weapons.push(row(l, &known, it, it.mount.as_ref(), it.dps));
            }
            for it in &lo.missiles {
                missiles.push(row(l, &known, it, it.rack.as_ref(), it.damage));
            }
        }
        sort_rows(&mut weapons);
        sort_rows(&mut missiles);
        let t = s.totals.as_ref();
        let stats: Vec<Stat> = [
            stat("dps", "Weapon DPS", t.and_then(|t| t.weapon_dps), None),
            stat("weapons", "Guns", t.and_then(|t| t.weapons), None),
            stat("missiles", "Missiles", t.and_then(|t| t.missiles), None),
            stat(
                "missileDamage",
                "Missile damage",
                t.and_then(|t| t.missile_damage),
                None,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        // A ship without guns or missiles has no card.
        if weapons.is_empty() && missiles.is_empty() && stats.iter().all(|s| s.value == 0.0) {
            return None;
        }
        return Some(Combat {
            kind: CombatKind::Ship,
            stats,
            facts: vec![],
            weapons,
            missiles,
            modes: vec![],
        });
    }
    if let Some(w) = gd.weapons.iter().find(|w| is(&w.item.id)) {
        let c = weapon_combat(w);
        return (!c.stats.is_empty() || !c.modes.is_empty()).then_some(c);
    }
    if let Some(m) = gd.missiles.iter().find(|m| is(&m.item.id)) {
        let mut stats = missile_stats(m);
        stats.extend([
            stat("speed", "Speed", m.speed, Some("m/s")),
            stat("angle", "Lock angle", m.locking_angle, Some("°")),
            stat("radius", "Blast radius", m.explosion_radius, Some("m")),
        ]);
        let stats: Vec<Stat> = stats.into_iter().flatten().collect();
        if stats.is_empty() {
            return None;
        }
        return Some(Combat {
            kind: CombatKind::Missile,
            stats,
            facts: facts(&[
                ("Kind", Some(if m.is_bomb { "Bomb" } else { "Missile" })),
                ("Damage type", m.damage_type.as_deref()),
                ("Tracking", m.tracking_signal.as_deref()),
            ]),
            weapons: vec![],
            missiles: vec![],
            modes: vec![],
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev::readable::{MapLookup, NoLookup};
    use crate::model::{
        FireKind, FireMode, ItemIdentity, Loadout, LocKeys, Missile, Ship, ShipLoc, Totals,
    };
    use std::sync::Arc;

    fn ident(guid: &str, name: &str, key: Option<&str>) -> ItemIdentity {
        ItemIdentity {
            record_name: format!("EntityClassDefinition.{name}"),
            id: guid.into(),
            loc: LocKeys {
                name: key.map(str::to_string),
                ..LocKeys::default()
            },
            ..ItemIdentity::default()
        }
    }

    fn fixture() -> GameData {
        let mut gun = Weapon {
            item: ident(
                "AAAA-0001",
                "KLWE_LaserRepeater_S3",
                Some("@item_NameRepeater"),
            ),
            class: WeaponClass::Ship,
            power: Some(3.0),
            modes: vec![
                FireMode {
                    name: "Rapid".into(),
                    kind: FireKind::Rapid,
                    fire_rate: Some(600.0),
                    heat_per_shot: Some(4.5),
                    ..FireMode::default()
                },
                FireMode {
                    name: "Burst".into(),
                    kind: FireKind::Burst,
                    fire_rate: Some(900.0),
                    sustained_fire_rate: Some(450.0),
                    shot_count: Some(3.0),
                    ..FireMode::default()
                },
            ],
            ..Weapon::default()
        };
        for m in &mut gun.modes {
            m.finish(Some(20.0));
        }
        gun.derive_headline();
        gun.damage_type = Some("energy".into());
        let missile = Missile {
            item: ident("BBBB-0002", "MISL_S02_IR_Arrester", None),
            damage_total: Some(2500.0),
            lock_range_max: Some(6000.0),
            lock_time: Some(2.5),
            tracking_signal: Some("Infrared".into()),
            ..Missile::default()
        };
        let lo = Loadout {
            weapons: vec![
                LoadoutItem {
                    item: "KLWE_LaserRepeater_S3".into(),
                    size: Some(3.0),
                    count: Some(2.0),
                    item_type: Some("WeaponGun".into()),
                    mount: Some("MRAI_Gimbal_S3".into()),
                    dps: Some(200.0),
                    ..LoadoutItem::default()
                },
                LoadoutItem {
                    item: "UNKN_Mystery_S1".into(),
                    size: Some(1.0),
                    dps: Some(50.0),
                    ..LoadoutItem::default()
                },
            ],
            missiles: vec![LoadoutItem {
                item: "MISL_S02_IR_Arrester".into(),
                size: Some(2.0),
                count: Some(4.0),
                rack: Some("MRCK_S02_Rack".into()),
                damage: Some(2500.0),
                ..LoadoutItem::default()
            }],
            ..Loadout::default()
        };
        let ship = |id: &str, name: &str, lo: Option<Loadout>, dps: Option<f64>| Ship {
            record_name: format!("EntityClassDefinition.{name}"),
            id: id.into(),
            loc: ShipLoc::default(),
            loadout: lo,
            totals: Some(Totals {
                weapon_dps: dps,
                ..Totals::default()
            }),
            ..Ship::default()
        };
        GameData {
            weapons: vec![gun],
            missiles: vec![missile],
            ships: vec![
                ship("CCCC-0003", "AEGS_Gladius", Some(lo), Some(450.0)),
                ship("DDDD-0004", "DRAK_Hauler", None, None),
            ],
            ..GameData::default()
        }
    }

    fn lookup(gd: &GameData) -> MapLookup {
        MapLookup::from_gamedata(gd).with_stock(Arc::new(|k: &str| {
            (k.eq_ignore_ascii_case("item_NameRepeater")).then(|| "Laser Repeater".to_string())
        }))
    }

    #[test]
    fn a_ship_lists_its_guns_and_missiles_with_links_and_totals() {
        let gd = fixture();
        let c = combat_of(&gd, &lookup(&gd), "cccc-0003").unwrap();
        assert_eq!(c.kind, CombatKind::Ship);
        assert_eq!(c.stats[0].key, "dps");
        assert_eq!(c.stats[0].value, 450.0);
        let w = &c.weapons[0];
        assert_eq!(w.item.name, "Laser Repeater");
        assert_eq!(w.item.guid.as_deref(), Some("aaaa-0001"));
        assert_eq!((w.count, w.each, w.total), (2.0, Some(200.0), Some(400.0)));
        // The mount is not a known record: its short name stays, unlinked.
        let mount = w.mount.as_ref().unwrap();
        assert_eq!(
            (mount.name.as_str(), mount.guid.as_ref()),
            ("MRAI_Gimbal_S3", None)
        );
        // Unknown guns stay by short name, sorted after the bigger total.
        assert_eq!(c.weapons[1].item.name, "UNKN_Mystery_S1");
        assert_eq!(c.weapons[1].item.guid, None);
        assert_eq!(c.weapons[1].total, Some(50.0));
        let m = &c.missiles[0];
        assert_eq!(m.item.guid.as_deref(), Some("bbbb-0002"));
        assert_eq!(m.item.name, "MISL_S02_IR_Arrester");
        assert_eq!((m.each, m.total), (Some(2500.0), Some(10000.0)));
        assert_eq!(m.mount.as_ref().unwrap().item, "MRCK_S02_Rack");
    }

    #[test]
    fn a_weapon_shows_its_headline_and_modes() {
        let gd = fixture();
        let c = combat_of(&gd, &NoLookup, "AAAA-0001").unwrap();
        assert_eq!(c.kind, CombatKind::Weapon);
        let v = |k: &str| c.stats.iter().find(|s| s.key == k).map(|s| s.value);
        assert_eq!(v("dps"), Some(200.0));
        assert_eq!(v("alpha"), Some(20.0));
        assert_eq!(v("rate"), Some(600.0));
        assert_eq!(v("power"), Some(3.0));
        assert_eq!(v("range"), None);
        assert!(c.facts.iter().any(|f| f.text == "energy"));
        assert_eq!(c.modes.len(), 2);
        assert_eq!(c.modes[0].label, "Auto");
        assert_eq!(c.modes[1].label, "Burst 3");
        assert_eq!(c.modes[1].rate, Some(450.0));
        assert_eq!(c.modes[1].dps, Some(150.0));
    }

    #[test]
    fn a_missile_shows_damage_and_lock() {
        let gd = fixture();
        let c = combat_of(&gd, &NoLookup, "bbbb-0002").unwrap();
        assert_eq!(c.kind, CombatKind::Missile);
        let keys: Vec<&str> = c.stats.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["damage", "lock", "lockTime"]);
        assert!(c.facts.iter().any(|f| f.text == "Infrared"));
    }

    #[test]
    fn records_without_combat_numbers_have_no_card() {
        let gd = fixture();
        assert_eq!(combat_of(&gd, &NoLookup, "dddd-0004"), None);
        assert_eq!(combat_of(&gd, &NoLookup, "eeee-0005"), None);
        assert_eq!(combat_of(&gd, &NoLookup, ""), None);
        assert_eq!(
            combat_of(&GameData::default(), &NoLookup, "cccc-0003"),
            None
        );
    }
}
