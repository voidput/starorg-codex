//! The app's Commodities screen (`commodity_list`, `commodity_detail`): every tradable
//! resource of the build's `game-commodities`, and one in full on request, typed in
//! [`starorg_gamedata::query::commodities`]. [`crate::tools::Tools`] answers both from data
//! built once per build ([`crate::store::Store::typed`]) and, for kiosk prices, once per
//! set of UEX files ([`crate::store::Store::typed_uex`]).

use serde_json::Value;
pub use starorg_gamedata::query::commodities::*;
pub use starorg_gamedata::query::mined::{MiningSpawns, where_mined};
use starorg_gamedata::query::quality::QualityResource;
use starorg_gamedata::query::sells::CommodityPrice;
pub use starorg_gamedata::query::sells::KioskPrice;
use starorg_gamedata::query::shops::UexTerminal;
pub use starorg_gamedata::query::types::LootArea;

/// [`Kiosks::new`] over the two UEX files as parsed JSON (`{ _fetched, prices }` and
/// `{ terminals }`); rows that do not parse are left out.
pub fn kiosks_from_files(
    prices: &Value,
    terminals: &Value,
    resources: &[QualityResource],
    folder: Option<String>,
) -> Kiosks {
    let price_rows: Vec<CommodityPrice> = rows(prices, "prices");
    let terms: Vec<UexTerminal> = rows(terminals, "terminals");
    let meta = MarketMeta {
        loaded: true,
        fetched: prices
            .get("_fetched")
            .and_then(Value::as_str)
            .map(str::to_string),
        folder,
    };
    Kiosks::new(&price_rows, &terms, resources, meta)
}

fn rows<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Vec<T> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The list without a channel or before the game data is read.
pub fn empty_directory(state: CommodityDataState, market: MarketMeta) -> CommodityDirectory {
    CommodityDirectory {
        state,
        build: None,
        commodities: Vec::new(),
        market,
    }
}
