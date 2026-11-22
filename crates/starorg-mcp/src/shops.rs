//! The shop browser: what every UEX terminal sells, grouped by star system, place and shop,
//! each item in a readable category and linked to its game record when the build's item data
//! knows it. The app's Shops screen (`shop_inventory`, `shop_items`) and the `shop_inventory`
//! MCP tool answer from one [`ShopIndex`] built from the two UEX files.
//!
//! Shop stock is server-side and in no game file: every row is UEX data, reported by players.
//!
//! Categories: the game's item type when the row is linked to a record (authoritative), else
//! the UEX category id the price row carries (every row has one; UEX lists ship guns, mining
//! lasers and tractor beams that `game-item-tags` does not). See [`categorise`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use starorg_gamedata::collate::locale_compare;
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
    ShopIndex {
        fetched: fetched.map(str::to_string),
        shops,
        rows,
        linked,
        ship_items: false,
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
