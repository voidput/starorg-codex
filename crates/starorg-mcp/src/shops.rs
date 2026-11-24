//! The shop browser: what every UEX terminal sells, grouped by star system, place and shop,
//! each item in a readable category and linked to its game record when the build's item data
//! knows it. The app's Shops screen (`shop_inventory`, `shop_items`, and the item search
//! `shop_item_search`) and the `shop_inventory` MCP tool answer from one [`ShopIndex`] built
//! from the two UEX files.
//!
//! Shop stock is server-side and in no game file: every row is UEX data, reported by players.
//!
//! Categories: the game's item type when the row is linked to a record (authoritative), else
//! the UEX category id the price row carries (every row has one; UEX lists ship guns, mining
//! lasers and tractor beams that `game-item-tags` does not). See [`categorise`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use starorg_gamedata::collate::locale_compare;
use starorg_gamedata::query::fuzzy::{self, Entry, EntryKind};
use starorg_gamedata::query::shops::{
    AsOf, Locator, STALE_DAYS, ShopPrice, UexTerminal, normalise_place, price_age, reference_time,
};
pub use starorg_gamedata::query::types::ItemTags;
use std::collections::{BTreeMap, HashMap};

/// [`build_index`] over the two files as parsed JSON (`{ _fetched, prices }` and
/// `{ terminals }`); rows that do not parse are left out.
pub fn index_from_files(
    prices: &Value,
    terminals: &Value,
    items: &[ItemTags],
    linked: Option<String>,
) -> ShopIndex {
    let rows: Vec<ShopPrice> = prices
        .get("prices")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let terms: Vec<UexTerminal> = terminals
        .get("terminals")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let fetched = prices.get("_fetched").and_then(Value::as_str);
    build_index(&rows, &terms, items, fetched, linked)
}

/// A readable bucket of what a shop sells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShopCategory {
    FpsWeapons,
    FpsAttachments,
    Armour,
    ShipWeapons,
    Missiles,
    ShipComponents,
    Mining,
    Consumables,
    Other,
}

impl ShopCategory {
    /// Display order.
    pub const ALL: [ShopCategory; 9] = [
        Self::FpsWeapons,
        Self::FpsAttachments,
        Self::Armour,
        Self::ShipWeapons,
        Self::Missiles,
        Self::ShipComponents,
        Self::Mining,
        Self::Consumables,
        Self::Other,
    ];

    /// The serialized key (`ship-weapons`).
    pub fn key(self) -> &'static str {
        match self {
            Self::FpsWeapons => "fps-weapons",
            Self::FpsAttachments => "fps-attachments",
            Self::Armour => "armour",
            Self::ShipWeapons => "ship-weapons",
            Self::Missiles => "missiles",
            Self::ShipComponents => "ship-components",
            Self::Mining => "mining",
            Self::Consumables => "consumables",
            Self::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FpsWeapons => "FPS weapons",
            Self::FpsAttachments => "FPS attachments",
            Self::Armour => "Armour & clothing",
            Self::ShipWeapons => "Ship weapons",
            Self::Missiles => "Missiles & torpedoes",
            Self::ShipComponents => "Ship components",
            Self::Mining => "Mining & salvage",
            Self::Consumables => "Consumables",
            Self::Other => "Other",
        }
    }

    /// A key or label as a user types it: `missiles`, `Ship weapons`, `armor`.
    pub fn parse(s: &str) -> Option<Self> {
        let n = normalise_place(s);
        if n.is_empty() {
            return None;
        }
        let alias = match n.as_str() {
            "armor" | "clothing" | "clothes" => Some(Self::Armour),
            "torpedoes" | "torpedo" | "missile" | "bombs" => Some(Self::Missiles),
            "components" => Some(Self::ShipComponents),
            "attachments" => Some(Self::FpsAttachments),
            "weapons" | "guns" | "fps" => Some(Self::FpsWeapons),
            "salvage" => Some(Self::Mining),
            "food" | "drinks" | "medical" => Some(Self::Consumables),
            _ => None,
        };
        alias.or_else(|| {
            Self::ALL.into_iter().find(|c| {
                normalise_place(c.key()) == n
                    || normalise_place(c.label()) == n
                    || normalise_place(c.label()).starts_with(&n) && n.len() >= 4
            })
        })
    }
}

/// The bucket of a game item type (`type` / `subType` of `game-item-tags`); `None` for a type
/// that says too little (`Misc`, `Container`, `Gadget`), left to the UEX category.
pub fn by_game_type(ty: &str) -> Option<ShopCategory> {
    use ShopCategory::*;
    if ty.starts_with("Char_Armor") || ty.starts_with("Char_Clothing") {
        return Some(Armour);
    }
    Some(match ty {
        "WeaponPersonal" => FpsWeapons,
        "WeaponAttachment" => FpsAttachments,
        "WeaponGun" | "Turret" | "TurretBase" | "WeaponDefensive" => ShipWeapons,
        "Missile" | "MissileLauncher" | "Bomb" | "BombLauncher" => Missiles,
        "Shield"
        | "PowerPlant"
        | "Cooler"
        | "QuantumDrive"
        | "Radar"
        | "JumpDrive"
        | "FuelTank"
        | "QuantumFuelTank"
        | "ExternalFuelTank"
        | "FuelIntake"
        | "FlightController"
        | "LifeSupportGenerator"
        | "TractorBeam"
        | "EMP"
        | "QuantumInterdictionGenerator"
        | "DockingCollar"
        | "ToolArm"
        | "Battery" => ShipComponents,
        "WeaponMining"
        | "MiningModifier"
        | "SalvageHead"
        | "SalvageModifier"
        | "SalvageFieldEmitter" => Mining,
        "FPS_Consumable" | "Food" | "Drink" | "Bottle" => Consumables,
        _ => return None,
    })
}

/// The bucket of a UEX item category id (`id_category` of `items_prices_all`), as UEX
/// numbers them in API 2.0 (Nov 2026); an id not listed is [`ShopCategory::Other`].
pub fn by_uex_category(id: u64) -> ShopCategory {
    use ShopCategory::*;
    match id {
        18 => FpsWeapons,
        17 => FpsAttachments,
        // Arms, backpacks, helmets, legs, torso, footwear, gloves, hats, jackets, pants,
        // shirts, undersuits.
        1 | 2 | 3 | 4 | 5 | 8 | 9 | 10 | 11 | 13 | 14 | 24 => Armour,
        // Guns; turrets and mounts.
        32 | 35 => ShipWeapons,
        // Missile racks, missiles, bombs, bomb racks.
        33 | 34 | 70 | 90 => Missiles,
        // Coolers, power plants, quantum drives, shields, fuel nozzles, fuel pods, tractor
        // beams, flight blades, radars, jump drives.
        19 | 21 | 22 | 23 | 25 | 26 | 67 | 82 | 83 | 86 | 110 => ShipComponents,
        // Mining gadgets, mining lasers, mining modules, salvage modules.
        28..=31 => Mining,
        // Medical, drinks, food.
        16 | 62 | 63 => Consumables,
        _ => Other,
    }
}

/// The game type when it says enough, else the UEX category, else [`ShopCategory::Other`].
pub fn categorise(game_type: Option<&str>, uex_category: Option<u64>) -> ShopCategory {
    game_type
        .and_then(by_game_type)
        .or_else(|| uex_category.map(by_uex_category))
        .unwrap_or(ShopCategory::Other)
}

/// Whether the two UEX files were there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShopDataState {
    Ready,
    /// One or both files are missing (Settings -> Network -> Fetch now).
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryCount {
    pub category: ShopCategory,
    pub count: usize,
}

/// One shop (a UEX item terminal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopSummary {
    /// UEX terminal id (the `terminal` of `shop_items`).
    pub id: u64,
    /// The terminal (`Dumper's GrimHEX`).
    pub name: String,
    /// The place without the system (`Grim HEX, Yela, Crusader`).
    pub location: String,
    pub system: Option<String>,
    /// Item rows with a price.
    pub items: usize,
    /// What it sells, in [`ShopCategory::ALL`] order.
    pub categories: Vec<CategoryCount>,
    /// UEX marks the terminal unavailable on LIVE.
    pub not_on_live: bool,
    /// The newest day UEX confirmed a row (YYYY-MM-DD).
    pub updated: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopPlaceGroup {
    /// `Grim HEX, Yela, Crusader`.
    pub name: String,
    pub shops: Vec<ShopSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopSystem {
    /// `Stanton`; `Unknown system` when UEX names none.
    pub name: String,
    pub locations: Vec<ShopPlaceGroup>,
}

/// The game record an item row is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRecord {
    /// The record's GUID (UEX's `item_uuid`): what the Explorer opens.
    pub guid: String,
    pub class_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopItemRow {
    /// The game's name when the row is linked, else UEX's.
    pub name: String,
    /// UEX's name when it differs from the game's.
    pub uex_name: Option<String>,
    pub category: ShopCategory,
    pub price_buy: Option<f64>,
    /// What the shop pays for it.
    pub price_sell: Option<f64>,
    /// Day UEX last confirmed the row (YYYY-MM-DD).
    pub updated: Option<String>,
    /// Not confirmed in [`STALE_DAYS`] days.
    pub stale: bool,
    pub record: Option<ItemRecord>,
}

/// `shop_inventory`: every shop, by system, then place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopDirectory {
    pub state: ShopDataState,
    /// Where the UEX files go (`<cache>/mcp/uex`).
    pub folder: Option<String>,
    /// When the files were fetched (`_fetched`, ISO 8601).
    pub fetched: Option<String>,
    pub systems: Vec<ShopSystem>,
    pub shop_count: usize,
    pub item_count: usize,
    /// The build whose item data linked rows to records (`LIVE 4.10.201.48085`); `None` when
    /// no item data was cached: names are UEX's then and nothing links.
    pub linked: Option<String>,
    /// The build's ship items (`game-ship-components`: the ship guns, missiles and components
    /// item-tags lacks) linked rows too. False: most of those rows keep UEX's name, unlinked,
    /// until the channel's game data is read (`read_game_data`).
    pub ship_items: bool,
}

/// `shop_items`: one shop's rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopItems {
    pub state: ShopDataState,
    pub folder: Option<String>,
    pub fetched: Option<String>,
    /// `None` when no shop has that id (or the files are missing).
    pub shop: Option<ShopSummary>,
    /// By category ([`ShopCategory::ALL`] order), then name.
    pub items: Vec<ShopItemRow>,
    pub linked: Option<String>,
}

/// One shop with its rows, and the words a name query matches.
#[derive(Debug, Clone)]
pub struct IndexedShop {
    pub summary: ShopSummary,
    pub items: Vec<ShopItemRow>,
    /// `normalise_place` of the terminal and of its full UEX name.
    names: Vec<String>,
    /// Lower-case words of the terminal, its UEX names and its place.
    words: Vec<String>,
}

/// Every shop of the UEX files, built once per pair of files and item data.
#[derive(Debug, Clone, Default)]
pub struct ShopIndex {
    pub fetched: Option<String>,
    /// Sorted by system, place, then name.
    pub shops: Vec<IndexedShop>,
    /// Price rows kept (with a price, at a terminal with an id).
    pub rows: usize,
    pub linked: Option<String>,
    /// See [`ShopDirectory::ship_items`]; set by the caller that knows the item data.
    pub ship_items: bool,
    /// Every item any shop lists, for the item search ([`ShopIndex::search_items`]).
    pub items: Vec<IndexedItem>,
    /// Fuzzy entries of [`Self::items`]: one per name (the game's and each UEX name).
    entries: Vec<Entry>,
    /// The item (index into [`Self::items`]) of each of [`Self::entries`].
    entry_item: Vec<usize>,
}

/// Most items an item search lists.
pub const ITEM_MATCH_LIMIT: usize = 40;

/// One item across every shop that lists it (rows of the same record, or of the same name
/// when unlinked).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopItemMatch {
    /// The record's class name when linked, else `uex:` and the lower-case name. What
    /// `shop_item_search` takes as `key`.
    pub key: String,
    pub name: String,
    /// UEX's name when it differs from the game's.
    pub uex_name: Option<String>,
    pub category: ShopCategory,
    pub record: Option<ItemRecord>,
    /// Shops that sell it (a buy price); a shop with several rows of the item counts once.
    pub sold_at: usize,
    /// Shops that pay for it (a sell price), counted like [`Self::sold_at`].
    pub bought_at: usize,
    /// Cheapest buy price.
    pub min_buy: Option<f64>,
    /// Best price a shop pays.
    pub max_sell: Option<f64>,
}

/// One shop's offer for an item: its rows of the item merged (UEX can list one record twice
/// under two names): the cheapest buy, the best sell, the newest date, stale only when every
/// row is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopItemOffer {
    /// [`ShopSummary::id`].
    pub terminal: u64,
    /// The shop (terminal) name.
    pub shop: String,
    /// The place without the system.
    pub location: String,
    pub system: Option<String>,
    pub not_on_live: bool,
    pub price_buy: Option<f64>,
    pub price_sell: Option<f64>,
    pub updated: Option<String>,
    pub stale: bool,
}

/// `shop_item_search`: the items a name matches, and the shops of one of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShopItemSearch {
    pub state: ShopDataState,
    pub folder: Option<String>,
    pub fetched: Option<String>,
    pub linked: Option<String>,
    pub query: String,
    /// Best match first, at most [`ITEM_MATCH_LIMIT`].
    pub items: Vec<ShopItemMatch>,
    /// Matches before the cap.
    pub total: usize,
    /// The item `key` named, else the best match; `None` when nothing matches.
    pub item: Option<ShopItemMatch>,
    /// Its shops, one offer each, cheapest buy first (shops without a buy price last). Not
    /// capped: at most one per terminal, and the app filters by system and re-sorts to sell.
    pub offers: Vec<ShopItemOffer>,
}

/// An item of the index: its summary, its distinct UEX names, and where its rows are: per shop
/// (index into [`ShopIndex::shops`]) the indices of that shop's rows of the item, in index order.
#[derive(Debug, Clone)]
pub struct IndexedItem {
    pub summary: ShopItemMatch,
    /// Every UEX name that differs from the game's, first seen first.
    pub uex_names: Vec<String>,
    pub rows: Vec<(usize, Vec<usize>)>,
}

/// One shop's rows of one item, merged: cheapest buy, best sell, newest date, stale only when
/// every row is.
#[derive(Debug, Clone, Default, PartialEq)]
struct Merged {
    buy: Option<f64>,
    sell: Option<f64>,
    updated: Option<String>,
    stale: bool,
}

fn merge_rows<'a>(rows: impl IntoIterator<Item = &'a ShopItemRow>) -> Merged {
    let mut m = Merged {
        stale: true,
        ..Merged::default()
    };
    let mut any = false;
    for r in rows {
        any = true;
        if let Some(b) = r.price_buy {
            m.buy = Some(m.buy.map_or(b, |x| x.min(b)));
        }
        if let Some(p) = r.price_sell {
            m.sell = Some(m.sell.map_or(p, |x| x.max(p)));
        }
        // YYYY-MM-DD: the string order is the date order, and `None` sorts first.
        if r.updated > m.updated {
            m.updated = r.updated.clone();
        }
        m.stale &= r.stale;
    }
    m.stale &= any;
    m
}

impl IndexedShop {
    fn merged(&self, rows: &[usize]) -> Merged {
        merge_rows(rows.iter().filter_map(|&ri| self.items.get(ri)))
    }
}

/// Cheapest buy price first, rows without one last; a stable sort, so ties keep their order.
/// Shared by the Shops screen's item search and the MCP `where_to_buy` tool.
pub fn cheapest_first<T>(rows: &mut [T], buy: impl Fn(&T) -> Option<f64>) {
    rows.sort_by(|a, b| {
        buy(a)
            .unwrap_or(f64::INFINITY)
            .total_cmp(&buy(b).unwrap_or(f64::INFINITY))
    });
}

/// The key of an item row: see [`ShopItemMatch::key`].
fn item_key(row: &ShopItemRow) -> String {
    match &row.record {
        Some(r) => r.class_name.clone(),
        None => format!("uex:{}", row.name.trim().to_lowercase()),
    }
}

/// Every item of the shops, by name, with one fuzzy entry per name. An entry carries no type
/// and, unlinked, an empty key, so only names (and class names) match: not the category label,
/// nor the `uex:` of [`item_key`].
fn index_items(shops: &[IndexedShop]) -> (Vec<IndexedItem>, Vec<Entry>, Vec<usize>) {
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<IndexedItem> = Vec::new();
    for (si, s) in shops.iter().enumerate() {
        for (ri, row) in s.items.iter().enumerate() {
            let key = item_key(row);
            let i = *at.entry(key.clone()).or_insert_with(|| {
                items.push(IndexedItem {
                    summary: ShopItemMatch {
                        key,
                        name: row.name.clone(),
                        uex_name: None,
                        category: row.category,
                        record: row.record.clone(),
                        sold_at: 0,
                        bought_at: 0,
                        min_buy: None,
                        max_sell: None,
                    },
                    uex_names: Vec::new(),
                    rows: Vec::new(),
                });
                items.len() - 1
            });
            let it = &mut items[i];
            // Shops come in order, so a shop's rows of the item are adjacent.
            match it.rows.last_mut() {
                Some((last, rs)) if *last == si => rs.push(ri),
                _ => it.rows.push((si, vec![ri])),
            }
            if let Some(u) = &row.uex_name
                && !it.uex_names.contains(u)
            {
                it.uex_names.push(u.clone());
            }
        }
    }
    for it in &mut items {
        let m = &mut it.summary;
        m.uex_name = it.uex_names.first().cloned();
        for (si, rs) in &it.rows {
            let g = shops[*si].merged(rs);
            if let Some(b) = g.buy {
                m.sold_at += 1;
                m.min_buy = Some(m.min_buy.map_or(b, |x| x.min(b)));
            }
            if let Some(p) = g.sell {
                m.bought_at += 1;
                m.max_sell = Some(m.max_sell.map_or(p, |x| x.max(p)));
            }
        }
    }
    items.sort_by(|a, b| {
        locale_compare(&a.summary.name, &b.summary.name)
            .then_with(|| a.summary.key.cmp(&b.summary.key))
    });
    let mut entries = Vec::new();
    let mut entry_item = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let m = &it.summary;
        let key = m
            .record
            .as_ref()
            .map(|r| r.class_name.clone())
            .unwrap_or_default();
        for name in std::iter::once(&m.name).chain(&it.uex_names) {
            entries.push(Entry {
                kind: EntryKind::Item,
                key: key.clone(),
                name: name.clone(),
                maker: None,
                ty: None,
                family: None,
            });
            entry_item.push(i);
        }
    }
    (items, entries, entry_item)
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn positive(v: Option<f64>) -> Option<f64> {
    v.filter(|p| p.is_finite() && *p > 0.0)
}

/// `Grim HEX, Yela, Crusader, Stanton` without `, Stanton`.
fn place_without_system(label: &str, system: Option<&str>) -> String {
    if let Some(s) = system
        && let Some(cut) = label.strip_suffix(s)
        && let Some(cut) = cut.trim_end().strip_suffix(',')
    {
        let cut = cut.trim();
        if !cut.is_empty() {
            return cut.to_string();
        }
    }
    label.to_string()
}

/// Items by GUID, and by lower-case name where only one non-template item has it.
struct ItemLookup<'a> {
    by_id: HashMap<&'a str, &'a ItemTags>,
    by_name: HashMap<String, Option<&'a ItemTags>>,
}

impl<'a> ItemLookup<'a> {
    fn new(items: &'a [ItemTags]) -> Self {
        let mut by_id = HashMap::new();
        let mut by_name: HashMap<String, Option<&ItemTags>> = HashMap::new();
        for i in items {
            if !i.id.is_empty() {
                by_id.entry(i.id.as_str()).or_insert(i);
            }
            if !i.template() && !i.name.trim().is_empty() {
                by_name
                    .entry(i.name.trim().to_lowercase())
                    .and_modify(|v| {
                        if v.is_some_and(|x| x.class_name != i.class_name) {
                            *v = None;
                        }
                    })
                    .or_insert(Some(i));
            }
        }
        Self { by_id, by_name }
    }

    /// The UUID when it is a record; else the one item with the UEX name.
    fn find(&self, row: &ShopPrice) -> Option<&'a ItemTags> {
        row.item_uuid
            .as_deref()
            .filter(|u| !u.is_empty())
            .and_then(|u| self.by_id.get(u).copied())
            .or_else(|| {
                let n = row.item_name.as_deref()?.trim().to_lowercase();
                self.by_name.get(&n).copied().flatten()
            })
    }
}

fn uex_category(row: &ShopPrice) -> Option<u64> {
    row.extra.get("id_category").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
    })
}

fn sort_items(items: &mut [ShopItemRow]) {
    items.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| locale_compare(&a.name, &b.name))
            .then_with(|| {
                a.price_buy
                    .unwrap_or(f64::INFINITY)
                    .total_cmp(&b.price_buy.unwrap_or(f64::INFINITY))
            })
    });
}

fn counts(items: &[ShopItemRow]) -> Vec<CategoryCount> {
    let mut m: BTreeMap<ShopCategory, usize> = BTreeMap::new();
    for i in items {
        *m.entry(i.category).or_default() += 1;
    }
    m.into_iter()
        .map(|(category, count)| CategoryCount { category, count })
        .collect()
}

/// The index of the UEX rows. `items`: the build's item data (item-tags, plus ship items it
/// lacks) to link rows to records; empty links nothing. `linked` names that build.
pub fn build_index(
    prices: &[ShopPrice],
    terminals: &[UexTerminal],
    items: &[ItemTags],
    fetched: Option<&str>,
    linked: Option<String>,
) -> ShopIndex {
    let locator = Locator::new(terminals);
    let lookup = ItemLookup::new(items);
    let as_of = fetched.map(|f| AsOf::Iso(f.to_string()));
    let reference = reference_time(as_of.as_ref());
    let by_id: HashMap<u64, &UexTerminal> = terminals
        .iter()
        .filter_map(|t| Some((t.id.filter(|i| *i >= 0.0)? as u64, t)))
        .collect();
    let mut shops: Vec<IndexedShop> = Vec::new();
    let mut at: HashMap<u64, usize> = HashMap::new();
    let mut rows = 0;
    for row in prices {
        let (buy, sell) = (positive(row.price_buy), positive(row.price_sell));
        if buy.is_none() && sell.is_none() {
            continue;
        }
        let Some(id) = row.id_terminal.filter(|i| *i >= 0.0).map(|i| i as u64) else {
            continue;
        };
        let gi = *at.entry(id).or_insert_with(|| {
            let loc = locator.locate(row);
            let system = loc.system.clone();
            let t = by_id.get(&id);
            let mut names = vec![normalise_place(&loc.terminal)];
            let mut ws = words(&loc.terminal);
            for n in [
                t.and_then(|t| t.name.as_deref()),
                t.and_then(|t| t.fullname.as_deref()),
                t.and_then(|t| t.code.as_deref()),
            ]
            .into_iter()
            .flatten()
            {
                names.push(normalise_place(n));
                ws.extend(words(n));
            }
            ws.extend(words(&loc.label));
            ws.sort();
            ws.dedup();
            shops.push(IndexedShop {
                summary: ShopSummary {
                    id,
                    name: loc.terminal.clone(),
                    location: place_without_system(&loc.label, system.as_deref()),
                    system,
                    items: 0,
                    categories: Vec::new(),
                    not_on_live: loc.live == Some(false),
                    updated: None,
                },
                items: Vec::new(),
                names,
                words: ws,
            });
            shops.len() - 1
        });
        let record = lookup.find(row);
        let game_name = record
            .filter(|x| !x.template())
            .map(|x| x.name.trim().to_string());
        let uex = row
            .item_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .map(|n| n.trim().to_string());
        let name = game_name
            .clone()
            .or_else(|| uex.clone())
            .or_else(|| row.item_uuid.clone())
            .unwrap_or_else(|| "unknown item".into());
        let uex_name = match (&game_name, &uex) {
            (Some(g), Some(u)) if u != g => Some(u.clone()),
            _ => None,
        };
        let (updated, stale) = price_age(row.date_modified, reference, STALE_DAYS);
        let s = &mut shops[gi];
        if updated.as_deref() > s.summary.updated.as_deref() {
            s.summary.updated = updated.clone();
        }
        s.items.push(ShopItemRow {
            name,
            uex_name,
            category: categorise(record.and_then(|r| r.ty.as_deref()), uex_category(row)),
            price_buy: buy,
            price_sell: sell,
            updated,
            stale,
            record: record.map(|r| ItemRecord {
                guid: r.id.clone(),
                class_name: r.class_name.clone(),
            }),
        });
        rows += 1;
    }
    for s in &mut shops {
        sort_items(&mut s.items);
        s.summary.items = s.items.len();
        s.summary.categories = counts(&s.items);
    }
    shops.sort_by(|a, b| {
        let sys = |s: &IndexedShop| s.summary.system.clone().unwrap_or_default();
        locale_compare(&sys(a), &sys(b))
            .then_with(|| locale_compare(&a.summary.location, &b.summary.location))
            .then_with(|| locale_compare(&a.summary.name, &b.summary.name))
            .then_with(|| a.summary.id.cmp(&b.summary.id))
    });
    let (items, entries, entry_item) = index_items(&shops);
    ShopIndex {
        fetched: fetched.map(str::to_string),
        shops,
        rows,
        linked,
        ship_items: false,
        items,
        entries,
        entry_item,
    }
}

/// System name for a shop without one.
pub const NO_SYSTEM: &str = "Unknown system";

impl ShopIndex {
    /// Systems, places and shops, in index order.
    pub fn directory(&self, folder: Option<String>) -> ShopDirectory {
        let mut systems: Vec<ShopSystem> = Vec::new();
        for s in &self.shops {
            let sys = s.summary.system.as_deref().unwrap_or(NO_SYSTEM);
            if systems.last().is_none_or(|x| x.name != sys) {
                systems.push(ShopSystem {
                    name: sys.to_string(),
                    locations: Vec::new(),
                });
            }
            let locs = &mut systems.last_mut().expect("pushed").locations;
            if locs.last().is_none_or(|l| l.name != s.summary.location) {
                locs.push(ShopPlaceGroup {
                    name: s.summary.location.clone(),
                    shops: Vec::new(),
                });
            }
            locs.last_mut()
                .expect("pushed")
                .shops
                .push(s.summary.clone());
        }
        ShopDirectory {
            state: ShopDataState::Ready,
            folder,
            fetched: self.fetched.clone(),
            systems,
            shop_count: self.shops.len(),
            item_count: self.rows,
            linked: self.linked.clone(),
            ship_items: self.ship_items,
        }
    }

    pub fn shop(&self, id: u64) -> Option<&IndexedShop> {
        self.shops.iter().find(|s| s.summary.id == id)
    }

    /// One shop's rows; `shop: None` when no shop has the id.
    pub fn items(&self, id: u64, folder: Option<String>) -> ShopItems {
        let s = self.shop(id);
        ShopItems {
            state: ShopDataState::Ready,
            folder,
            fetched: self.fetched.clone(),
            shop: s.map(|s| s.summary.clone()),
            items: s.map(|s| s.items.clone()).unwrap_or_default(),
            linked: self.linked.clone(),
        }
    }

    /// Shops a terminal name or id names, best tier only: (1) the id, (2) one of its names
    /// after normalising, (3) a name starting with the query, (4) every query word is a word
    /// or word start of its names or place.
    pub fn find(&self, query: &str) -> Vec<&IndexedShop> {
        let q = query.trim();
        if let Ok(id) = q.parse::<u64>()
            && let Some(s) = self.shop(id)
        {
            return vec![s];
        }
        let n = normalise_place(q);
        let qw = words(q);
        if n.is_empty() || qw.is_empty() {
            return Vec::new();
        }
        let tier = |s: &IndexedShop| -> Option<u8> {
            if s.names.contains(&n) {
                Some(1)
            } else if n.len() >= 3 && s.names.iter().any(|x| x.starts_with(&n)) {
                Some(2)
            } else if qw.iter().all(|w| {
                s.words
                    .iter()
                    .any(|x| x == w || (w.chars().count() >= 3 && x.starts_with(w.as_str())))
            }) {
                Some(3)
            } else {
                None
            }
        };
        let best = self.shops.iter().filter_map(tier).min();
        match best {
            Some(b) => self.shops.iter().filter(|s| tier(s) == Some(b)).collect(),
            None => Vec::new(),
        }
    }

    /// Items whose game or UEX name matches the query, best first ([`fuzzy::rank_scored`],
    /// the scorer of the `search` tool), one row per item. An empty query matches nothing.
    pub fn match_items(&self, query: &str) -> Vec<&IndexedItem> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        // `rank_scored` hands back references into `entries`: an entry's address names it.
        let item_of: HashMap<*const Entry, usize> = self
            .entries
            .iter()
            .zip(&self.entry_item)
            .map(|(e, &i)| (e as *const Entry, i))
            .collect();
        let mut seen = std::collections::HashSet::new();
        fuzzy::rank_scored(&self.entries, query)
            .into_iter()
            .filter_map(|(e, _)| item_of.get(&(e as *const Entry)).copied())
            .filter(|&i| seen.insert(i))
            .filter_map(|i| self.items.get(i))
            .collect()
    }

    /// One item's shops, one offer per shop (its rows merged), cheapest buy first
    /// ([`cheapest_first`]); ties in index order (system, place, shop).
    pub fn offers(&self, item: &IndexedItem) -> Vec<ShopItemOffer> {
        let mut v: Vec<ShopItemOffer> = item
            .rows
            .iter()
            .filter_map(|(si, rs)| {
                let s = self.shops.get(*si)?;
                let m = s.merged(rs);
                Some(ShopItemOffer {
                    terminal: s.summary.id,
                    shop: s.summary.name.clone(),
                    location: s.summary.location.clone(),
                    system: s.summary.system.clone(),
                    not_on_live: s.summary.not_on_live,
                    price_buy: m.buy,
                    price_sell: m.sell,
                    updated: m.updated,
                    stale: m.stale,
                })
            })
            .collect();
        cheapest_first(&mut v, |o| o.price_buy);
        v
    }

    /// `shop_item_search`: the items `query` matches (capped) and the shops of the item
    /// `key` names (any item of the index, matched or not), else of the best match.
    pub fn search_items(
        &self,
        query: &str,
        key: Option<&str>,
        folder: Option<String>,
    ) -> ShopItemSearch {
        let found = self.match_items(query);
        let chosen = key
            .filter(|k| !k.is_empty())
            .and_then(|k| self.items.iter().find(|i| i.summary.key == k))
            .or_else(|| found.first().copied());
        let offers = chosen.map(|i| self.offers(i)).unwrap_or_default();
        ShopItemSearch {
            state: ShopDataState::Ready,
            folder,
            fetched: self.fetched.clone(),
            linked: self.linked.clone(),
            query: query.to_string(),
            total: found.len(),
            items: found
                .iter()
                .take(ITEM_MATCH_LIMIT)
                .map(|i| i.summary.clone())
                .collect(),
            item: chosen.map(|i| i.summary.clone()),
            offers,
        }
    }
}

/// The item search while the UEX files are missing.
pub fn missing_item_search(folder: Option<String>, query: &str) -> ShopItemSearch {
    ShopItemSearch {
        state: ShopDataState::Missing,
        folder,
        fetched: None,
        linked: None,
        query: query.to_string(),
        items: Vec::new(),
        total: 0,
        item: None,
        offers: Vec::new(),
    }
}

/// The answer while the UEX files are missing.
pub fn missing_directory(folder: Option<String>) -> ShopDirectory {
    ShopDirectory {
        state: ShopDataState::Missing,
        folder,
        fetched: None,
        systems: Vec::new(),
        shop_count: 0,
        item_count: 0,
        linked: None,
        ship_items: false,
    }
}

pub fn missing_items(folder: Option<String>) -> ShopItems {
    ShopItems {
        state: ShopDataState::Missing,
        folder,
        fetched: None,
        shop: None,
        items: Vec::new(),
        linked: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn terminals() -> Vec<UexTerminal> {
        serde_json::from_value(json!([
            {"id": 1, "nickname": "Guns Ruin", "name": "Guns - Ruin Station", "displayname": "Ruin Station", "orbit_name": "Terminus", "star_system_name": "Pyro"},
            {"id": 2, "nickname": "Dumper's GrimHEX", "name": "Dumper's Depot - GrimHEX", "displayname": "Grim HEX", "moon_name": "Yela", "orbit_name": "Crusader", "star_system_name": "Stanton"},
            {"id": 3, "nickname": "Skutters Grim", "displayname": "Grim HEX", "moon_name": "Yela", "orbit_name": "Crusader", "star_system_name": "Stanton", "is_available_live": 0},
            {"id": 4, "nickname": "Cafe Lorville", "city_name": "Lorville", "planet_name": "Hurston", "star_system_name": "Stanton"}
        ]))
        .unwrap()
    }

    fn row(name: &str, uuid: Option<&str>, terminal: u64, cat: u64, buy: f64) -> ShopPrice {
        let mut v = json!({"item_name": name, "id_terminal": terminal, "id_category": cat, "price_buy": buy, "date_modified": 1794268800});
        if let Some(u) = uuid {
            v["item_uuid"] = json!(u);
        }
        serde_json::from_value(v).unwrap()
    }

    fn items() -> Vec<ItemTags> {
        let it = |name: &str, class: &str, id: &str, ty: &str| ItemTags {
            name: name.into(),
            class_name: class.into(),
            id: id.into(),
            ty: Some(ty.into()),
            ..Default::default()
        };
        vec![
            it(
                "FS-9 LMG",
                "behr_lmg_ballistic_01",
                "id-fs9",
                "WeaponPersonal",
            ),
            it("Ignite II Missile", "misl_s02_ignite", "id-ign", "Missile"),
            it(
                "Pembroke Helmet",
                "rsi_helmet_01",
                "id-helm",
                "Char_Armor_Helmet",
            ),
            it("Twin Name", "twin_a", "id-ta", "Food"),
            it("Twin Name", "twin_b", "id-tb", "Food"),
        ]
    }

    fn index() -> ShopIndex {
        let prices = vec![
            row("FS-9 LMG", Some("id-fs9"), 2, 18, 6900.0),
            // UEX's name differs; the uuid links it.
            row("Ignite II", Some("id-ign"), 2, 34, 1200.0),
            // No uuid: the unique name links it.
            row("Pembroke Helmet", None, 2, 3, 3000.0),
            // Not in item-tags: UEX category 32 says ship weapon.
            row("M5A Cannon", Some("id-m5a"), 2, 32, 78262.0),
            // Two items share the name: no link, UEX category says consumable.
            row("Twin Name", None, 4, 63, 5.0),
            row("FS-9 LMG", Some("id-fs9"), 1, 18, 7029.0),
            row("Thing", None, 3, 999, 10.0),
            // No price: dropped; no terminal id: dropped.
            row("Free", None, 1, 18, 0.0),
            serde_json::from_value(json!({"item_name": "Lost", "price_buy": 5})).unwrap(),
        ];
        build_index(
            &prices,
            &terminals(),
            &items(),
            Some("2026-11-21T00:00:00.000Z"),
            Some("LIVE 4.10".into()),
        )
    }

    #[test]
    fn categories_prefer_the_game_type_then_uex() {
        assert_eq!(
            categorise(Some("WeaponPersonal"), Some(63)),
            ShopCategory::FpsWeapons
        );
        assert_eq!(
            categorise(Some("Char_Clothing_Feet"), None),
            ShopCategory::Armour
        );
        assert_eq!(categorise(Some("Misc"), Some(29)), ShopCategory::Mining);
        assert_eq!(
            categorise(Some("Missile"), Some(34)),
            ShopCategory::Missiles
        );
        assert_eq!(categorise(None, Some(32)), ShopCategory::ShipWeapons);
        assert_eq!(categorise(None, Some(23)), ShopCategory::ShipComponents);
        assert_eq!(categorise(None, Some(12345)), ShopCategory::Other);
        assert_eq!(categorise(None, None), ShopCategory::Other);
        assert_eq!(
            ShopCategory::parse("Missiles"),
            Some(ShopCategory::Missiles)
        );
        assert_eq!(ShopCategory::parse("armor"), Some(ShopCategory::Armour));
        assert_eq!(
            ShopCategory::parse("ship_components"),
            Some(ShopCategory::ShipComponents)
        );
        assert_eq!(
            ShopCategory::parse("Ship weapons"),
            Some(ShopCategory::ShipWeapons)
        );
        assert_eq!(ShopCategory::parse("zzz"), None);
        // Keys round-trip through serde.
        for c in ShopCategory::ALL {
            assert_eq!(serde_json::to_value(c).unwrap(), json!(c.key()));
        }
    }

    #[test]
    fn rows_group_by_system_place_and_shop() {
        let ix = index();
        assert_eq!(ix.rows, 7);
        let d = ix.directory(Some("/uex".into()));
        assert_eq!(d.state, ShopDataState::Ready);
        assert_eq!((d.shop_count, d.item_count), (4, 7));
        let systems: Vec<&str> = d.systems.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(systems, ["Pyro", "Stanton"]);
        let stanton = &d.systems[1];
        let places: Vec<&str> = stanton.locations.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(places, ["Grim HEX, Yela, Crusader", "Lorville, Hurston"]);
        let grim: Vec<&str> = stanton.locations[0]
            .shops
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(grim, ["Dumper's GrimHEX", "Skutters Grim"]);
        let dumper = &stanton.locations[0].shops[0];
        assert_eq!(dumper.items, 4);
        assert_eq!(
            dumper.categories,
            vec![
                CategoryCount {
                    category: ShopCategory::FpsWeapons,
                    count: 1
                },
                CategoryCount {
                    category: ShopCategory::Armour,
                    count: 1
                },
                CategoryCount {
                    category: ShopCategory::ShipWeapons,
                    count: 1
                },
                CategoryCount {
                    category: ShopCategory::Missiles,
                    count: 1
                },
            ]
        );
        assert_eq!(dumper.updated.as_deref(), Some("2026-11-10"));
        assert!(stanton.locations[0].shops[1].not_on_live);
        assert_eq!(d.linked.as_deref(), Some("LIVE 4.10"));
    }

    #[test]
    fn items_link_by_uuid_then_unique_name() {
        let ix = index();
        let it = ix.items(2, None);
        let names: Vec<(&str, ShopCategory)> = it
            .items
            .iter()
            .map(|i| (i.name.as_str(), i.category))
            .collect();
        assert_eq!(
            names,
            [
                ("FS-9 LMG", ShopCategory::FpsWeapons),
                ("Pembroke Helmet", ShopCategory::Armour),
                ("M5A Cannon", ShopCategory::ShipWeapons),
                ("Ignite II Missile", ShopCategory::Missiles),
            ]
        );
        let ign = &it.items[3];
        assert_eq!(ign.uex_name.as_deref(), Some("Ignite II"));
        assert_eq!(ign.record.as_ref().unwrap().guid, "id-ign");
        assert_eq!(
            it.items[1].record.as_ref().unwrap().class_name,
            "rsi_helmet_01"
        );
        assert!(it.items[2].record.is_none());
        assert!(!it.items[0].stale);
        let twin = ix.items(4, None);
        assert!(twin.items[0].record.is_none());
        assert_eq!(twin.items[0].category, ShopCategory::Consumables);
        assert!(ix.items(99, None).shop.is_none());
        // No item data: UEX names and categories, nothing linked.
        let bare = build_index(
            &[row("FS-9 LMG", Some("id-fs9"), 2, 18, 6900.0)],
            &terminals(),
            &[],
            None,
            None,
        );
        let r = &bare.shops[0].items[0];
        assert!(r.record.is_none());
        assert_eq!(r.category, ShopCategory::FpsWeapons);
    }

    #[test]
    fn find_takes_ids_names_and_words() {
        let ix = index();
        let ids = |q: &str| -> Vec<u64> { ix.find(q).iter().map(|s| s.summary.id).collect() };
        assert_eq!(ids("2"), [2]);
        assert_eq!(ids("dumper's grimhex"), [2]);
        assert_eq!(ids("Dumpers Depot - GrimHEX"), [2]);
        assert_eq!(ids("skutters"), [3]);
        // A place: every shop there.
        assert_eq!(ids("grim hex"), [2, 3]);
        assert_eq!(ids("ruin"), [1]);
        assert!(ids("orison").is_empty());
        assert!(ids("  ").is_empty());
    }

    #[test]
    fn item_search_matches_names_and_lists_shops_cheapest_first() {
        let ix = index();
        let r = ix.search_items("FS-9", None, Some("/uex".into()));
        assert_eq!(r.state, ShopDataState::Ready);
        let best = r.item.as_ref().unwrap();
        assert_eq!(best.key, "behr_lmg_ballistic_01");
        assert_eq!(r.items[0].key, best.key);
        assert_eq!((best.sold_at, best.bought_at), (2, 0));
        assert_eq!(best.min_buy, Some(6900.0));
        assert_eq!(best.category, ShopCategory::FpsWeapons);
        let shops: Vec<(u64, Option<f64>)> =
            r.offers.iter().map(|o| (o.terminal, o.price_buy)).collect();
        assert_eq!(shops, [(2, Some(6900.0)), (1, Some(7029.0))]);
        assert_eq!(r.offers[0].shop, "Dumper's GrimHEX");
        assert_eq!(r.offers[0].system.as_deref(), Some("Stanton"));
        assert_eq!(r.offers[1].location, "Ruin Station, Terminus");
        // UEX's name finds the linked item, once.
        let r = ix.search_items("ignite ii", None, None);
        let keys: Vec<&str> = r.items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys.iter().filter(|k| **k == "misl_s02_ignite").count(), 1);
        assert_eq!(r.item.unwrap().name, "Ignite II Missile");
        // An unlinked row keys on its name; `key` picks an item the query did not match.
        let r = ix.search_items("lmg", Some("uex:twin name"), None);
        assert_eq!(r.item.as_ref().unwrap().name, "Twin Name");
        assert!(r.item.unwrap().record.is_none());
        assert_eq!(r.offers.len(), 1);
        assert_eq!(r.offers[0].terminal, 4);
        // An unknown key falls back to the best match; an empty query matches nothing.
        let r = ix.search_items("lmg", Some("nope"), None);
        assert_eq!(r.item.unwrap().key, "behr_lmg_ballistic_01");
        let r = ix.search_items("  ", None, None);
        assert!(r.items.is_empty() && r.item.is_none() && r.offers.is_empty());
        assert!(ix.search_items("zzzzqqq", None, None).item.is_none());
    }

    #[test]
    fn item_search_caps_matches_and_works_without_item_data() {
        let prices: Vec<ShopPrice> = (0..60)
            .map(|n| row(&format!("Widget {n:02}"), None, 2, 999, 10.0 + n as f64))
            .collect();
        // No item data: UEX names only, nothing links.
        let ix = build_index(&prices, &terminals(), &[], None, None);
        let r = ix.search_items("widget", None, None);
        assert_eq!(r.total, 60);
        assert_eq!(r.items.len(), ITEM_MATCH_LIMIT);
        assert!(r.items.iter().all(|i| i.record.is_none()));
        assert!(r.linked.is_none());
        let m = missing_item_search(Some("/uex".into()), "widget");
        assert_eq!(m.state, ShopDataState::Missing);
        assert!(m.items.is_empty() && m.offers.is_empty() && m.item.is_none());
        assert_eq!(m.query, "widget");
    }

    #[test]
    fn item_search_merges_a_shops_rows_of_one_item() {
        // UEX lists one record twice at one terminal under two names (seen in real data).
        let dated = |name: &str, buy: f64, sell: f64, when: i64| -> ShopPrice {
            serde_json::from_value(
                json!({"item_name": name, "item_uuid": "id-fs9", "id_terminal": 2,
                "id_category": 18, "price_buy": buy, "price_sell": sell, "date_modified": when}),
            )
            .unwrap()
        };
        let prices = vec![
            // Old (stale), cheaper to buy.
            dated("FS-9 LMG", 6900.0, 50.0, 1_780_000_000),
            // Fresh, pays more.
            dated("Behring FS9", 7100.0, 60.0, 1_794_268_800),
            row("FS-9 LMG", Some("id-fs9"), 1, 18, 7029.0),
        ];
        let ix = build_index(
            &prices,
            &terminals(),
            &items(),
            Some("2026-11-21T00:00:00.000Z"),
            None,
        );
        let r = ix.search_items("fs-9", None, None);
        let m = r.item.as_ref().unwrap();
        assert_eq!(m.key, "behr_lmg_ballistic_01");
        assert_eq!((m.sold_at, m.bought_at), (2, 1));
        assert_eq!((m.min_buy, m.max_sell), (Some(6900.0), Some(60.0)));
        let terminals: Vec<u64> = r.offers.iter().map(|o| o.terminal).collect();
        assert_eq!(terminals, [2, 1]);
        let o = &r.offers[0];
        assert_eq!((o.price_buy, o.price_sell), (Some(6900.0), Some(60.0)));
        let fresh = ix
            .shops
            .iter()
            .flat_map(|s| &s.items)
            .find(|i| i.uex_name.as_deref() == Some("Behring FS9"));
        assert_eq!(o.updated, fresh.unwrap().updated);
        assert!(!o.stale);
        // Both UEX names find it; the first that differs from the game's is the one shown.
        assert_eq!(m.uex_name.as_deref(), Some("Behring FS9"));
        let r = ix.search_items("behring", None, None);
        assert_eq!(r.item.unwrap().key, "behr_lmg_ballistic_01");
        assert_eq!(r.total, 1);
        // Stale only when every row is.
        let shop = &ix.shops[ix.shops.iter().position(|s| s.summary.id == 2).unwrap()];
        let old: Vec<usize> = (0..shop.items.len())
            .filter(|&i| shop.items[i].stale)
            .collect();
        assert_eq!(old.len(), 1);
        assert!(shop.merged(&old).stale);
        assert!(!shop.merged(&[]).stale);
    }

    #[test]
    fn item_search_matches_names_not_category_words_or_the_uex_prefix() {
        let ix = index();
        // "Thing" and "Twin Name" are unlinked (`uex:` keys); the rest are category labels.
        assert_eq!(ix.search_items("uex", None, None).total, 0);
        assert_eq!(ix.search_items("torpedoes", None, None).total, 0);
        assert_eq!(ix.search_items("clothing", None, None).total, 0);
        assert_eq!(ix.search_items("other", None, None).total, 0);
        // Class names still match.
        let r = ix.search_items("misl_s02", None, None);
        assert_eq!(r.item.unwrap().key, "misl_s02_ignite");
    }

    #[test]
    fn cheapest_first_puts_rows_without_a_buy_price_last() {
        let mut v = [(Some(5.0), 'a'), (None, 'b'), (Some(1.0), 'c'), (None, 'd')];
        cheapest_first(&mut v, |r| r.0);
        let order: String = v.iter().map(|r| r.1).collect();
        assert_eq!(order, "cabd");
    }

    #[test]
    fn place_drops_only_the_trailing_system() {
        assert_eq!(
            place_without_system("Grim HEX, Yela, Crusader, Stanton", Some("Stanton")),
            "Grim HEX, Yela, Crusader"
        );
        assert_eq!(place_without_system("Stanton", Some("Stanton")), "Stanton");
        assert_eq!(place_without_system("Ghost Kiosk", None), "Ghost Kiosk");
    }

    /// Real UEX files (`STARORG_UEX_DIR`) and item-tags (`STARORG_ITEM_TAGS`, a
    /// `game-item-tags.json`): shops, rows and the share linked per category.
    #[test]
    #[ignore = "needs real UEX files"]
    fn real_uex_files() {
        let read = |p: std::path::PathBuf| -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap()
        };
        let dir = std::path::PathBuf::from(std::env::var_os("STARORG_UEX_DIR").unwrap());
        let p = read(dir.join("uex-item-prices.json"));
        let t = read(dir.join("uex-terminals.json"));
        let mut items: Vec<ItemTags> = std::env::var_os("STARORG_ITEM_TAGS")
            .map(|f| serde_json::from_value(read(f.into())["items"].clone()).unwrap())
            .unwrap_or_default();
        if let Some(f) = std::env::var_os("STARORG_SHIP_COMPONENTS") {
            let extra = crate::tools::extra_from_components(&items, &read(f.into()));
            eprintln!("ship-components adds {} items", extra.len());
            items.extend(extra);
        }
        let prices: Vec<ShopPrice> = serde_json::from_value(p["prices"].clone()).unwrap();
        let terms: Vec<UexTerminal> = serde_json::from_value(t["terminals"].clone()).unwrap();
        let t0 = std::time::Instant::now();
        let ix = build_index(&prices, &terms, &items, p["_fetched"].as_str(), None);
        let d = ix.directory(None);
        eprintln!(
            "built in {:?}: {} shops, {} rows of {}, {} systems, {} places",
            t0.elapsed(),
            d.shop_count,
            d.item_count,
            prices.len(),
            d.systems.len(),
            d.systems.iter().map(|s| s.locations.len()).sum::<usize>()
        );
        let mut per: BTreeMap<ShopCategory, (usize, usize)> = BTreeMap::new();
        for s in &ix.shops {
            for i in &s.items {
                let e = per.entry(i.category).or_default();
                e.0 += 1;
                e.1 += usize::from(i.record.is_some());
            }
        }
        for (c, (n, linked)) in &per {
            eprintln!(
                "{:<22} {n:>6} rows, {linked:>6} linked ({:.0} %)",
                c.label(),
                *linked as f64 * 100.0 / *n as f64
            );
        }
        for s in &d.systems {
            eprintln!(
                "{}: {} places, {} shops",
                s.name,
                s.locations.len(),
                s.locations.iter().map(|l| l.shops.len()).sum::<usize>()
            );
        }
        for q in std::env::var("STARORG_SHOP_QUERIES")
            .unwrap_or_default()
            .split(',')
            .filter(|q| !q.is_empty())
        {
            let hits = ix.find(q);
            eprintln!("find {q:?}: {} shop(s)", hits.len());
            for s in hits.iter().take(4) {
                let m = &s.summary;
                let cats: Vec<String> = m
                    .categories
                    .iter()
                    .map(|c| format!("{} {}", c.category.key(), c.count))
                    .collect();
                eprintln!(
                    "  {} {} | {} | {} items | {}",
                    m.id,
                    m.name,
                    m.location,
                    m.items,
                    cats.join(", ")
                );
            }
        }
        assert!(d.shop_count > 100);
    }
}
