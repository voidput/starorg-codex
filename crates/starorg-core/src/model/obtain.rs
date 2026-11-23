//! Where to get an item or commodity: the neutral input of the `obtain-hints` generator
//! (spec obtain-hints).
//!
//! `starorg-core` names none of the gamedata dataset types: a caller that has them (the MCP
//! server's `Tools::obtain_hints`) fills this model, and apply hands it to
//! [`crate::enhance::generate_with`]. Craft and contract rewards are not here: they come
//! from apply's own [`super::GameData`] (`blueprints`, `contracts`, `blueprint_pools`).
//!
//! Like [`super::GameData`], it holds loc keys and short place names, never CIG prose.
//! Every list is in the order the text shows it (best first); the generator only cuts it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Obtain hints of one build, by entity class (lower case).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ObtainHints {
    /// Sources by entity class (lower case).
    pub items: BTreeMap<String, ItemObtain>,
    /// Description loc reference (with or without `@`) by entity class (lower case): which
    /// stock key gets the block. A class with sources but no entry here gets nothing; a
    /// class here without sources still gets its craft and reward lines from `GameData`.
    pub desc: BTreeMap<String, String>,
    /// When the commodity prices behind [`ItemObtain::sells`] were fetched (the UEX file's
    /// `_fetched`, RFC 3339); the sell line is stamped with its day.
    pub prices_fetched: Option<String>,
}

impl ObtainHints {
    /// Fill [`Self::desc`] from the build's own description keys ([`super::GameData`]
    /// `item_descs` and `resource_descs`), keeping entries already there.
    ///
    /// A resource owns its description key: entities that reuse it (the mineable rock, the
    /// hand-held mineral, the cargo box of `items_commodities_aphorite_desc`) are left out,
    /// so the resource's mining and quality lines are not contested by an entity's loot.
    pub fn fill_descs(&mut self, gd: &super::GameData) {
        use crate::ini::fold_key;
        let folded = |r: &str| super::loc_key(r).map(fold_key);
        let resource_keys: std::collections::HashSet<String> = gd
            .resource_descs
            .values()
            .filter_map(|r| folded(r))
            .collect();
        for (res, r) in &gd.resource_descs {
            self.desc
                .entry(res.to_ascii_lowercase())
                .or_insert_with(|| r.clone());
        }
        for (class, r) in &gd.item_descs {
            if folded(r).is_some_and(|k| resource_keys.contains(&k)) {
                continue;
            }
            self.desc
                .entry(class.to_ascii_lowercase())
                .or_insert_with(|| r.clone());
        }
    }
}

/// Every source of one class.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ItemObtain {
    /// Shops selling it, cheapest first (from UEX files the user supplied; names only).
    pub shops: Vec<ShopHint>,
    /// Commodity kiosks buying it, best price first (UEX commodity prices, per SCU).
    pub sells: Vec<SellHint>,
    /// How many kiosks buy it in all (`sells` may be cut short); 0 means `sells.len()`.
    pub sells_total: usize,
    /// Loot containers holding it, best first (highest odds).
    pub loot: Vec<LootHint>,
    /// How many containers hold it in all (`loot` may be cut short); 0 means `loot.len()`.
    pub loot_total: usize,
    /// Mining places, richest first.
    pub mined: Vec<MinedHint>,
    /// Ore quality spread, when the item is a mined resource.
    pub quality: Option<QualityHint>,
}

impl ItemObtain {
    /// Kiosks in all: `sells_total`, never fewer than the listed ones.
    pub fn sells_count(&self) -> usize {
        self.sells_total.max(self.sells.len())
    }

    /// Containers in all: `loot_total`, never fewer than the listed ones.
    pub fn loot_count(&self) -> usize {
        self.loot_total.max(self.loot.len())
    }

    pub fn is_empty(&self) -> bool {
        self.shops.is_empty()
            && self.sells.is_empty()
            && self.loot.is_empty()
            && self.loot_total == 0
            && self.mined.is_empty()
            && self.quality.is_none()
    }
}

/// A shop terminal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShopHint {
    /// Terminal or shop name (`Center Mass`).
    pub terminal: String,
    /// Short place (`Area18`), when known.
    pub place: Option<String>,
    /// Star system (`Stanton`), when known.
    pub system: Option<String>,
}

/// A commodity kiosk that buys the resource.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SellHint {
    /// Terminal name (`TDD Area 18`).
    pub terminal: String,
    /// Short place (`Area 18`), when known.
    pub place: Option<String>,
    /// Star system, when known.
    pub system: Option<String>,
    /// What the kiosk pays, aUEC per SCU (player-reported; the text rounds it).
    pub price: f64,
}

/// A loot container kind holding the item.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LootHint {
    /// What the container is (`Weapon crate`, `Ammo box`).
    pub contents: String,
    /// Container size (`Small`, `S2`), when known.
    pub size: Option<String>,
    /// Rarity tier of the entry (`Common`, `Rare`), when known.
    pub rarity: Option<String>,
    /// Where such containers are (`Bunkers`, `Derelicts`), when known.
    pub place: Option<String>,
    /// Star system, when known.
    pub system: Option<String>,
    /// Chance per container, 0 to 1 (an estimate; shown only in Detailed).
    pub odds: Option<f64>,
}

/// A place the resource is mined.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MinedHint {
    /// Body or spot (`Lyria`, `Pyro I`, `Aaron Halo`).
    pub place: String,
    /// Star system, when known.
    pub system: Option<String>,
    /// How it is mined: `hand`, `ship`, `vehicle`, when known.
    pub kind: Option<String>,
    /// Share of the rock's composition, in percent (0 to 100), low and high end.
    pub min_pct: Option<f64>,
    pub max_pct: Option<f64>,
}

/// Ore quality spread (game quality units, 0 to 1000).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QualityHint {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub stddev: Option<f64>,
    /// Places whose range differs from the default spread.
    pub overrides: Vec<QualityOverride>,
}

/// A place with its own quality range.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QualityOverride {
    pub place: String,
    pub min: f64,
    pub max: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults() {
        let h = ObtainHints {
            items: BTreeMap::from([(
                "agri".to_string(),
                ItemObtain {
                    mined: vec![MinedHint {
                        place: "Lyria".into(),
                        kind: Some("hand".into()),
                        ..Default::default()
                    }],
                    quality: Some(QualityHint {
                        min: 1.0,
                        max: 900.0,
                        mean: 500.0,
                        ..Default::default()
                    }),
                    sells: vec![SellHint {
                        terminal: "TDD Area 18".into(),
                        price: 6240.0,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            )]),
            desc: BTreeMap::from([("agri".to_string(), "@items_commodities_agri_desc".into())]),
            prices_fetched: Some("2026-11-21T09:00:00.000Z".into()),
        };
        let s = serde_json::to_string(&h).unwrap();
        let back: ObtainHints = serde_json::from_str(&s).unwrap();
        assert_eq!(back, h);
        let empty: ObtainHints = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, ObtainHints::default());
        let mut i = ItemObtain::default();
        assert!(i.is_empty());
        i.loot_total = 4;
        assert!(!i.is_empty());
        assert_eq!(i.loot_count(), 4);
    }

    #[test]
    fn fill_descs_gives_resources_their_own_keys() {
        let gd = crate::model::GameData {
            item_descs: BTreeMap::from([
                (
                    "behr_rifle_ballistic_01".into(),
                    "@item_DescBEHR_Rifle".into(),
                ),
                (
                    "mineablerock_fps_aphorite".into(),
                    "@items_commodities_aphorite_desc".into(),
                ),
                ("kept".into(), "@kept_desc".into()),
            ]),
            resource_descs: BTreeMap::from([(
                "aphorite".into(),
                "@Items_Commodities_Aphorite_Desc".into(),
            )]),
            ..Default::default()
        };
        let mut h = ObtainHints {
            desc: BTreeMap::from([("kept".to_string(), "@already".to_string())]),
            ..Default::default()
        };
        h.fill_descs(&gd);
        assert_eq!(h.desc["behr_rifle_ballistic_01"], "@item_DescBEHR_Rifle");
        assert_eq!(h.desc["aphorite"], "@Items_Commodities_Aphorite_Desc");
        assert_eq!(h.desc["kept"], "@already");
        assert!(!h.desc.contains_key("mineablerock_fps_aphorite"));
    }
}
