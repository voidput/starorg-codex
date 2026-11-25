//! Blueprint finder (addendum §7.19): every blueprint of a build with what the app knows
//! about it (kind, type, manufacturer, size, grade), merged with the owned sets of every
//! character of one or more channels.
//!
//! Ownership stays scoped exactly as stored (§3): per channel data folder, per character.
//! [`build`] only reads; it never moves an entry between channels or characters. Rows of
//! several channels are merged by blueprint id, and each owner keeps its channel, so the
//! "All channels" view can say where a blueprint is owned.
//!
//! Sorting, filtering and facet counts run in the app on these rows (`app/src/lib/bpfinder.ts`);
//! the rows here carry every field they need.

use super::{Characters, OwnedSet, normalize};
use crate::enhance::format::{int, pct};
use crate::enhance::missions::{Odds, groups as title_groups, odds_by, record_label, usable};
use crate::enhance::stock_name;
use crate::enhance::tags::grade_letter;
use crate::groups::GroupIndex;
use crate::ini::{StockIni, fold_key};
use crate::model::{Blueprint, Contract, GameData, ItemIdentity, is_placeholder_name};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// What a blueprint makes, from its record path and category (stable across builds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    ShipComponent,
    ShipWeapon,
    FpsWeapon,
    Armor,
    FpsAmmo,
    MissionItem,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ShipComponent => "Ship component",
            Self::ShipWeapon => "Ship weapon",
            Self::FpsWeapon => "FPS weapon",
            Self::Armor => "Armor",
            Self::FpsAmmo => "FPS ammo",
            Self::MissionItem => "Mission item",
            Self::Other => "Other",
        }
    }
}

/// One blueprint of one build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogItem {
    /// Blueprint record GUID (or the record short name when it has none).
    pub id: String,
    /// Record short name, `BP_CRAFT_AMRS_LaserCannon_S6`.
    pub record: String,
    /// English display name of the produced item.
    pub name: String,
    pub kind: Kind,
    /// `Cooler`, `Laser cannon`, `Rifle`, `Arms`, ...
    pub type_label: Option<String>,
    /// Armor weight: `Light`, `Medium`, `Heavy`.
    pub class: Option<String>,
    /// Upper-case code, `AEGS`.
    pub manufacturer: Option<String>,
    /// Stock `manufacturer_Name<code>` text.
    pub manufacturer_name: Option<String>,
    pub size: Option<u8>,
    /// `A`..`D` (ship components only).
    pub grade: Option<String>,
    /// The missions that can award it ("Earn it"); `None` = this build's contracts were
    /// not read (they come from local extraction only).
    pub earn: Option<Vec<Earn<Mission>>>,
}

/// A mission that can award blueprints: the contracts that share one title (the in-game
/// title tags and MISSION DETAILS are computed over the same group).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Mission {
    /// In-game title (stock text; `~mission(Location)` reads `[Location]`). `None`: the
    /// contract has no readable title.
    pub title: Option<String>,
    /// Contract debug name (`Name (+2 more)` when several contracts share the title).
    pub record: String,
    /// Contract generator record short name of the first contract (an Explorer search).
    pub generator: String,
    /// Faction reputation records its contracts' success rewards pay
    /// (`factionreputation_x`), in first-seen order.
    pub factions: Vec<String>,
    /// Those factions readable, one per entry of `factions`: the game's own name when the
    /// record has one, else the record name in words.
    pub reputation: Vec<String>,
    /// Star systems its locality prerequisites name; empty = not known.
    pub systems: Vec<String>,
    /// MISSION DETAILS `Type:` words: `Chain`, `Once only`, each only when every contract
    /// of the title is one.
    pub kinds: Vec<String>,
    /// MISSION DETAILS `Requires:` standing name.
    pub requires: Option<String>,
    /// Contracts with this title.
    pub contracts: u32,
}

/// How sure one blueprint is from one mission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Award {
    /// Every contract of the title awards a pool at 100% and the pool holds only it.
    Guaranteed,
    /// Every contract awards a pool at 100%, but it is one draw of several.
    GuaranteedPool,
    /// Awarded by chance, or only by some contracts of the title.
    Chance,
}

/// How one mission awards one blueprint. `M` is the mission itself in a build's catalogue
/// and its index in [`Finder::missions`] on a finder row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Earn<M> {
    pub mission: M,
    pub award: Award,
    /// The MISSION DETAILS `Blueprint reward:` value (the pool's odds, as in game):
    /// `Guaranteed`, `Some variants only` (not every contract with the title awards it) or
    /// `25%`.
    pub odds: String,
    /// Contracts of the title that can award it (of [`Mission::contracts`]).
    pub variants: u32,
    /// Blueprints in the pool it is drawn from: `6`, or `4–6` over several pools.
    pub pool: String,
    /// Its share of the pool by weight (`17%`, `8–17%`); `None` when weights are missing.
    pub pick: Option<String>,
    /// Channels whose build has this (finder rows; empty in a build's catalogue).
    pub channels: Vec<String>,
}

/// Rename table for record path folders (`vehiclegear/<x>`, `fpsgear/weapons/<x>`, ...).
const TYPE_LABELS: &[(&str, &str)] = &[
    ("cooler", "Cooler"),
    ("powerplant", "Power plant"),
    ("shield", "Shield"),
    ("radar", "Radar"),
    ("quantumdrive", "Quantum drive"),
    ("mininglaser", "Mining laser"),
    ("tractorbeam", "Tractor beam"),
    ("refuelling", "Refuelling nozzle"),
    ("salvage", "Salvage head"),
    ("pistol", "Pistol"),
    ("rifle", "Rifle"),
    ("sniper", "Sniper rifle"),
    ("smg", "SMG"),
    ("shotgun", "Shotgun"),
    ("lmg", "LMG"),
    ("laser", "Laser"),
    ("ballistic", "Ballistic"),
    ("distortion", "Distortion"),
    ("electron", "Electron"),
    ("plasma", "Plasma"),
    ("cannon", "cannon"),
    ("repeater", "repeater"),
    ("gatling", "gatling"),
    ("scatter", "scattergun"),
    ("massdriver", "mass driver"),
    ("undersuit", "Undersuit"),
    ("flightsuit", "Flight suit"),
];

fn type_label(seg: &str) -> Option<&'static str> {
    TYPE_LABELS.iter().find(|(k, _)| *k == seg).map(|(_, v)| *v)
}

/// `AttachDef.Type` values of items whose record path says nothing.
const ITEM_TYPES: &[(&str, &str)] = &[
    ("Cooler", "Cooler"),
    ("PowerPlant", "Power plant"),
    ("Shield", "Shield"),
    ("Radar", "Radar"),
    ("QuantumDrive", "Quantum drive"),
    ("WeaponMining", "Mining laser"),
    ("TractorBeam", "Tractor beam"),
    ("WeaponGun", "Gun"),
    ("WeaponPersonal", "Personal weapon"),
    ("Missile", "Missile"),
];

fn item_type(item: Option<&ItemIdentity>) -> Option<String> {
    let t = item?.item_type.as_deref()?;
    Some(
        ITEM_TYPES
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(t))
            .map_or(t, |(_, v)| *v)
            .to_string(),
    )
}

/// Types the record path does not say (`$templates`, bare `vehiclegear`), by a word in the
/// record name. Checked before the path.
const RECORD_TYPES: &[(&str, &str)] = &[
    ("_hmg_", "HMG"),
    ("crossbow", "Crossbow"),
    ("shipmining_pod", "Ore pod"),
    ("mining_modules", "Mining module"),
    ("tachyoncannon", "Tachyon cannon"),
    ("neutroncannon", "Neutron cannon"),
    ("neutronrepeater", "Neutron repeater"),
    ("lasergatling", "Laser gatling"),
];

/// Armor slots, by a word in the record name (`ccc_medium_armor_arms_01`).
const ARMOR_SLOTS: &[(&str, &str)] = &[
    ("helmet", "Helmet"),
    ("_core", "Torso"),
    ("_arms", "Arms"),
    ("_legs", "Legs"),
    ("backpack", "Backpack"),
    ("undersuit", "Undersuit"),
];

/// Armor slots by strings group id (with the loc-reference pass, the group is exact).
const ARMOR_GROUPS: &[(&str, &str)] = &[
    ("armor/helmets", "Helmet"),
    ("armor/torso", "Torso"),
    ("armor/arms", "Arms"),
    ("armor/legs", "Legs"),
    ("armor/undersuits", "Undersuit"),
    ("armor/backpacks", "Backpack"),
];

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// `VehicleComponentS2` / `VehicleWeaponsS3` -> 2 / 3.
fn category_size(category: &str) -> Option<u8> {
    let (_, n) = category.rsplit_once('S')?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .then(|| n.parse().ok())
        .flatten()
}

/// Kind, type and armor class of one blueprint. `group` is the strings group of the
/// produced item's name key, when known.
pub fn classify(bp: &Blueprint, item: Option<&ItemIdentity>, group: Option<&str>) -> Classified {
    let path = bp.path.to_ascii_lowercase();
    let rel = path
        .split_once("blueprints/crafting/")
        .map(|(_, r)| r)
        .unwrap_or(&path);
    let segs: Vec<&str> = rel.split('/').filter(|s| !s.starts_with('$')).collect();
    // The last segment is the record file itself.
    let dirs = &segs[..segs.len().saturating_sub(1)];
    let cat = bp.category.as_deref().unwrap_or_default();
    let record = bp.short_name().to_ascii_lowercase();
    let kind = match (dirs.first().copied(), dirs.get(1).copied()) {
        (Some("fpsgear"), Some("armour" | "armor")) => Kind::Armor,
        (Some("fpsgear"), Some("weapons")) => Kind::FpsWeapon,
        (Some("fpsgear"), Some("ammo")) => Kind::FpsAmmo,
        (Some("vehiclegear"), Some("weapons")) => Kind::ShipWeapon,
        (Some("vehiclegear"), _) => Kind::ShipComponent,
        (Some("missionitems"), _) => Kind::MissionItem,
        _ if cat.starts_with("FPSArmour") => Kind::Armor,
        _ if cat.starts_with("FPSWeapon") => Kind::FpsWeapon,
        _ if cat.starts_with("VehicleComponent") => Kind::ShipComponent,
        _ if cat.starts_with("VehicleWeapon") => Kind::ShipWeapon,
        _ if cat == "MissionItem" => Kind::MissionItem,
        _ => Kind::Other,
    };
    let (ty, class) = match kind {
        Kind::Armor => {
            let slot = group
                .and_then(|g| ARMOR_GROUPS.iter().find(|(id, _)| *id == g))
                .map(|(_, l)| *l)
                .or_else(|| dirs.get(2).copied().and_then(type_label))
                .or_else(|| {
                    ARMOR_SLOTS
                        .iter()
                        .find(|(w, _)| record.contains(w))
                        .map(|(_, l)| *l)
                });
            let class = dirs
                .iter()
                .find(|s| matches!(**s, "light" | "medium" | "heavy"))
                .map(|s| cap(s));
            (slot.map(str::to_string), class)
        }
        Kind::FpsWeapon => (
            dirs.get(2)
                .copied()
                .and_then(type_label)
                .map(str::to_string),
            None,
        ),
        Kind::FpsAmmo => (
            dirs.get(2)
                .copied()
                .and_then(type_label)
                .map(|t| format!("{t} ammo")),
            None,
        ),
        Kind::ShipWeapon => {
            let t = match (
                dirs.get(2).copied().and_then(type_label),
                dirs.get(3).copied().and_then(type_label),
            ) {
                (Some(a), Some(b)) => Some(format!("{a} {b}")),
                (Some(a), None) => Some(a.to_string()),
                _ => item_type(item),
            };
            (t, None)
        }
        Kind::ShipComponent => (
            dirs.get(1)
                .copied()
                .and_then(type_label)
                .map(str::to_string)
                .or_else(|| item_type(item)),
            None,
        ),
        _ => (None, None),
    };
    let ty = match kind {
        Kind::FpsWeapon | Kind::ShipWeapon | Kind::ShipComponent => RECORD_TYPES
            .iter()
            .find(|(w, _)| record.contains(w))
            .map(|(_, l)| l.to_string())
            .or(ty),
        _ => ty,
    };
    Classified { kind, ty, class }
}

/// [`classify`]'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    pub kind: Kind,
    pub ty: Option<String>,
    pub class: Option<String>,
}

/// Every named blueprint of a build. `groups` (the strings grouping of the same build, see
/// addendum 8) adds the manufacturer and the armor slot of items `GameData` does not list
/// (armor, undersuits).
pub fn catalog(gd: &GameData, stock: &StockIni, groups: Option<&GroupIndex>) -> Vec<CatalogItem> {
    let mut items: HashMap<String, &ItemIdentity> = HashMap::new();
    for i in gd
        .components
        .iter()
        .map(|c| &c.item)
        .chain(gd.weapons.iter().map(|w| &w.item))
        .chain(gd.missiles.iter().map(|m| &m.item))
    {
        items.insert(i.short_name().to_ascii_lowercase(), i);
    }
    // Name key -> strings row, for the keys blueprints use (rows follow `entries()` order).
    let mut rows: HashMap<String, usize> = HashMap::new();
    if groups.is_some() {
        let wanted: std::collections::HashSet<String> = gd
            .blueprints
            .iter()
            .filter_map(|b| b.produces_name_key.as_deref().map(fold_key))
            .collect();
        for (row, (_, key, _)) in stock.entries().enumerate() {
            let k = fold_key(key);
            if wanted.contains(&k) {
                rows.entry(k).or_insert(row);
            }
        }
    }
    let earn = earn_index(gd, stock);
    let maker_name = |code: &str| {
        stock
            .value(&format!("manufacturer_Name{code}"))
            .map(str::trim)
            .filter(|v| !is_placeholder_name(Some(v)))
            .map(str::to_string)
    };
    gd.blueprints
        .iter()
        .filter_map(|bp| {
            let name = stock_name(stock, bp.produces_name_key.as_deref())?;
            let item = bp
                .produces
                .as_deref()
                .and_then(|p| items.get(&p.to_ascii_lowercase()).copied());
            let row = bp
                .produces_name_key
                .as_deref()
                .and_then(|k| rows.get(&fold_key(k)).copied());
            let group = groups.zip(row).map(|(g, r)| g.group(r).id);
            let k = classify(bp, item, group);
            let manufacturer = item
                .and_then(|i| i.manufacturer.clone())
                .filter(|m| !m.is_empty())
                .or_else(|| {
                    groups
                        .zip(row)
                        .and_then(|(g, r)| g.manufacturer(r))
                        .map(|m| m.code.clone())
                })
                .map(|m| m.to_ascii_uppercase());
            let size = bp
                .category
                .as_deref()
                .and_then(category_size)
                .or_else(|| {
                    item.and_then(|i| i.size)
                        .filter(|s| (0..=12).contains(s))
                        .map(|s| s as u8)
                })
                .filter(|_| k.kind != Kind::Armor);
            let grade = item
                .and_then(|i| i.grade)
                .filter(|_| k.kind == Kind::ShipComponent)
                .and_then(grade_letter)
                .map(String::from);
            Some(CatalogItem {
                id: if bp.id.is_empty() {
                    bp.short_name().to_string()
                } else {
                    bp.id.clone()
                },
                record: bp.short_name().to_string(),
                name: name.to_string(),
                kind: k.kind,
                type_label: k.ty,
                class: k.class,
                manufacturer_name: manufacturer.as_deref().and_then(maker_name),
                manufacturer,
                size,
                grade,
                earn: earn.as_ref().map(|e| {
                    e.get(&bp.short_name().to_ascii_lowercase())
                        .cloned()
                        .unwrap_or_default()
                }),
            })
        })
        .collect()
}

/// `~mission(Location|Address)` as `[Location]`, markup tags (`<EM4>`) dropped.
pub fn readable_title(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(['~', '<']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(arg) = tail.strip_prefix("~mission(")
            && let Some(end) = arg.find(')')
        {
            let name = arg[..end].split('|').next().unwrap_or_default();
            out.push_str(&format!("[{name}]"));
            rest = &arg[end + 1..];
        } else if tail.starts_with('<')
            && let Some(end) = tail.find('>')
        {
            rest = &tail[end + 1..];
        } else {
            out.push_str(&tail[..1]);
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A faction reputation record in words, for one the game gives no name:
/// `factionreputation_lawful_ftl` -> `FTL`. The record prefix and the lawful / unlawful
/// marker go; short words without a vowel are acronyms.
pub fn faction_label(record: &str) -> String {
    let lower = record.to_ascii_lowercase();
    let mut rest = [
        "factionreputation_",
        "faction_reputation_",
        "factionreputation",
    ]
    .iter()
    .find_map(|p| lower.strip_prefix(p))
    .unwrap_or(&lower);
    for p in ["lawful_", "unlawful_"] {
        if let Some(r) = rest.strip_prefix(p).filter(|r| !r.is_empty()) {
            rest = r;
        }
    }
    rest.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            if w.len() <= 3 && !w.contains(['a', 'e', 'i', 'o', 'u']) {
                w.to_ascii_uppercase()
            } else {
                cap(w)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `N` or `A–B`.
fn span(lo: u32, hi: u32) -> String {
    if lo == hi {
        lo.to_string()
    } else {
        format!("{lo}–{hi}")
    }
}

/// A blueprint's pools under one title: their sizes and its weight shares.
fn pool_odds(
    in_pools: &BTreeSet<&str>,
    pools: &HashMap<String, Vec<(String, Option<f64>)>>,
    bp: &str,
) -> (String, Option<String>) {
    let mut sizes = Vec::new();
    let mut shares = Vec::new();
    let mut weighed = true;
    for p in in_pools {
        let Some(list) = pools.get(*p) else { continue };
        sizes.push(list.len() as u32);
        let total: Option<f64> = list.iter().map(|(_, w)| *w).sum();
        let mine = list.iter().find(|(b, _)| b == bp).and_then(|(_, w)| *w);
        match (mine, total) {
            (Some(m), Some(t)) if t > 0.0 => shares.push(m / t * 100.0),
            _ => weighed = false,
        }
    }
    let pick = (weighed && !shares.is_empty()).then(|| {
        let lo = shares.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = shares.iter().copied().fold(0.0, f64::max);
        if int(lo) == int(hi) {
            pct(hi)
        } else {
            format!("{}–{}", int(lo), pct(hi))
        }
    });
    let size = span(
        sizes.iter().copied().min().unwrap_or(0),
        sizes.iter().copied().max().unwrap_or(0),
    );
    (size, pick)
}

/// The missions that can award each blueprint, by blueprint record short name (lower case),
/// guaranteed rewards first. Computed once per build with the catalogue (and cached with
/// it). `None` when the build's contracts were not read.
pub fn earn_index(gd: &GameData, stock: &StockIni) -> Option<HashMap<String, Vec<Earn<Mission>>>> {
    if gd.contracts.is_empty() {
        return None;
    }
    // Pool (lower case) -> its blueprints (lower case) with their weights.
    let mut pools: HashMap<String, Vec<(String, Option<f64>)>> = HashMap::new();
    for p in &gd.blueprint_pools {
        let list = pools.entry(p.name.to_ascii_lowercase()).or_default();
        for e in &p.blueprints {
            let b = e.blueprint.to_ascii_lowercase();
            // A blueprint listed twice in a pool: one entry with both weights.
            match list.iter_mut().find(|(x, _)| *x == b) {
                Some((_, w)) => *w = w.zip(e.weight).map(|(a, b)| a + b),
                None => list.push((b, e.weight)),
            }
        }
    }
    let contracts: Vec<&Contract> = gd.contracts.iter().filter(|c| usable(c)).collect();
    let mut groups: Vec<(Option<String>, Vec<&Contract>)> =
        title_groups(&contracts, |c| c.title_key.as_ref())
            .into_values()
            .map(|(key, g)| (stock_name(stock, Some(&key)).map(readable_title), g))
            .collect();
    // A contract with no title key is its own mission, known by its record name.
    for c in &contracts {
        if c.title_key
            .as_deref()
            .and_then(crate::model::loc_key)
            .is_none()
            && !c.blueprint_rewards.is_empty()
        {
            groups.push((None, vec![c]));
        }
    }
    let standings: HashMap<String, (f64, Option<&str>)> = gd
        .reputation_standings
        .iter()
        .map(|s| {
            (
                s.name.to_ascii_lowercase(),
                (
                    s.min_reputation.unwrap_or(0.0),
                    s.display_name_key.as_deref(),
                ),
            )
        })
        .collect();
    let mut out: HashMap<String, Vec<Earn<Mission>>> = HashMap::new();
    for (title, group) in &groups {
        // Blueprint -> the pools of this title that hold it.
        let mut held: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for c in group {
            for r in &c.blueprint_rewards {
                let Some((name, list)) = pools.get_key_value(&r.pool.to_ascii_lowercase()) else {
                    continue;
                };
                for (b, _) in list {
                    held.entry(b.as_str()).or_default().insert(name.as_str());
                }
            }
        }
        if held.is_empty() {
            continue;
        }
        // Every faction the title's success rewards pay, with the game's name when known.
        let mut factions: Vec<(String, Option<String>)> = Vec::new();
        for r in group
            .iter()
            .flat_map(|c| &c.reputation)
            .filter(|r| r.on_success && r.amount > 0.0)
        {
            let Some(f) = &r.faction else { continue };
            let name = stock_name(stock, r.faction_name_key.as_deref()).map(str::to_string);
            match factions.iter_mut().find(|(x, _)| x == f) {
                Some((_, n)) => {
                    if n.is_none() {
                        *n = name;
                    }
                }
                None => factions.push((f.clone(), name)),
            }
        }
        let mut kinds = Vec::new();
        if group.iter().all(|c| c.chain) {
            kinds.push("Chain".to_string());
        }
        if group.iter().all(|c| c.once_only) {
            kinds.push("Once only".to_string());
        }
        let requires = group
            .iter()
            .filter_map(|c| standings.get(&c.min_standing.as_ref()?.to_ascii_lowercase()))
            .filter_map(|(min, key)| Some((*min, stock_name(stock, *key)?)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n.to_string());
        let systems: BTreeSet<&str> = group
            .iter()
            .flat_map(|c| c.systems.iter().map(String::as_str))
            .collect();
        let mission = Mission {
            title: title.clone(),
            record: record_label(group),
            generator: group[0].generator.clone(),
            reputation: factions
                .iter()
                .map(|(f, n)| n.clone().unwrap_or_else(|| faction_label(f)))
                .collect(),
            factions: factions.into_iter().map(|(f, _)| f).collect(),
            systems: systems.into_iter().map(str::to_string).collect(),
            kinds,
            requires,
            contracts: group.len() as u32,
        };
        for (bp, in_pools) in held {
            let chance_of = |c: &Contract| {
                c.blueprint_rewards
                    .iter()
                    .filter(|r| in_pools.contains(r.pool.to_ascii_lowercase().as_str()))
                    .map(|r| r.chance.unwrap_or(1.0))
                    .reduce(f64::max)
            };
            let Some(odds) = odds_by(group, chance_of) else {
                continue;
            };
            let variants = group.iter().filter(|c| chance_of(c).is_some()).count() as u32;
            let (pool, pick) = pool_odds(&in_pools, &pools, bp);
            let alone = in_pools
                .iter()
                .all(|p| pools.get(*p).is_some_and(|l| l.len() == 1));
            let award = match odds {
                Odds::Guaranteed if alone => Award::Guaranteed,
                Odds::Guaranteed => Award::GuaranteedPool,
                _ => Award::Chance,
            };
            out.entry(bp.to_string()).or_default().push(Earn {
                mission: mission.clone(),
                award,
                odds: odds.text(),
                variants,
                pool,
                pick,
                channels: Vec::new(),
            });
        }
    }
    for list in out.values_mut() {
        list.sort_by(|a, b| earn_order(a.award, &a.mission).cmp(&earn_order(b.award, &b.mission)));
    }
    Some(out)
}

/// Sure rewards first, then titled before untitled, then by title and record.
fn earn_order(award: Award, m: &Mission) -> (Award, bool, Option<&str>, &str) {
    (
        award,
        m.title.is_none(),
        m.title.as_deref(),
        m.record.as_str(),
    )
}

/// What one channel adds to the finder.
#[derive(Debug, Clone, Default)]
pub struct ChannelInput {
    pub channel: String,
    /// `None`: this build's game data has not been read yet. Shared with the per-build
    /// cache (not copied per read).
    pub catalog: Option<Arc<Vec<CatalogItem>>>,
    pub characters: Characters,
    /// Owned sets of `characters` (missing ones count as empty).
    pub sets: Vec<OwnedSet>,
    /// `unattributed.json`: seen before any login line; owner [`UNATTRIBUTED`].
    pub unattributed: Option<OwnedSet>,
    /// Set files that could not be read.
    pub unreadable: u32,
}

/// Owner id of the unattributed set (no geid is all letters).
pub const UNATTRIBUTED: &str = "unattributed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Owner {
    pub channel: String,
    /// geid.
    pub character: String,
    /// `log`, `import:smart-citizen`, `starorg:<guild>`.
    pub sources: Vec<String>,
    /// Earliest source time.
    pub acquired: Option<String>,
    pub favorite: bool,
    /// The name could be several blueprints (an import that matched several, or a name
    /// several blueprints have): listed, never marked (§4).
    pub ambiguous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    /// Blueprint id (lower case), or `name:<normalised name>` for an owned entry that matched
    /// no blueprint of the channels' builds (or several).
    pub key: String,
    pub name: String,
    pub record: Option<String>,
    pub kind: Kind,
    pub type_label: Option<String>,
    pub class: Option<String>,
    pub manufacturer: Option<String>,
    pub manufacturer_name: Option<String>,
    pub size: Option<u8>,
    pub grade: Option<String>,
    /// Channels whose build has this blueprint (name rows: their candidates' channels).
    pub channels: Vec<String>,
    /// Name rows: the blueprints the name could be (several); empty = in no read build.
    pub candidates: Vec<String>,
    pub owners: Vec<Owner>,
    /// Owners of an ambiguous name that could be this blueprint ("maybe owned").
    pub maybe: Vec<Owner>,
    /// The missions that can award it, by index in [`Finder::missions`], over every channel
    /// (each entry says whose builds have it), sure rewards first.
    pub earn: Vec<Earn<u32>>,
    /// Channels whose build's contracts were read for this blueprint: on any other channel
    /// its missions are not known (name rows: none).
    pub earn_read: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinderCharacter {
    pub id: String,
    /// Latest name seen in any of the channels.
    pub name: String,
    pub last_seen: Option<String>,
    /// Channels whose logs know the character.
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinderChannel {
    pub channel: String,
    /// Blueprints in this build; `None` = game data not read yet.
    pub catalog: Option<u32>,
    pub characters: u32,
    /// Owned entries over all characters (and the unattributed set).
    pub owned: u32,
    /// Set files that could not be read or placed (no character id): not shown.
    pub skipped: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Finder {
    pub channels: Vec<FinderChannel>,
    pub characters: Vec<FinderCharacter>,
    /// Sorted by name, then key.
    pub rows: Vec<Row>,
    /// Every mission a row's `earn` names, sorted (title, record, ...).
    pub missions: Vec<Mission>,
}

fn row_of(c: &CatalogItem) -> Row {
    Row {
        key: c.id.to_ascii_lowercase(),
        name: c.name.clone(),
        record: Some(c.record.clone()),
        kind: c.kind,
        type_label: c.type_label.clone(),
        class: c.class.clone(),
        manufacturer: c.manufacturer.clone(),
        manufacturer_name: c.manufacturer_name.clone(),
        size: c.size,
        grade: c.grade.clone(),
        channels: Vec::new(),
        candidates: Vec::new(),
        owners: Vec::new(),
        maybe: Vec::new(),
        earn: Vec::new(),
        earn_read: Vec::new(),
    }
}

/// The missions of the finder rows, each once.
#[derive(Default)]
struct MissionTable {
    list: Vec<Mission>,
    index: HashMap<Mission, u32>,
}

impl MissionTable {
    fn intern(&mut self, m: &Mission) -> u32 {
        if let Some(&i) = self.index.get(m) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(m.clone());
        self.index.insert(m.clone(), i);
        i
    }

    /// One channel's missions of a blueprint into its row: an entry every channel has the
    /// same once, with its channels.
    fn merge(&mut self, row: &mut Row, channel: &str, list: &[Earn<Mission>]) {
        if !row.earn_read.iter().any(|c| c == channel) {
            row.earn_read.push(channel.to_string());
        }
        for e in list {
            let i = self.intern(&e.mission);
            let same = |x: &Earn<u32>| {
                x.mission == i
                    && x.award == e.award
                    && x.odds == e.odds
                    && x.variants == e.variants
                    && x.pool == e.pool
                    && x.pick == e.pick
            };
            match row.earn.iter_mut().find(|x| same(x)) {
                Some(x) => {
                    if !x.channels.iter().any(|c| c == channel) {
                        x.channels.push(channel.to_string());
                    }
                }
                None => row.earn.push(Earn {
                    mission: i,
                    award: e.award,
                    odds: e.odds.clone(),
                    variants: e.variants,
                    pool: e.pool.clone(),
                    pick: e.pick.clone(),
                    channels: vec![channel.to_string()],
                }),
            }
        }
    }

    /// The table sorted, and the rows' indices moved with it.
    fn finish(self, rows: &mut [Row]) -> Vec<Mission> {
        let mut order: Vec<u32> = (0..self.list.len() as u32).collect();
        order.sort_by(|a, b| self.list[*a as usize].cmp(&self.list[*b as usize]));
        let mut to = vec![0u32; order.len()];
        for (new, old) in order.iter().enumerate() {
            to[*old as usize] = new as u32;
        }
        for r in rows {
            // Channels may add missions in any order: sure ones first again (stable).
            r.earn.sort_by(|a, b| {
                earn_order(a.award, &self.list[a.mission as usize])
                    .cmp(&earn_order(b.award, &self.list[b.mission as usize]))
            });
            for e in r.earn.iter_mut() {
                e.mission = to[e.mission as usize];
            }
        }
        let mut list: Vec<Option<Mission>> = self.list.into_iter().map(Some).collect();
        order
            .iter()
            .filter_map(|i| list[*i as usize].take())
            .collect()
    }
}

/// Merge channels into finder rows. The first channel's build wins for a blueprint's name and
/// details when builds differ.
pub fn build(inputs: &[ChannelInput]) -> Finder {
    let mut rows: Vec<Row> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    // Normalised name -> row keys, per channel and over all channels.
    let mut names_in: HashMap<&str, HashMap<String, Vec<String>>> = HashMap::new();
    let mut names_all: HashMap<String, Vec<String>> = HashMap::new();
    let mut missions = MissionTable::default();
    for inp in inputs {
        let Some(cat) = &inp.catalog else { continue };
        let names = names_in.entry(inp.channel.as_str()).or_default();
        for c in cat.iter() {
            let key = c.id.to_ascii_lowercase();
            let i = *by_key.entry(key.clone()).or_insert_with(|| {
                rows.push(row_of(c));
                rows.len() - 1
            });
            if !rows[i].channels.contains(&inp.channel) {
                rows[i].channels.push(inp.channel.clone());
            }
            if let Some(e) = &c.earn {
                missions.merge(&mut rows[i], &inp.channel, e);
            }
            let n = normalize(&c.name);
            for m in [
                names.entry(n.clone()).or_default(),
                names_all.entry(n).or_default(),
            ] {
                if !m.contains(&key) {
                    m.push(key.clone());
                }
            }
        }
    }
    // Catalogue rows end here; name rows come after.
    let catalog_rows = rows.len();
    let mut chars: BTreeMap<String, FinderCharacter> = BTreeMap::new();
    let mut channels = Vec::new();
    // Merged geids (a reset: same account and name, new geid) of every channel. The same geid
    // is the same character on every channel, so a merge seen on one channel joins the
    // character up on the others too (All channels).
    let mut aliases = super::Characters::default();
    for inp in inputs {
        for (from, to) in &inp.characters.aliases {
            aliases
                .aliases
                .entry(from.clone())
                .or_insert_with(|| to.clone());
        }
    }
    for inp in inputs {
        let mut owned = 0u32;
        let mut skipped = inp.unreadable;
        for (g, c) in &inp.characters.characters {
            let g = aliases.canonical(g);
            let e = chars.entry(g.clone()).or_insert_with(|| FinderCharacter {
                id: g.clone(),
                name: c.name.clone(),
                last_seen: c.last_seen.clone(),
                channels: Vec::new(),
            });
            if c.last_seen > e.last_seen {
                e.name = c.name.clone();
                e.last_seen = c.last_seen.clone();
            }
            if !e.channels.contains(&inp.channel) {
                e.channels.push(inp.channel.clone());
            }
        }
        let names = names_in.get(inp.channel.as_str());
        let mut sets: Vec<(String, &OwnedSet)> = Vec::new();
        for set in &inp.sets {
            match &set.geid {
                Some(g) => sets.push((aliases.canonical(g), set)),
                // A set without its character id cannot be placed: counted, shown.
                None => skipped += 1,
            }
        }
        if let Some(u) = inp.unattributed.as_ref().filter(|u| !u.entries.is_empty()) {
            sets.push((UNATTRIBUTED.to_string(), u));
            let e = chars
                .entry(UNATTRIBUTED.to_string())
                .or_insert_with(|| FinderCharacter {
                    id: UNATTRIBUTED.into(),
                    name: "Unattributed".into(),
                    last_seen: None,
                    channels: Vec::new(),
                });
            e.channels.push(inp.channel.clone());
        }
        for (geid, set) in sets {
            for (norm, e) in &set.entries {
                owned += 1;
                let unique = |m: Option<&HashMap<String, Vec<String>>>| {
                    m.and_then(|m| m.get(norm))
                        .filter(|v| v.len() == 1)
                        .map(|v| v[0].clone())
                };
                let several = |m: Option<&HashMap<String, Vec<String>>>| {
                    m.and_then(|m| m.get(norm)).filter(|v| v.len() > 1).cloned()
                };
                // By matched id first, then by a name only one blueprint has: in this
                // channel's build, else in any read build. An import marked ambiguous, or a
                // name several blueprints have, matches none of them: its own row lists the
                // candidates, and each candidate shows it as "maybe owned".
                let (key, candidates) = if e.ambiguous {
                    let ids: Vec<String> = e
                        .ids
                        .iter()
                        .map(|i| i.to_ascii_lowercase())
                        .filter(|k| by_key.get(k).is_some_and(|&i| i < catalog_rows))
                        .collect();
                    let any = |m: Option<&HashMap<String, Vec<String>>>| {
                        m.and_then(|m| m.get(norm))
                            .filter(|v| !v.is_empty())
                            .cloned()
                    };
                    let c = if ids.is_empty() {
                        any(names)
                            .or_else(|| any(Some(&names_all)))
                            .unwrap_or_default()
                    } else {
                        ids
                    };
                    (None, c)
                } else {
                    let k = e
                        .ids
                        .iter()
                        .map(|i| i.to_ascii_lowercase())
                        .find(|k| by_key.get(k).is_some_and(|&i| i < catalog_rows))
                        .or_else(|| unique(names))
                        .or_else(|| unique(Some(&names_all)));
                    let c = if k.is_some() {
                        Vec::new()
                    } else {
                        several(names)
                            .or_else(|| several(Some(&names_all)))
                            .unwrap_or_default()
                    };
                    (k, c)
                };
                let ambiguous = key.is_none() && (e.ambiguous || !candidates.is_empty());
                let key = key.unwrap_or_else(|| format!("name:{norm}"));
                let i = *by_key.entry(key.clone()).or_insert_with(|| {
                    rows.push(Row {
                        key: key.clone(),
                        name: e.name.clone(),
                        record: None,
                        kind: Kind::Other,
                        type_label: None,
                        class: None,
                        manufacturer: None,
                        manufacturer_name: None,
                        size: None,
                        grade: None,
                        channels: Vec::new(),
                        candidates: Vec::new(),
                        owners: Vec::new(),
                        maybe: Vec::new(),
                        earn: Vec::new(),
                        earn_read: Vec::new(),
                    });
                    rows.len() - 1
                });
                for c in &candidates {
                    if !rows[i].candidates.contains(c) {
                        rows[i].candidates.push(c.clone());
                    }
                }
                let owner = Owner {
                    channel: inp.channel.clone(),
                    character: geid.to_string(),
                    sources: e.sources.iter().map(|s| s.kind.clone()).collect(),
                    acquired: e.sources.iter().filter_map(|s| s.time.clone()).min(),
                    favorite: e.favorite,
                    ambiguous,
                };
                for c in &candidates {
                    if let Some(&ci) = by_key.get(c) {
                        add_owner(&mut rows[ci].maybe, owner.clone());
                    }
                }
                add_owner(&mut rows[i].owners, owner);
            }
        }
        channels.push(FinderChannel {
            channel: inp.channel.clone(),
            catalog: inp.catalog.as_ref().map(|c| c.len() as u32),
            characters: inp.characters.characters.len() as u32,
            owned,
            skipped,
        });
    }
    // A name row takes what all its candidates agree on (kind, type, maker, size ...).
    for i in catalog_rows..rows.len() {
        let cands: Vec<Row> = rows[i]
            .candidates
            .iter()
            .filter_map(|k| by_key.get(k).map(|&j| rows[j].clone()))
            .collect();
        let Some(first) = cands.first() else { continue };
        let same = |f: &dyn Fn(&Row) -> String| cands.iter().all(|c| f(c) == f(first));
        let r = &mut rows[i];
        if same(&|c| format!("{:?}", c.kind)) {
            r.kind = first.kind;
        }
        macro_rules! common {
            ($($f:ident),*) => {$(
                if same(&|c| format!("{:?}", c.$f)) {
                    r.$f = first.$f.clone();
                }
            )*};
        }
        common!(
            type_label,
            class,
            manufacturer,
            manufacturer_name,
            size,
            grade
        );
        for c in &cands {
            for ch in &c.channels {
                if !r.channels.contains(ch) {
                    r.channels.push(ch.clone());
                }
            }
        }
    }
    rows.sort_by(|a, b| (a.name.to_lowercase(), &a.key).cmp(&(b.name.to_lowercase(), &b.key)));
    let mut characters: Vec<FinderCharacter> = chars.into_values().collect();
    characters.sort_by(|a, b| {
        ((a.id == UNATTRIBUTED), a.name.to_lowercase(), &a.id).cmp(&(
            (b.id == UNATTRIBUTED),
            b.name.to_lowercase(),
            &b.id,
        ))
    });
    let missions = missions.finish(&mut rows);
    Finder {
        channels,
        characters,
        rows,
        missions,
    }
}

/// One owner per channel and character; two entries of one set on one blueprint (a log name
/// and an import) merge.
fn add_owner(list: &mut Vec<Owner>, owner: Owner) {
    match list
        .iter_mut()
        .find(|o| o.channel == owner.channel && o.character == owner.character)
    {
        Some(o) => {
            for s in owner.sources {
                if !o.sources.contains(&s) {
                    o.sources.push(s);
                }
            }
            o.acquired = match (o.acquired.take(), owner.acquired) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            o.favorite |= owner.favorite;
            o.ambiguous &= owner.ambiguous;
        }
        None => list.push(owner),
    }
}

#[cfg(test)]
#[path = "finder_tests.rs"]
mod tests;
