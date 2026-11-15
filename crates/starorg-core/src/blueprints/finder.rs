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
use crate::enhance::stock_name;
use crate::enhance::tags::grade_letter;
use crate::groups::GroupIndex;
use crate::ini::{StockIni, fold_key};
use crate::model::{Blueprint, GameData, ItemIdentity, is_placeholder_name};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

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
            })
        })
        .collect()
}

/// What one channel adds to the finder.
#[derive(Debug, Clone, Default)]
pub struct ChannelInput {
    pub channel: String,
    /// `None`: this build's game data has not been read yet.
    pub catalog: Option<Vec<CatalogItem>>,
    pub characters: Characters,
    /// Owned sets of `characters` (missing ones count as empty).
    pub sets: Vec<OwnedSet>,
}

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
    /// Matched several blueprints on import: listed, never marked (§4).
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
    /// Channels whose build has this blueprint (empty: in no read build).
    pub channels: Vec<String>,
    pub owners: Vec<Owner>,
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
    /// Owned entries over all characters.
    pub owned: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Finder {
    pub channels: Vec<FinderChannel>,
    pub characters: Vec<FinderCharacter>,
    /// Sorted by name, then key.
    pub rows: Vec<Row>,
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
        owners: Vec::new(),
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
    for inp in inputs {
        let Some(cat) = &inp.catalog else { continue };
        let names = names_in.entry(inp.channel.as_str()).or_default();
        for c in cat {
            let key = c.id.to_ascii_lowercase();
            let i = *by_key.entry(key.clone()).or_insert_with(|| {
                rows.push(row_of(c));
                rows.len() - 1
            });
            if !rows[i].channels.contains(&inp.channel) {
                rows[i].channels.push(inp.channel.clone());
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
    let mut chars: BTreeMap<String, FinderCharacter> = BTreeMap::new();
    let mut channels = Vec::new();
    for inp in inputs {
        let mut owned = 0u32;
        for (g, c) in &inp.characters.characters {
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
            e.channels.push(inp.channel.clone());
        }
        let names = names_in.get(inp.channel.as_str());
        for set in &inp.sets {
            let Some(geid) = &set.geid else { continue };
            for (norm, e) in &set.entries {
                owned += 1;
                let unique = |m: Option<&HashMap<String, Vec<String>>>| {
                    m.and_then(|m| m.get(norm))
                        .filter(|v| v.len() == 1)
                        .map(|v| v[0].clone())
                };
                // By matched id first (an ambiguous entry matches nothing), then by a name
                // only one blueprint has: in this channel's build, else in any read build.
                let key = if e.ambiguous {
                    None
                } else {
                    e.ids
                        .iter()
                        .map(|i| i.to_ascii_lowercase())
                        .find(|k| by_key.contains_key(k))
                        .or_else(|| unique(names))
                        .or_else(|| unique(Some(&names_all)))
                };
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
                        owners: Vec::new(),
                    });
                    rows.len() - 1
                });
                let owner = Owner {
                    channel: inp.channel.clone(),
                    character: geid.clone(),
                    sources: e.sources.iter().map(|s| s.kind.clone()).collect(),
                    acquired: e.sources.iter().filter_map(|s| s.time.clone()).min(),
                    favorite: e.favorite,
                    ambiguous: e.ambiguous,
                };
                let r = &mut rows[i];
                match r
                    .owners
                    .iter_mut()
                    .find(|o| o.channel == owner.channel && o.character == owner.character)
                {
                    // Two entries of one set on one blueprint (a log name and an import).
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
                    None => r.owners.push(owner),
                }
            }
        }
        channels.push(FinderChannel {
            channel: inp.channel.clone(),
            catalog: inp.catalog.as_ref().map(|c| c.len() as u32),
            characters: inp.characters.characters.len() as u32,
            owned,
        });
    }
    rows.sort_by(|a, b| (a.name.to_lowercase(), &a.key).cmp(&(b.name.to_lowercase(), &b.key)));
    let mut characters: Vec<FinderCharacter> = chars.into_values().collect();
    characters.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
    Finder {
        channels,
        characters,
        rows,
    }
}

#[cfg(test)]
#[path = "finder_tests.rs"]
mod tests;
