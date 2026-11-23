//! Where a record comes from, typed for the app's explorer Sources card: the shops that
//! sell the item (UEX files the user supplied), the commodity kiosks that pay best for a
//! resource (UEX commodity prices), the loot containers that can hold it (type,
//! size, rarity, rough odds), where the ore it names is mined and the quality it comes in. [`crate::tools::Tools::sources`] fills it from the same datasets the
//! `where_found` and `where_mined` tools read, and `where_found` answers with these rows too
//! (keys in camelCase there, as every tool answers).

use serde::{Deserialize, Serialize};
use starorg_gamedata::query::mined::area_place;
pub use starorg_gamedata::query::mined::{MinedBelt, MinedReport, MinedSpot};
use starorg_gamedata::query::obtain::{AreaSummary, ObtainReport, SpawnKind, area_label};
use starorg_gamedata::query::quality::QualityResource;
pub use starorg_gamedata::query::quality::{
    OreQuality, QualityOverride, QualityRange, QualityStep,
};
use starorg_gamedata::query::sells::SellRow;
use starorg_gamedata::query::shops::LocatedShop;

/// The card's caps: at most this many container rows (the rest are counted in
/// `container_total`) and loot tables (counted in `table_total`).
pub const MAX_CONTAINERS: usize = 60;
pub const MAX_TABLES: usize = 12;
/// At most this many shop rows (the rest are counted in `total`).
pub const MAX_SHOPS: usize = 40;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RecordSources {
    /// `None` when the record is no item the item data knows.
    pub buy: Option<BuySources>,
    /// Where it sells best at commodity kiosks; `None` when the record is no resource (or
    /// cargo holding one).
    #[serde(default)]
    pub sell: Option<SellSources>,
    /// `None` when the record is no item the loot data knows.
    pub loot: Option<LootSources>,
    /// Where the ores the record names are mined; empty when it names none.
    pub mined: Vec<MinedReport>,
    /// The quality the record (a resource) or its mined ores come in; empty for others.
    pub quality: Vec<OreQuality>,
}

/// Shops that sell the item. Shop stock is server-side and in no game file: the rows are
/// UEX (player-reported) prices, read only from files the user put in `folder`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BuySources {
    /// False when the UEX files are missing: `offers` is then empty for that reason.
    pub loaded: bool,
    /// Where the UEX files go (`<cache>/mcp/uex`).
    pub folder: Option<String>,
    /// Cheapest first.
    pub offers: Vec<ShopOffer>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ShopOffer {
    /// The terminal (`Dumper's GrimHEX`).
    pub terminal: String,
    /// Place, moon or orbit, planet, system.
    pub location: String,
    pub price_buy: f64,
    /// Day UEX last confirmed the row (YYYY-MM-DD).
    pub updated: Option<String>,
    /// Older than 30 days.
    pub stale: bool,
    /// UEX marks the terminal unavailable on LIVE.
    pub not_on_live: bool,
}

/// Rows that sell the item (a buy price above 0), cheapest first, at most `max`.
pub fn buy_sources(
    shops: &[LocatedShop],
    loaded: bool,
    folder: Option<String>,
    max: usize,
) -> BuySources {
    let mut offers: Vec<ShopOffer> = shops
        .iter()
        .filter_map(|s| {
            Some(ShopOffer {
                terminal: s.terminal.clone(),
                location: s.location.clone(),
                price_buy: s.row.price_buy.filter(|p| *p > 0.0)?,
                updated: s.updated.clone(),
                stale: s.stale,
                not_on_live: s.live == Some(false),
            })
        })
        .collect();
    offers.sort_by(|a, b| a.price_buy.total_cmp(&b.price_buy));
    let total = offers.len();
    offers.truncate(max);
    BuySources {
        loaded,
        folder,
        offers,
        total,
    }
}

/// Kiosks listed per commodity (the card and the in-game sell line).
pub const MAX_SELLS: usize = 3;

/// Commodity kiosks that buy the resource: UEX (player-reported) prices per SCU, read only
/// from `uex-commodity-prices.json` and `uex-terminals.json` in `folder`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SellSources {
    /// False when either file is missing: `offers` is then empty for that reason.
    pub loaded: bool,
    pub folder: Option<String>,
    /// The resource (`ResourceType.Laranite`) and its name as shown (`Laranite`).
    pub resource: String,
    pub name: String,
    /// Best price first, at most [`MAX_SELLS`].
    pub offers: Vec<SellOffer>,
    /// Kiosks that buy it in all.
    pub total: usize,
    /// When the prices were fetched (the file's `_fetched`).
    pub fetched: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SellOffer {
    /// The terminal (`TDD Area 18`).
    pub terminal: String,
    /// Place, moon or orbit, planet, system.
    pub location: String,
    /// What the kiosk pays, aUEC per SCU.
    pub price_sell: f64,
    /// SCU the kiosk holds, when UEX knows.
    pub scu_stock: Option<f64>,
    /// Day UEX last confirmed the row (YYYY-MM-DD).
    pub updated: Option<String>,
    /// Older than 30 days.
    pub stale: bool,
}

/// The card's sell section from ranked rows ([`starorg_gamedata::query::sells::best_sells`]).
pub fn sell_sources(
    rows: &[SellRow],
    resource: &QualityResource,
    loaded: bool,
    folder: Option<String>,
    fetched: Option<String>,
) -> SellSources {
    SellSources {
        loaded,
        folder,
        resource: resource.record_name.clone(),
        name: resource
            .display_name
            .clone()
            .unwrap_or_else(|| resource.name.clone()),
        offers: rows
            .iter()
            .take(MAX_SELLS)
            .map(|r| SellOffer {
                terminal: r.terminal.clone(),
                location: r.location.clone(),
                price_sell: r.price_sell,
                scu_stock: r.scu_stock,
                updated: r.updated.clone(),
                stale: r.stale,
            })
            .collect(),
        total: rows.len(),
        fetched,
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LootSources {
    /// The item's display name.
    pub name: String,
    /// `Some(false)` when the game bars it from loot.
    pub can_loot: Option<bool>,
    pub reason: Option<String>,
    /// The item's own loot rarity tag (`Rare`).
    pub rarity: Option<String>,
    /// One row per area, best spawn first.
    pub containers: Vec<LootContainer>,
    pub container_total: usize,
    /// Loot tables that roll it but no area spawns (enemies, bosses, missions).
    pub tables: Vec<LootTableOdds>,
    pub table_total: usize,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LootContainer {
    /// Readable area (`caves: loot caves occupied rock`).
    pub area: String,
    /// The place when the records do not name it (`Rock Cracker, Nyx`).
    pub place: Option<String>,
    /// The spawn config (`Weapons Large Rare`).
    pub spawn_point: Option<String>,
    /// "loot" (rolls a table), "fixed" (always holds it) or "loose" (lies there).
    pub kind: String,
    /// Container name (`Standard Container`), else its entity.
    pub container: String,
    /// What the spot is for, from its `LootContents` tag (`Armour`, `Weapons`).
    pub contents: Option<String>,
    /// From its `LootSize` tag (`Large`).
    pub size: Option<String>,
    /// From its `LootRarity` tag, else the last word of the config name.
    pub rarity: Option<String>,
    /// Share of the spawn config's slots that are this container (0..1).
    pub slot_share: f64,
    /// Rough odds one spawned container holds it (0..1).
    pub odds_per_box: f64,
    /// Chance per loot roll (0..1).
    pub per_roll: Option<f64>,
    pub table: Option<String>,
    /// Spawn configs in the area that can produce it.
    pub configs: usize,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LootTableOdds {
    pub table: String,
    pub per_roll: f64,
}

const RARITIES: &[&str] = &["Common", "Uncommon", "Rare", "Epic", "Legendary"];

/// The value after `LootGeneration/<what>/` in spawn tags (`LootSize` -> `Large`).
fn tag(tags: &[String], what: &str) -> Option<String> {
    let p = format!("LootGeneration/{what}/");
    tags.iter()
        .find_map(|t| t.strip_prefix(&p))
        .map(|v| v.replace('/', " "))
}

fn kind_name(k: SpawnKind) -> &'static str {
    match k {
        SpawnKind::Loot => "loot",
        SpawnKind::Fixed => "fixed",
        SpawnKind::Loose => "loose",
    }
}

/// One card row: the best spawn of an area that holds the item.
pub fn loot_container(a: &AreaSummary) -> LootContainer {
    let h = &a.best;
    let rarity = tag(&h.spawn_tags, "LootRarity").or_else(|| {
        h.config.as_deref().and_then(|c| {
            let last = c.rsplit(' ').next()?;
            RARITIES
                .iter()
                .find(|r| r.eq_ignore_ascii_case(last))
                .map(|r| (*r).to_string())
        })
    });
    LootContainer {
        area: area_label(&a.area),
        place: area_place(&a.area).map(str::to_string),
        spawn_point: h.config.clone(),
        kind: kind_name(h.kind).into(),
        container: h.entity_name.clone().unwrap_or_else(|| h.entity.clone()),
        contents: tag(&h.spawn_tags, "LootContents"),
        size: tag(&h.spawn_tags, "LootSize"),
        rarity,
        slot_share: h.slot_share,
        odds_per_box: h.score,
        per_roll: h.per_roll,
        table: h.table.clone(),
        configs: a.configs,
    }
}

/// The loot half of the card from an obtain report, keeping at most `max_containers` rows
/// and `max_tables` tables.
pub fn loot_sources(r: &ObtainReport, max_containers: usize, max_tables: usize) -> LootSources {
    let containers = r
        .areas
        .iter()
        .take(max_containers)
        .map(loot_container)
        .collect();
    LootSources {
        name: r.item.name.clone(),
        can_loot: r.eligibility.can_loot,
        reason: r.eligibility.reason.clone(),
        rarity: r.eligibility.rarity.clone(),
        containers,
        container_total: r.areas.len(),
        tables: r
            .other_tables
            .iter()
            .take(max_tables)
            .map(|t| LootTableOdds {
                table: t.table.clone(),
                per_roll: t.per_roll,
            })
            .collect(),
        table_total: r.other_tables.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_read_after_their_prefix() {
        let t = vec![
            "LootGeneration/LootContents/Specific/WeaponsRack".to_string(),
            "LootGeneration/LootSize/Large".to_string(),
        ];
        assert_eq!(tag(&t, "LootSize").as_deref(), Some("Large"));
        assert_eq!(
            tag(&t, "LootContents").as_deref(),
            Some("Specific WeaponsRack")
        );
        assert_eq!(tag(&t, "LootRarity"), None);
    }
}
