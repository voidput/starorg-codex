//! Strings grouping (addendum 8): one primary group per stock key, plus manufacturer and
//! size facets, from the records that use the key.
//!
//! Rules, in order (8.2):
//!
//! 1. A [`GameData`] link: a component, weapon, missile, ship, contract or reputation
//!    standing whose loc keys include the key.
//! 2. [`KeyRefs`]: every main DataCore record that holds the key as an `@key` string, with
//!    its struct type and, for entities, `AttachDef` type, sub-type, size and manufacturer.
//!    The most specific candidate wins ([`rules::Rank`]), then the one with more references,
//!    then the lower group id.
//! 3. Key-prefix rules ([`rules::KEY_PREFIXES`]); `item_*` keys go to the group of their
//!    namesake record when one exists ([`crate::model::namesake::named_after`]).
//! 4. [`rules::OTHER`].
//!
//! Pure and deterministic; [`assign`] runs once per cache entry (about 91,000 keys) and the
//! result lives next to the Strings table. Group labels are English only (Q-G7).

pub mod owners;
pub mod rules;

use crate::ini::{StockIni, fold_key};
use crate::model::{GameData, ItemIdentity, WeaponClass, loc_key, namesake};
use rules::{GroupDef, HIERARCHY, OTHER, Rank, UNKNOWN_ITEM};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Layout version of [`KeyRefs`] (`keyrefs.json` in a cache entry).
pub const KEYREFS_SCHEMA: u32 = 1;
/// Candidates kept per key (the most referenced ones).
pub const MAX_CANDIDATES: usize = 4;
/// Sizes above this are not a size facet (records use 0 and huge values as "none").
pub const MAX_SIZE: i64 = 12;

/// What kind of record references a key. Deduplicated in [`KeyRefs::sources`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefSource {
    /// DataCore struct type of the main record, `EntityClassDefinition`.
    pub struct_type: String,
    /// Entities: `AttachDef.Type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    /// Manufacturer code, upper case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// First segments of the record path (`entities/spaceships`, `ui/hud/...`), lower case.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

/// Which records reference which loc keys: the output of the reference pass.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRefs {
    pub schema: u32,
    pub sources: Vec<RefSource>,
    /// Folded key -> `[source index, reference count]`, most references first.
    pub keys: BTreeMap<String, Vec<[u32; 2]>>,
}

/// Collects references record by record.
#[derive(Debug, Default)]
pub struct KeyRefsBuilder {
    sources: Vec<RefSource>,
    by_source: HashMap<RefSource, u32>,
    keys: HashMap<String, HashMap<u32, u32>>,
}

impl KeyRefsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one record's references: `keys` as found (with or without `@`), repeats count.
    pub fn add<'k>(&mut self, source: RefSource, keys: impl IntoIterator<Item = &'k str>) {
        let next = self.sources.len() as u32;
        let mut id = None;
        for k in keys {
            let Some(k) = loc_key(k) else { continue };
            let src = *id.get_or_insert_with(|| {
                *self.by_source.entry(source.clone()).or_insert_with(|| {
                    self.sources.push(source.clone());
                    next
                })
            });
            *self
                .keys
                .entry(fold_key(k))
                .or_default()
                .entry(src)
                .or_default() += 1;
        }
    }

    pub fn finish(self) -> KeyRefs {
        let keys = self
            .keys
            .into_iter()
            .map(|(k, m)| {
                let mut v: Vec<[u32; 2]> = m.into_iter().map(|(s, c)| [s, c]).collect();
                v.sort_by(|a, b| b[1].cmp(&a[1]).then(a[0].cmp(&b[0])));
                v.truncate(MAX_CANDIDATES);
                (k, v)
            })
            .collect();
        KeyRefs {
            schema: KEYREFS_SCHEMA,
            sources: self.sources,
            keys,
        }
    }
}

/// One key's place: indexes, so a row costs 4 bytes (8.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Assignment {
    /// Index into [`HIERARCHY`] (always a leaf).
    pub group: u8,
    /// 1-based index into [`GroupIndex::manufacturers`]; 0 = none.
    pub manufacturer: u16,
    /// 1..=12; 0 = none.
    pub size: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manufacturer {
    /// Upper-case code, `AEGS`.
    pub code: String,
    /// Display name from the stock `manufacturer_Name<code>` key (codes without one are
    /// not a manufacturer facet).
    pub label: String,
}

/// The result of [`assign`].
#[derive(Debug, Clone, Default)]
pub struct GroupIndex {
    /// Built with [`KeyRefs`]; without them only rules 1, 3 and 4 applied.
    pub exact: bool,
    /// One per stock entry, in [`StockIni::entries`] order.
    pub rows: Vec<Assignment>,
    /// Sorted by label.
    pub manufacturers: Vec<Manufacturer>,
    /// Row -> other groups that also use the key (Q-G3 "Also used by").
    pub also: HashMap<u32, Vec<u8>>,
    /// Entity item types no rule names, with key counts (drift check).
    pub unknown_item_types: BTreeMap<String, u32>,
}

impl GroupIndex {
    pub fn group(&self, row: usize) -> &'static GroupDef {
        &HIERARCHY[self.rows.get(row).map(|a| a.group).unwrap_or(other_index()) as usize]
    }

    pub fn manufacturer(&self, row: usize) -> Option<&Manufacturer> {
        let m = self.rows.get(row)?.manufacturer;
        (m > 0).then(|| &self.manufacturers[m as usize - 1])
    }

    pub fn size(&self, row: usize) -> Option<u8> {
        self.rows.get(row).map(|a| a.size).filter(|s| *s > 0)
    }

    /// Labels of the other groups that use the key of `row`.
    pub fn also_used_by(&self, row: usize) -> Vec<&'static str> {
        self.also
            .get(&(row as u32))
            .map(|v| v.iter().map(|&g| HIERARCHY[g as usize].label).collect())
            .unwrap_or_default()
    }
}

// Group indexes are stored as u8.
const _: () = assert!(HIERARCHY.len() < u8::MAX as usize);

/// Index of a group id in [`HIERARCHY`].
pub fn index_of(id: &str) -> Option<u8> {
    HIERARCHY.iter().position(|g| g.id == id).map(|i| i as u8)
}

fn other_index() -> u8 {
    index_of(OTHER).unwrap_or_default()
}

/// The top-level group of a leaf.
pub fn top_of(group: &GroupDef) -> &'static str {
    match group.parent {
        Some(p) => HIERARCHY
            .iter()
            .find(|g| g.id == p)
            .map(|g| g.id)
            .unwrap_or(OTHER),
        None => HIERARCHY
            .iter()
            .find(|g| g.id == group.id)
            .map(|g| g.id)
            .unwrap_or(OTHER),
    }
}

/// True for groups that have children (a selection of one selects its children).
pub fn has_children(id: &str) -> bool {
    HIERARCHY.iter().any(|g| g.parent == Some(id))
}

/// Where a candidate comes from; decides whether a key-prefix rule overrides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Rule 1: the model names the key.
    Data,
    /// Rule 2, an entity with an `AttachDef`.
    Item,
    /// Rule 2, an entity without one (NPCs, props, areas).
    Untyped,
    /// Rule 2, any other record.
    Record,
}

/// A candidate group for a key, with what decides between candidates.
#[derive(Debug, Clone)]
struct Candidate {
    leaf: &'static str,
    rank: Rank,
    refs: u32,
    origin: Origin,
    manufacturer: Option<String>,
    size: Option<i64>,
}

impl Candidate {
    /// An entity that only displays the key loses to its key-prefix rule: always without an
    /// `AttachDef`, and with one when the prefix names a text group (`ui_`, `hud_`, `chat_`).
    fn loses_to(&self, prefix: Option<&str>) -> bool {
        match (self.origin, prefix) {
            (Origin::Untyped, Some(_)) => true,
            (Origin::Item, Some(p)) => rules::is_text(p),
            _ => false,
        }
    }
}

fn better(a: &Candidate, b: &Candidate) -> bool {
    (a.rank, std::cmp::Reverse(a.refs), a.leaf) < (b.rank, std::cmp::Reverse(b.refs), b.leaf)
}

/// Leaf of an entity from its `AttachDef`, `None` for entities without one.
fn entity_leaf(item_type: &str, sub_type: Option<&str>, folder: &str) -> Option<&'static str> {
    let lower = item_type.to_ascii_lowercase();
    if lower.contains("consumable") {
        let sub = sub_type.unwrap_or_default().to_ascii_lowercase();
        let medical = rules::MEDICAL_SUBTYPES.iter().any(|m| sub.contains(m));
        return Some(if medical {
            "consumables/medical"
        } else if sub.contains("food") {
            "consumables/food"
        } else if sub.contains("drink") {
            "consumables/drink"
        } else {
            "consumables/other"
        });
    }
    let leaf = rules::find_item_type(item_type)?;
    Some(
        if leaf == "ships/ships" && folder.contains("groundvehicle") {
            "ships/ground"
        } else {
            leaf
        },
    )
}

fn source_candidate(
    s: &RefSource,
    refs: u32,
    unknown: &mut BTreeMap<String, u32>,
) -> Option<Candidate> {
    let (leaf, rank, origin) = if s.struct_type == "EntityClassDefinition" {
        match s.item_type.as_deref() {
            Some(t) => match entity_leaf(t, s.sub_type.as_deref(), &s.folder) {
                Some(l) => (l, Rank::Item, Origin::Item),
                None => {
                    *unknown.entry(t.to_string()).or_default() += 1;
                    (UNKNOWN_ITEM, Rank::Item, Origin::Item)
                }
            },
            // Entities without AttachDef (NPCs, props, areas): weaker than any named type.
            None if s.folder.contains("spaceships") => ("ships/ships", Rank::Item, Origin::Untyped),
            None => (UNKNOWN_ITEM, Rank::Dialogue, Origin::Untyped),
        }
    } else {
        let (mut leaf, rank) = rules::find_struct(&s.struct_type)?;
        if s.struct_type == "BuildingBlocks_Canvas" {
            leaf = rules::CANVAS_PATHS
                .iter()
                .find(|(p, _)| s.folder.contains(p))
                .map(|(_, g)| *g)
                .unwrap_or(leaf);
        }
        (leaf, rank, Origin::Record)
    };
    Some(Candidate {
        leaf,
        rank,
        refs,
        origin,
        manufacturer: s.manufacturer.clone(),
        size: s.size,
    })
}

fn item_leaf(item: &ItemIdentity, fallback: &'static str) -> &'static str {
    item.item_type
        .as_deref()
        .and_then(|t| entity_leaf(t, item.sub_type.as_deref(), &item.path))
        .unwrap_or(fallback)
}

fn component_fallback(category: &str) -> &'static str {
    match category {
        "coolers" => "components/coolers",
        "powerPlants" => "components/power-plants",
        "shields" => "components/shields",
        "quantumDrives" | "quantumEnforcement" => "components/quantum-drives",
        "jumpDrives" => "components/jump-drives",
        "radars" => "components/radars",
        "thrusters" => "components/thrusters",
        "fuelTanks" | "quantumFuelTanks" | "fuelIntakes" => "components/fuel",
        "armor" => "components/armor",
        "mounts" | "turrets" => "components/turrets",
        "missileRacks" => "components/missile-racks",
        "weapons" => "ship-weapons/guns",
        "countermeasures" => "ship-weapons/countermeasures",
        "missiles" => "missiles",
        _ => "components/other",
    }
}

/// Rule 1: folded key -> candidate, from the model.
fn game_data_links(gd: &GameData) -> (HashMap<String, Candidate>, HashMap<String, &'static str>) {
    let mut links: HashMap<String, Candidate> = HashMap::new();
    // Short name (squashed) of every item and ship -> leaf, for the namesake fallback.
    let mut names: HashMap<String, &'static str> = HashMap::new();
    let mut put = |key: Option<&str>, c: &Candidate| {
        if let Some(k) = key.and_then(loc_key) {
            let k = fold_key(k);
            match links.get(&k) {
                Some(old) if !better(c, old) => {}
                _ => {
                    links.insert(k, c.clone());
                }
            }
        }
    };
    let mut item =
        |it: &ItemIdentity, leaf: &'static str, names: &mut HashMap<String, &'static str>| {
            let c = Candidate {
                leaf,
                rank: Rank::Item,
                refs: 1,
                origin: Origin::Data,
                manufacturer: it.manufacturer.clone(),
                size: it.size,
            };
            if !it.is_template {
                let name = namesake::squash_name(it.short_name());
                // `RSI_Bengal_Turret_SCItem` is named by `item_NameRSI_Bengal_Turret`.
                if let Some(base) = name.strip_suffix("scitem").filter(|b| !b.is_empty()) {
                    names.entry(base.to_string()).or_insert(leaf);
                }
                names.entry(name).or_insert(leaf);
            }
            for k in [&it.loc.name, &it.loc.short_name, &it.loc.desc] {
                put(k.as_deref(), &c);
            }
        };
    for c in &gd.components {
        let leaf = item_leaf(&c.item, component_fallback(&c.category));
        item(&c.item, leaf, &mut names);
    }
    for w in &gd.weapons {
        let fallback = match w.class {
            WeaponClass::Ship => "ship-weapons/guns",
            WeaponClass::Fps => "fps-weapons/weapons",
        };
        item(&w.item, item_leaf(&w.item, fallback), &mut names);
    }
    for m in &gd.missiles {
        item(&m.item, "missiles", &mut names);
    }
    for s in &gd.ships {
        let leaf = if s.path.contains("groundvehicle") {
            "ships/ground"
        } else {
            "ships/ships"
        };
        let c = Candidate {
            leaf,
            rank: Rank::Item,
            refs: 1,
            origin: Origin::Data,
            manufacturer: s.manufacturer.as_deref().map(str::to_ascii_uppercase),
            size: s.size.map(|v| v.round() as i64),
        };
        if !s.is_template {
            names
                .entry(namesake::squash_name(crate::model::short_name(
                    &s.record_name,
                )))
                .or_insert(leaf);
        }
        for k in [&s.loc.name, &s.loc.desc] {
            put(k.as_deref(), &c);
        }
    }
    let mission = Candidate {
        leaf: "missions/contracts",
        rank: Rank::Mission,
        refs: 1,
        origin: Origin::Data,
        manufacturer: None,
        size: None,
    };
    for c in &gd.contracts {
        put(c.title_key.as_deref(), &mission);
        put(c.description_key.as_deref(), &mission);
    }
    let rep = Candidate {
        leaf: "reputation",
        rank: Rank::Reputation,
        refs: 1,
        origin: Origin::Data,
        manufacturer: None,
        size: None,
    };
    for r in &gd.reputation_standings {
        put(r.display_name_key.as_deref(), &rep);
    }
    (links, names)
}

/// Group every stock key. `keyrefs: None` gives an approximate index (rules 1, 3, 4).
pub fn assign(stock: &StockIni, gd: &GameData, keyrefs: Option<&KeyRefs>) -> GroupIndex {
    let (links, names) = game_data_links(gd);
    let mut unknown = BTreeMap::new();
    let mut mfr_codes: BTreeMap<String, ()> = BTreeMap::new();
    let mut picked: Vec<(u8, Option<String>, u8)> = Vec::new();
    let mut also = HashMap::new();
    for (row, (_, key, _)) in stock.entries().enumerate() {
        let folded = fold_key(key);
        let mut cands: Vec<Candidate> = Vec::new();
        if let Some(c) = links.get(&folded) {
            cands.push(c.clone());
        }
        if let Some(refs) = keyrefs.and_then(|k| k.keys.get(&folded).map(|v| (k, v))) {
            let (k, list) = refs;
            for [src, n] in list {
                if let Some(s) = k.sources.get(*src as usize)
                    && let Some(c) = source_candidate(s, *n, &mut unknown)
                {
                    cands.push(c);
                }
            }
        }
        // Rule 1 wins outright; rule 2 by rank, without the entities that only display a
        // key whose prefix says what it is.
        let prefix = rules::find_prefix(&folded);
        let best = if let Some(c) = links.get(&folded) {
            Some(c.clone())
        } else {
            cands
                .iter()
                .filter(|c| !c.loses_to(prefix))
                .fold(None::<&Candidate>, |acc, c| match acc {
                    Some(a) if !better(c, a) => Some(a),
                    _ => Some(c),
                })
                .cloned()
        };
        let (leaf, mfr, size) = match best {
            Some(c) => {
                let (mfr, size) = facets(&c, &cands, &folded);
                (c.leaf, mfr, size)
            }
            None => (fallback_leaf(&folded, &names), None, None),
        };
        let size = size.filter(|_| rules::has_size(leaf));
        let gi = index_of(leaf).unwrap_or_else(other_index);
        let mut others: Vec<u8> = cands
            .iter()
            .filter_map(|c| index_of(c.leaf))
            .filter(|g| *g != gi)
            .collect();
        others.sort_unstable();
        others.dedup();
        if !others.is_empty() {
            also.insert(row as u32, others);
        }
        let size = size
            .filter(|s| (1..=MAX_SIZE).contains(s))
            .map(|s| s as u8)
            .unwrap_or(0);
        let mfr = mfr
            .filter(|m| !m.is_empty())
            .map(|m| m.to_ascii_uppercase());
        if let Some(m) = &mfr {
            mfr_codes.insert(m.clone(), ());
        }
        picked.push((gi, mfr, size));
    }
    // Manufacturers sorted by label; the u16 index leaves room for any real count.
    // Only codes with a stock display name: manufacturer records also hold paint and decal
    // logos (`PAINT_100I_..._LOGO`), which are no manufacturer to filter by.
    let mut manufacturers: Vec<Manufacturer> = mfr_codes
        .keys()
        .filter_map(|code| {
            let label = stock
                .value(&format!("manufacturer_Name{code}"))
                .map(str::trim)
                .filter(|v| !v.is_empty() && !crate::model::is_placeholder_name(Some(v)))?;
            Some(Manufacturer {
                code: code.clone(),
                label: label.to_string(),
            })
        })
        .collect();
    manufacturers
        .sort_by(|a, b| (a.label.to_lowercase(), &a.code).cmp(&(b.label.to_lowercase(), &b.code)));
    manufacturers.truncate(u16::MAX as usize - 1);
    let mfr_index: HashMap<&str, u16> = manufacturers
        .iter()
        .enumerate()
        .map(|(i, m)| (m.code.as_str(), i as u16 + 1))
        .collect();
    let rows = picked
        .iter()
        .map(|(g, m, s)| Assignment {
            group: *g,
            manufacturer: m
                .as_deref()
                .and_then(|m| mfr_index.get(m).copied())
                .unwrap_or(0),
            size: *s,
        })
        .collect();
    GroupIndex {
        exact: keyrefs.is_some(),
        rows,
        manufacturers,
        also,
        unknown_item_types: unknown,
    }
}

/// Manufacturer and size of the winning candidate. Rule 1 names its item, so its facets
/// stand. A record reference only counts when every entity that uses the key agrees: a
/// generic key (`panel_open`) on records of many makers has no maker, and shared hardpoint
/// names (`port_*`, `itemPort_*`) have none at all.
fn facets(best: &Candidate, cands: &[Candidate], folded: &str) -> (Option<String>, Option<i64>) {
    if best.origin == Origin::Data {
        return (best.manufacturer.clone(), best.size);
    }
    let token = folded.split('_').next().unwrap_or_default();
    if rules::NO_FACET_PREFIXES.contains(&token) {
        return (None, None);
    }
    let entities: Vec<&Candidate> = cands
        .iter()
        .filter(|c| matches!(c.origin, Origin::Item | Origin::Untyped))
        .collect();
    let mfr = agreed(best, &entities, |c| {
        c.manufacturer.as_deref().map(str::to_ascii_uppercase)
    });
    let size = agreed(best, &entities, |c| c.size);
    (mfr, size)
}

/// `f(best)` when every candidate in `all` gives the same value.
fn agreed<T: PartialEq>(
    best: &Candidate,
    all: &[&Candidate],
    f: impl Fn(&Candidate) -> Option<T>,
) -> Option<T> {
    let v = f(best)?;
    all.iter().all(|c| f(c).as_ref() == Some(&v)).then_some(v)
}

/// Rules 3 and 4.
fn fallback_leaf(folded: &str, names: &HashMap<String, &'static str>) -> &'static str {
    let token = folded.split('_').next().unwrap_or_default();
    if token == "item" || token == "items" || token == "vehicle" {
        if let Some(g) = namesake::stem(folded).and_then(|s| names.get(&s)) {
            return g;
        }
        if token != "vehicle" {
            return UNKNOWN_ITEM;
        }
    }
    rules::find_prefix(folded).unwrap_or(OTHER)
}

#[cfg(test)]
mod tests;
