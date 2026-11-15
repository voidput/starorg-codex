//! Blueprint display names to match imported names against (addendum §4).
//!
//! The real catalog is [`GameDataCatalog`]: the English display name of each blueprint's
//! output item (spec 6.1: `CraftingBlueprintRecord` → produced entity → its name key →
//! the stock `global.ini` text). Tests and the hidden `--test-catalog` option use
//! [`ListCatalog`].

use super::normalize;
use crate::enhance::stock_name;
use crate::ini::StockIni;
use crate::model::GameData;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Blueprint record id (GUID or record name).
    pub id: String,
    /// English display name of the produced item.
    pub name: String,
}

pub trait BlueprintCatalog {
    fn blueprints(&self) -> Vec<CatalogEntry>;
}

/// Blueprints of one build: `GameData` records, named through the stock text of the same
/// build. A blueprint whose produced item has no name key, or whose name is a placeholder,
/// is left out (it cannot be matched by name).
pub struct GameDataCatalog<'a> {
    pub game_data: &'a GameData,
    pub stock: &'a StockIni,
}

impl<'a> GameDataCatalog<'a> {
    pub fn new(game_data: &'a GameData, stock: &'a StockIni) -> Self {
        Self { game_data, stock }
    }
}

impl BlueprintCatalog for GameDataCatalog<'_> {
    fn blueprints(&self) -> Vec<CatalogEntry> {
        self.game_data
            .blueprints
            .iter()
            .filter_map(|bp| {
                let name = stock_name(self.stock, bp.produces_name_key.as_deref())?;
                let id = if bp.id.is_empty() {
                    bp.short_name().to_string()
                } else {
                    bp.id.clone()
                };
                Some(CatalogEntry {
                    id,
                    name: name.to_string(),
                })
            })
            .collect()
    }
}

/// A fixed list (tests, and the hidden `--test-catalog` CLI option).
#[derive(Debug, Clone, Default)]
pub struct ListCatalog(pub Vec<CatalogEntry>);

impl ListCatalog {
    /// One blueprint per line: `id<TAB>name`, or just `name` (the name is then the id).
    /// Blank lines and lines starting with `#` are skipped.
    pub fn parse(text: &str) -> Self {
        let entries = text
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| match l.split_once('\t') {
                Some((id, name)) => CatalogEntry {
                    id: id.trim().to_string(),
                    name: name.trim().to_string(),
                },
                None => CatalogEntry {
                    id: l.trim().to_string(),
                    name: l.trim().to_string(),
                },
            })
            .collect();
        Self(entries)
    }
}

impl BlueprintCatalog for ListCatalog {
    fn blueprints(&self) -> Vec<CatalogEntry> {
        self.0.clone()
    }
}

/// Normalised name → candidate ids (sorted, unique).
pub fn index(catalog: &dyn BlueprintCatalog) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in catalog.blueprints() {
        let k = normalize(&e.name);
        if k.is_empty() {
            continue;
        }
        let ids = m.entry(k).or_default();
        if !ids.contains(&e.id) {
            ids.push(e.id);
        }
    }
    for ids in m.values_mut() {
        ids.sort();
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_index() {
        let c =
            ListCatalog::parse("# test\nbp_a\tAlpha Helmet\n\nbp_b\talpha  helmet\nBeta Core\n");
        assert_eq!(c.0.len(), 3);
        let i = index(&c);
        assert_eq!(i["alpha helmet"], vec!["bp_a", "bp_b"]);
        assert_eq!(i["beta core"], vec!["Beta Core"]);
        let stock = StockIni::parse(b"\xEF\xBB\xBF".to_vec());
        assert!(index(&GameDataCatalog::new(&GameData::default(), &stock)).is_empty());
    }
}
