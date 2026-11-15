//! Blueprint display names to match imported names against (addendum §4).
//!
//! The real catalog is the English display name of each blueprint's output item in
//! `GameData` (spec 6.1: `CraftingBlueprintRecord` → produced entity → its name). M2 fills
//! the `GameData` side; until then [`GameData`] yields an empty catalog and tests use
//! [`ListCatalog`].

use super::normalize;
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

impl BlueprintCatalog for GameData {
    /// Empty until GameData carries blueprint records and their resolved names (M2).
    fn blueprints(&self) -> Vec<CatalogEntry> {
        Vec::new()
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
        assert!(index(&GameData::default()).is_empty());
    }
}
