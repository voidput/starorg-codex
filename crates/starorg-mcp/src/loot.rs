//! The app's Loot screen: every loot area of a build with what can drop there, and every
//! item that drops in an area with where, each likeliest first.
//!
//! One [`LootScreen`] per build, from the same corpus the Sources card and the obtain hints
//! read ([`crate::tools::Tools::loot_list`] keeps it in the store's per-build cache), worked
//! out with [`BulkObtain::areas_all`]: the best spawn of each (item, area) pair, inverted per
//! area. Odds are the rough per-box estimates of [`starorg_gamedata::query::obtain`]: the
//! chance one spawned container (or rack, or loose spot) holds the item, which assumes an
//! even pick inside each tag pool. Loot tables no area places (corpses, events) are not
//! listed.
//!
//! The list answers ([`LootList`]) carry summaries only, each with its place in the screen's
//! trees ([`crate::loot_tree`]: kind of place or category, a middle level, the row);
//! [`LootDetail`] is one area's or one item's drops, optionally filtered.

use crate::hints::loot_hint;
use crate::shops::{ItemRecord, by_game_type};
use serde::{Deserialize, Serialize};
use starorg_gamedata::query::obtain::{AreaSummary, BulkObtain, Datasets, LootIndex, SpawnKind};
use starorg_gamedata::query::types::ItemTags;
use std::collections::{HashMap, HashSet};

/// What a caller without `starorg-gamedata` needs to build a [`LootScreen`] (the app's mock).
pub mod input {
    pub use starorg_gamedata::query::obtain::{Datasets, LootData, build_loot_index};
    pub use starorg_gamedata::query::types::{
        ItemTags, LootArea, SpawnConfig, SpawnLoot, SpawnSlot,
    };
}

/// Drops kept per detail answer (the rest are counted in [`LootDetail::total`]; a filter
/// reaches them).
pub const MAX_DROPS: usize = 2000;

/// Separator of [`LootDrop::group`]'s tree path.
pub const PATH_SEP: &str = " › ";

/// `ready`, or `missing` while the build's loot data is not cached (Read game data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LootDataState {
    Ready,
    Missing,
}

/// Which list: loot areas, or items that drop in one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LootView {
    Areas,
    Items,
}

/// How the item is there: rolled by a loot table, held by a rack's loadout, or lying loose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LootDropKind {
    Loot,
    Fixed,
    Loose,
}

impl From<SpawnKind> for LootDropKind {
    fn from(k: SpawnKind) -> Self {
        match k {
            SpawnKind::Loot => Self::Loot,
            SpawnKind::Fixed => Self::Fixed,
            SpawnKind::Loose => Self::Loose,
        }
    }
}

/// One row of a list: an area or an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootSummary {
    /// The area's slot preset path, or the item's class name (what `loot_detail` takes).
    pub id: String,
    /// Readable name, unique among the rows of the same `path`.
    pub name: String,
    /// Kind of place (`Caves`) or item category (`FPS weapons`).
    pub group: String,
    /// The row's place in the list's tree, top first, without the row itself
    /// (`["Caves", "Occupied"]`, `["FPS weapons", "Rifles"]`); empty when the row is a
    /// kind of place or category of its own ([`crate::loot_tree`]).
    pub path: Vec<String>,
    /// Areas: items that can drop there. Items: areas it can drop in.
    pub count: usize,
    /// The best rough odds per spawned box, 0 to 1.
    pub best: f64,
}

/// `loot_list`: every area or every item, summaries only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootList {
    pub state: LootDataState,
    pub view: LootView,
    /// The build the data is of (`LIVE 4.10.201.48085`).
    pub build: Option<String>,
    /// Areas by group (`Other` last), path then name; items by name.
    pub rows: Vec<LootSummary>,
}

/// One drop of a detail: an item of an area, or an area of an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootDrop {
    /// The item's class name, or the area's id.
    pub id: String,
    pub name: String,
    /// Where the row sits in its list's tree (`Caves › Occupied`, `FPS weapons › Rifles`).
    pub group: String,
    /// Rough odds one spawned box holds the item, 0 to 1.
    pub odds: f64,
    /// What the spot is (`Large weapons crate`, `Weapons rack`, `Lying loose`).
    pub container: String,
    pub rarity: Option<String>,
    pub kind: LootDropKind,
}

/// `loot_detail`: one area's items or one item's areas, likeliest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootDetail {
    pub state: LootDataState,
    pub view: LootView,
    /// `None` when nothing has that id (or the data is missing).
    pub entry: Option<LootSummary>,
    /// Items: the game record (what the Explorer opens).
    pub record: Option<ItemRecord>,
    /// Likeliest first, those matching the filter, at most [`MAX_DROPS`].
    pub rows: Vec<LootDrop>,
    /// Drops matching the filter (all of them without one).
    pub total: usize,
    /// Every drop of the entry, filter or not.
    pub all: usize,
}

/// One (item, area) pair, by index.
#[derive(Debug, Clone, Copy)]
struct Pair {
    item: u32,
    area: u32,
    odds: f64,
    container: u32,
    rarity: Option<u32>,
    kind: LootDropKind,
}

/// The Loot screen of one build: both lists and every pair, built once.
#[derive(Debug, Default)]
pub struct LootScreen {
    pub build: Option<String>,
    areas: Vec<LootSummary>,
    items: Vec<LootSummary>,
    records: Vec<ItemRecord>,
    /// Interned container and rarity labels.
    labels: Vec<String>,
    pairs: Vec<Pair>,
    /// Pair indexes per area / item, likeliest first.
    by_area: Vec<Vec<u32>>,
    by_item: Vec<Vec<u32>>,
    area_ix: HashMap<String, usize>,
    item_ix: HashMap<String, usize>,
}

/// Test presets: not places a player reaches.
pub fn hidden_area(area: &str) -> bool {
    area.to_ascii_lowercase()
        .split(['/', '_'])
        .any(|w| w.starts_with("test"))
}

/// Manufacturer codes as class names start (`behr_lmg_01` -> `BEHR`): the first word of
/// every item with a manufacturer, so readable names write them in capitals.
pub fn maker_codes(items: &[&ItemTags]) -> HashSet<String> {
    items
        .iter()
        .filter(|i| {
            i.manufacturer.is_some() || i.tags.iter().any(|t| t.starts_with("Manufacturer/"))
        })
        .filter_map(|i| {
            let w = i.class_name.split('_').next()?;
            ((3..=5).contains(&w.len()) && w.bytes().all(|b| b.is_ascii_alphabetic()))
                .then(|| w.to_ascii_uppercase())
        })
        .collect()
}

/// The item's name through the app's name chain as far as this data goes: the game's
/// (item-tags' localized name) while it is real, else readable words from the class name
/// (`behr_lmg_ballistic_01` -> `BEHR LMG Ballistic 01`, `makers` from [`maker_codes`]).
pub fn item_name(item: &ItemTags, makers: &HashSet<String>) -> String {
    let n = item.name.trim();
    if !n.is_empty()
        && !n.eq_ignore_ascii_case(&item.class_name)
        && !starorg_core::model::is_placeholder_name(Some(n))
    {
        return n.to_string();
    }
    let r = starorg_core::enhance::names::readable(&item.class_name, makers);
    if r.trim().is_empty() {
        item.class_name.clone()
    } else {
        r
    }
}

/// The item's category (`FPS weapons`), from its game type.
pub fn item_group(item: &ItemTags) -> String {
    item.ty
        .as_deref()
        .and_then(by_game_type)
        .map_or("Other", |c| c.label())
        .to_string()
}

/// The area's kind of place (`Caves`), else `Other` ([`crate::loot_tree`]).
pub fn area_group(area: &str) -> String {
    crate::loot_tree::area_place(area)
}

fn intern(labels: &mut Vec<String>, ix: &mut HashMap<String, u32>, s: String) -> u32 {
    if let Some(&i) = ix.get(&s) {
        return i;
    }
    let i = labels.len() as u32;
    labels.push(s.clone());
    ix.insert(s, i);
    i
}

fn by_odds(pairs: &[Pair], list: &mut [u32]) {
    list.sort_by(|&a, &b| pairs[b as usize].odds.total_cmp(&pairs[a as usize].odds));
}

fn cmp_name(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

impl LootScreen {
    /// Every area spawn of every non-template item ([`BulkObtain::areas_all`], parallel),
    /// inverted per area. `build`: the label the lists carry.
    pub fn build(d: &Datasets, index: Option<&LootIndex>, build: Option<String>) -> Self {
        let bulk = BulkObtain::new(d, index);
        let list: Vec<&ItemTags> = d.items.iter().filter(|i| !i.template()).collect();
        let all = bulk.areas_all(&list);
        Self::from_areas(&list, &all, build)
    }

    /// [`Self::build`] from the ranked areas of each item.
    pub fn from_areas(
        items: &[&ItemTags],
        all: &[Vec<AreaSummary>],
        build: Option<String>,
    ) -> Self {
        let mut s = Self {
            build,
            ..Default::default()
        };
        let mut label_ix: HashMap<String, u32> = HashMap::new();
        // Areas in first-seen order; sorted (and renumbered) below.
        let mut area_names: Vec<String> = Vec::new();
        let mut area_seen: HashMap<String, u32> = HashMap::new();
        let makers = maker_codes(items);
        // (item, its name, its pairs)
        let mut kept_items: Vec<(&ItemTags, String, Vec<Pair>)> = Vec::new();
        for (item, areas) in items.iter().zip(all) {
            let mut pairs = Vec::new();
            for a in areas {
                if a.best.score <= 0.0 || hidden_area(&a.area) {
                    continue;
                }
                let ai = *area_seen.entry(a.area.clone()).or_insert_with(|| {
                    area_names.push(a.area.clone());
                    (area_names.len() - 1) as u32
                });
                let h = loot_hint(a);
                let container = match &h.size {
                    Some(z) => format!("{} {z}", h.contents),
                    None => h.contents.clone(),
                };
                pairs.push(Pair {
                    item: 0,
                    area: ai,
                    odds: a.best.score,
                    container: intern(&mut s.labels, &mut label_ix, container),
                    rarity: h.rarity.map(|r| intern(&mut s.labels, &mut label_ix, r)),
                    kind: a.best.kind.into(),
                });
            }
            if !pairs.is_empty() {
                kept_items.push((item, item_name(item, &makers), pairs));
            }
        }
        // Items by name; areas by group (`Other` last), path, then name.
        kept_items.sort_by(|(a, an, _), (b, bn, _)| {
            cmp_name(an, bn).then_with(|| a.class_name.cmp(&b.class_name))
        });
        let ids: Vec<&str> = area_names.iter().map(String::as_str).collect();
        let tree = crate::loot_tree::area_tree(&ids);
        let mut area_order: Vec<(String, Vec<String>, String, u32)> = tree
            .into_iter()
            .enumerate()
            .map(|(i, (path, name))| (area_group(&area_names[i]), path, name, i as u32))
            .collect();
        area_order.sort_by(|a, b| {
            let other = |g: &str| g == crate::loot_tree::OTHER;
            other(&a.0)
                .cmp(&other(&b.0))
                .then_with(|| cmp_name(&a.0, &b.0))
                .then_with(|| cmp_path(&a.1, &b.1))
                .then_with(|| cmp_name(&a.2, &b.2))
                .then_with(|| area_names[a.3 as usize].cmp(&area_names[b.3 as usize]))
        });
        let mut renumber = vec![0u32; area_names.len()];
        for (new, (group, path, name, old)) in area_order.into_iter().enumerate() {
            renumber[old as usize] = new as u32;
            let id = area_names[old as usize].clone();
            s.area_ix.insert(id.to_ascii_lowercase(), new);
            s.areas.push(LootSummary {
                id,
                name,
                group,
                path,
                count: 0,
                best: 0.0,
            });
        }
        let kinds: Vec<(String, Option<String>)> = kept_items
            .iter()
            .map(|(item, _, _)| (item_group(item), crate::loot_tree::item_kind(item)))
            .collect();
        let item_paths = crate::loot_tree::item_paths(&kinds);
        s.by_area = vec![Vec::new(); s.areas.len()];
        for (ii, ((item, name, pairs), ((group, _), path))) in kept_items
            .into_iter()
            .zip(kinds.into_iter().zip(item_paths))
            .enumerate()
        {
            s.item_ix.insert(item.class_name.to_ascii_lowercase(), ii);
            let mut mine = Vec::with_capacity(pairs.len());
            let mut best = 0.0f64;
            for mut p in pairs {
                p.item = ii as u32;
                p.area = renumber[p.area as usize];
                best = best.max(p.odds);
                let pi = s.pairs.len() as u32;
                s.by_area[p.area as usize].push(pi);
                mine.push(pi);
                s.pairs.push(p);
            }
            by_odds(&s.pairs, &mut mine);
            s.items.push(LootSummary {
                id: item.class_name.clone(),
                name,
                group,
                path,
                count: mine.len(),
                best,
            });
            s.records.push(ItemRecord {
                guid: item.id.clone(),
                class_name: item.class_name.clone(),
            });
            s.by_item.push(mine);
        }
        for (ai, list) in s.by_area.iter_mut().enumerate() {
            by_odds(&s.pairs, list);
            let a = &mut s.areas[ai];
            a.count = list.len();
            a.best = list.first().map_or(0.0, |&p| s.pairs[p as usize].odds);
        }
        s
    }

    pub fn list(&self, view: LootView) -> LootList {
        LootList {
            state: LootDataState::Ready,
            view,
            build: self.build.clone(),
            rows: match view {
                LootView::Areas => self.areas.clone(),
                LootView::Items => self.items.clone(),
            },
        }
    }

    /// One area's items or one item's areas, likeliest first. `filter`: only the drops
    /// holding every word of it (in the name, tree path, container, rarity or id), so the
    /// tail past [`MAX_DROPS`] is reachable; still at most [`MAX_DROPS`] rows.
    pub fn detail(&self, view: LootView, id: &str, filter: Option<&str>) -> LootDetail {
        let key = id.trim().to_ascii_lowercase();
        let found = match view {
            LootView::Areas => self.area_ix.get(&key),
            LootView::Items => self.item_ix.get(&key),
        };
        let Some(&i) = found else {
            return LootDetail {
                state: LootDataState::Ready,
                view,
                entry: None,
                record: None,
                rows: Vec::new(),
                total: 0,
                all: 0,
            };
        };
        let (entry, record, list) = match view {
            LootView::Areas => (self.areas[i].clone(), None, &self.by_area[i]),
            LootView::Items => (
                self.items[i].clone(),
                Some(self.records[i].clone()),
                &self.by_item[i],
            ),
        };
        let words: Vec<String> = filter
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let drop_of = |pi: u32| {
            let p = &self.pairs[pi as usize];
            let other = match view {
                LootView::Areas => &self.items[p.item as usize],
                LootView::Items => &self.areas[p.area as usize],
            };
            LootDrop {
                id: other.id.clone(),
                name: other.name.clone(),
                group: where_label(other),
                odds: p.odds,
                container: self.labels[p.container as usize].clone(),
                rarity: p.rarity.map(|r| self.labels[r as usize].clone()),
                kind: p.kind,
            }
        };
        let mut total = 0;
        let mut rows = Vec::new();
        for &pi in list {
            if words.is_empty() {
                if rows.len() < MAX_DROPS {
                    rows.push(drop_of(pi));
                }
                total += 1;
                continue;
            }
            let d = drop_of(pi);
            if drop_matches(&d, &words) {
                total += 1;
                if rows.len() < MAX_DROPS {
                    rows.push(d);
                }
            }
        }
        LootDetail {
            state: LootDataState::Ready,
            view,
            entry: Some(entry),
            record,
            rows,
            total,
            all: list.len(),
        }
    }
}

/// Where a row sits: its tree path (`Caves › Occupied`), or its group when it is a kind of
/// place or category of its own.
fn where_label(r: &LootSummary) -> String {
    if r.path.is_empty() {
        r.group.clone()
    } else {
        r.path.join(PATH_SEP)
    }
}

/// Whether a drop holds every (lower-case) word.
fn drop_matches(d: &LootDrop, words: &[String]) -> bool {
    let hay = format!(
        "{} {} {} {} {}",
        d.name,
        d.group,
        d.container,
        d.rarity.as_deref().unwrap_or(""),
        d.id
    )
    .to_lowercase();
    words.iter().all(|w| hay.contains(w.as_str()))
}

fn cmp_path(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b) {
        let o = cmp_name(x, y);
        if o.is_ne() {
            return o;
        }
    }
    a.len().cmp(&b.len())
}

/// The lists while the loot data is not there.
pub fn missing_list(view: LootView) -> LootList {
    LootList {
        state: LootDataState::Missing,
        view,
        build: None,
        rows: Vec::new(),
    }
}

pub fn missing_detail(view: LootView) -> LootDetail {
    LootDetail {
        state: LootDataState::Missing,
        view,
        entry: None,
        record: None,
        rows: Vec::new(),
        total: 0,
        all: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use starorg_gamedata::query::obtain::{LootData, SpawnHit, build_loot_index};
    use starorg_gamedata::query::types::{LootArea, SpawnConfig, SpawnLoot, SpawnSlot};

    const CAN: &str = "LootGeneration/CanGenerateAsLoot";
    const COMMON: &str = "LootGeneration/LootRarity/Common";

    fn item(class: &str, name: &str, ty: &str, tags: &[&str]) -> ItemTags {
        ItemTags {
            name: name.into(),
            class_name: class.into(),
            id: format!("guid-{class}"),
            ty: Some(ty.into()),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        }
    }

    fn datasets() -> Datasets {
        let loot: LootData = serde_json::from_value(json!({
            "tables": [{"name": "Crate_Weapons", "lootArchetypes": [{"archetype": "Arch_Guns", "weight": 1}]}],
            "archetypes": [{"name": "Arch_Guns", "primaryOrGroup": {"entries": [
                {"name": "LMG", "tag": "Weapon/FPS/Stocked/LMG", "weight": 3},
                {"name": "Pistol", "tag": "Weapon/FPS/Pistol", "weight": 1}]}}]
        }))
        .unwrap();
        let slot = |entity: &str, weight: f64, loot: Option<&str>, contains: &[&str]| SpawnSlot {
            entity: entity.into(),
            weight,
            loot: loot.map(|t| SpawnLoot {
                table: t.into(),
                ..Default::default()
            }),
            contains: (!contains.is_empty())
                .then(|| contains.iter().map(|c| c.to_string()).collect()),
            ..Default::default()
        };
        let config = |tags: &[&str], slots: Vec<SpawnSlot>| SpawnConfig {
            spawn_tags: tags.iter().map(|t| t.to_string()).collect(),
            slots,
            ..Default::default()
        };
        Datasets {
            items: vec![
                item(
                    "behr_lmg_01",
                    "FS-9 LMG",
                    "WeaponPersonal",
                    &[CAN, COMMON, "Weapon/FPS/Stocked/LMG"],
                ),
                item(
                    "ksar_pistol_01",
                    "Arclight Pistol",
                    "WeaponPersonal",
                    &[CAN, COMMON, "Weapon/FPS/Pistol"],
                ),
                // Placeholder name: readable words from the class name.
                item(
                    "rsi_helmet_01",
                    "rsi_helmet_01",
                    "Char_Armor_Helmet",
                    &["Manufacturer/RSI"],
                ),
                // Not lootable and in no rack: in no list.
                item(
                    "store_only",
                    "Shop Gun",
                    "WeaponPersonal",
                    &["Weapon/FPS/Pistol"],
                ),
            ],
            loot: Some(loot),
            areas: Some(vec![
                LootArea {
                    name: "caves/loot_caves_occupied".into(),
                    configs: vec![config(
                        &[
                            "LootGeneration/LootContents/Weapons",
                            "LootGeneration/LootSize/Large",
                        ],
                        vec![slot("crate_weapons", 1.0, Some("Crate_Weapons"), &[])],
                    )],
                },
                LootArea {
                    name: "lootgeneration_slotpreset_derelictoutpost".into(),
                    configs: vec![config(
                        &[],
                        vec![
                            slot("weapon_rack_1", 1.0, None, &["rsi_helmet_01"]),
                            slot("crate_weapons", 3.0, Some("Crate_Weapons"), &[]),
                        ],
                    )],
                },
                LootArea {
                    name: "test_slotpreset_caves".into(),
                    configs: vec![config(
                        &[],
                        vec![slot("crate_weapons", 1.0, Some("Crate_Weapons"), &[])],
                    )],
                },
            ]),
            ..Default::default()
        }
    }

    fn screen() -> LootScreen {
        let d = datasets();
        let index = build_loot_index(d.loot.as_ref().unwrap());
        LootScreen::build(&d, Some(&index), Some("LIVE 4.10".into()))
    }

    #[test]
    fn names_are_readable() {
        assert_eq!(
            area_group("lootgeneration_slotpreset_derelictoutpost"),
            "Derelict outposts"
        );
        assert_eq!(area_group("v3slotpreset_loot_generic"), "Other");
        assert_eq!(
            area_group("contestedzone_vault_executive"),
            "Contested zones"
        );
        assert_eq!(area_group("v3slotpreset_rockcracker"), "Rock Cracker, Nyx");
        assert!(hidden_area("test_slotpreset_caves"));
        assert!(!hidden_area("caves/loot_caves_occupied"));
        let helmet = item(
            "rsi_helmet_01",
            "<= PLACEHOLDER =>",
            "Char_Armor_Helmet",
            &[],
        );
        let makers: HashSet<String> = ["RSI".to_string()].into();
        assert_eq!(item_name(&helmet, &makers), "RSI Helmet 01");
        assert_eq!(item_name(&helmet, &HashSet::new()), "Rsi Helmet 01");
        assert_eq!(item_group(&helmet), "Armour & clothing");
        let gun = item("behr_lmg_01", "FS-9 LMG", "WeaponPersonal", &[]);
        assert_eq!(item_name(&gun, &makers), "FS-9 LMG");
        let tagged = item(
            "behr_lmg_01",
            "",
            "WeaponPersonal",
            &["Manufacturer/Behring"],
        );
        assert_eq!(
            maker_codes(&[&tagged, &helmet]),
            ["BEHR".to_string()].into()
        );
    }

    #[test]
    fn lists_hold_summaries_sorted() {
        let s = screen();
        let areas = s.list(LootView::Areas);
        assert_eq!(areas.state, LootDataState::Ready);
        assert_eq!(areas.build.as_deref(), Some("LIVE 4.10"));
        let ids: Vec<&str> = areas.rows.iter().map(|r| r.id.as_str()).collect();
        // By group (Caves before Derelict outposts); the test preset is left out.
        assert_eq!(
            ids,
            [
                "caves/loot_caves_occupied",
                "lootgeneration_slotpreset_derelictoutpost"
            ]
        );
        assert_eq!(areas.rows[0].group, "Caves");
        // Each alone in its place: a top-level row named by the place (and its words).
        assert!(areas.rows[0].path.is_empty());
        assert_eq!(areas.rows[0].name, "Caves: Occupied");
        assert_eq!(areas.rows[1].name, "Derelict outposts");
        assert_eq!(areas.rows[0].count, 2);
        let items = s.list(LootView::Items);
        let names: Vec<&str> = items.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Arclight Pistol", "FS-9 LMG", "RSI Helmet 01"]);
        // Two FPS weapons of different kinds: no kind level; the helmet is its category.
        assert_eq!(items.rows[0].path, ["FPS weapons"]);
        assert!(items.rows[2].path.is_empty());
        assert_eq!(items.rows[1].count, 2);
        // LMG weight 3 of 4 in a crate that fills every spawn.
        assert!((items.rows[1].best - 0.75).abs() < 1e-9);
    }

    #[test]
    fn details_are_likeliest_first() {
        let s = screen();
        let cave = s.detail(LootView::Areas, "CAVES/loot_caves_occupied", None);
        assert_eq!(cave.entry.as_ref().unwrap().name, "Caves: Occupied");
        let names: Vec<&str> = cave.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["FS-9 LMG", "Arclight Pistol"]);
        assert_eq!(cave.rows[0].container, "Large weapons crate");
        assert_eq!(cave.rows[0].kind, LootDropKind::Loot);
        assert_eq!((cave.total, cave.all), (2, 2));
        assert_eq!(cave.rows[0].group, "FPS weapons");

        let lmg = s.detail(LootView::Items, "behr_lmg_01", None);
        assert_eq!(lmg.record.as_ref().unwrap().guid, "guid-behr_lmg_01");
        // Caves: 0.75 per box; the derelict outpost's crate is 3 of 4 spawns: 0.5625.
        let odds: Vec<f64> = lmg.rows.iter().map(|r| r.odds).collect();
        assert!(odds.windows(2).all(|w| w[0] >= w[1]));
        assert_eq!(lmg.rows[0].id, "caves/loot_caves_occupied");
        assert!((lmg.rows[1].odds - 0.5625).abs() < 1e-9);

        let helmet = s.detail(LootView::Items, "rsi_helmet_01", None);
        assert_eq!(helmet.rows[0].kind, LootDropKind::Fixed);
        assert!((helmet.rows[0].odds - 0.25).abs() < 1e-9);

        let none = s.detail(LootView::Items, "store_only", None);
        assert!(none.entry.is_none() && none.rows.is_empty());
    }

    #[test]
    fn detail_filter_reaches_every_drop() {
        let s = screen();
        // Every word must match (name, path, container, rarity or id), any case.
        let d = s.detail(
            LootView::Areas,
            "caves/loot_caves_occupied",
            Some("  ARC  "),
        );
        let names: Vec<&str> = d.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Arclight Pistol"]);
        assert_eq!((d.total, d.all), (1, 2));
        let d = s.detail(
            LootView::Areas,
            "caves/loot_caves_occupied",
            Some("weapons ksar"),
        );
        assert_eq!(d.rows[0].id, "ksar_pistol_01");
        let d = s.detail(
            LootView::Areas,
            "caves/loot_caves_occupied",
            Some("nothing"),
        );
        assert!(d.entry.is_some() && d.rows.is_empty());
        assert_eq!((d.total, d.all), (0, 2));
        let d = s.detail(LootView::Items, "behr_lmg_01", Some("derelict"));
        assert_eq!(d.rows.len(), 1);
        assert_eq!(d.rows[0].id, "lootgeneration_slotpreset_derelictoutpost");
        // A blank filter is none.
        assert_eq!(s.detail(LootView::Items, "behr_lmg_01", Some(" ")).total, 2);
    }

    /// More drops than [`MAX_DROPS`]: the list stops there, a filter reaches the tail.
    #[test]
    fn detail_is_capped_and_the_filter_reaches_the_tail() {
        let n = MAX_DROPS + 5;
        let items: Vec<ItemTags> = (0..n)
            .map(|i| {
                item(
                    &format!("gun_{i:05}"),
                    &format!("Gun {i:05}"),
                    "WeaponPersonal",
                    &[],
                )
            })
            .collect();
        let refs: Vec<&ItemTags> = items.iter().collect();
        let all: Vec<Vec<AreaSummary>> = (0..n)
            .map(|i| {
                let area = "caves/loot_caves_occupied".to_string();
                vec![AreaSummary {
                    area: area.clone(),
                    best: SpawnHit {
                        area,
                        config: None,
                        spawn_tags: Vec::new(),
                        entity: "crate_weapons".into(),
                        entity_name: None,
                        slot_share: 1.0,
                        kind: SpawnKind::Loot,
                        table: None,
                        per_roll: None,
                        max_results: None,
                        score: 1.0 - i as f64 / (n as f64 * 2.0),
                        paths: None,
                    },
                    configs: 1,
                }]
            })
            .collect();
        let s = LootScreen::from_areas(&refs, &all, None);
        let d = s.detail(LootView::Areas, "caves/loot_caves_occupied", None);
        assert_eq!((d.rows.len(), d.total, d.all), (MAX_DROPS, n, n));
        let last = format!("gun_{:05}", n - 1);
        assert!(d.rows.iter().all(|r| r.id != last));
        let d = s.detail(LootView::Areas, "caves/loot_caves_occupied", Some(&last));
        assert_eq!(d.rows.len(), 1);
        assert_eq!(d.rows[0].id, last);
    }

    #[test]
    fn missing_answers_are_empty() {
        assert_eq!(missing_list(LootView::Items).state, LootDataState::Missing);
        assert!(missing_detail(LootView::Areas).rows.is_empty());
    }
}
