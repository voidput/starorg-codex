//! The app's Loot screen: every loot area of a build with what can drop there, and every
//! item that drops in an area with where, each likeliest first.
//!
//! One [`LootScreen`] per build, from the same corpus the Sources card and the obtain hints
//! read ([`crate::tools::Tools::loot_list`] keeps it in the store's per-build cache), worked
//! out with [`BulkObtain::areas_all`]: the best spawn of each (item, area) pair, inverted per
//! area. Odds are the rough per-box estimates of [`starorg_gamedata::query::obtain`]: the
//! chance one spawned container (or rack, or loose spot) holds the item, which assumes an
//! even pick inside each tag pool. Loot tables no area places (corpses, events) are not
//! listed, except NPC bodies: each actor loot table ([`crate::loot_tree::is_actor_table`])
//! is a row of the areas list under [`NPC_KIND`] (id [`NPC_PREFIX`] and the table), its
//! drops by the table's slot (`Ammo`, `Scrip`) with the odds of [`BulkObtain::bodies_all`].
//! That is what the NPC carries in its pockets. What it wears and wields is a second kind,
//! [`GEAR_KIND`] (id [`GEAR_PREFIX`] and an actor class), from `game-npc-loadouts`: one row
//! per NPC type (actors with the same name and gear collapse into one), by faction and role
//! ([`crate::loot_tree::gear_tree`]), its items grouped by slot with the chance it spawns
//! wearing each (even picks assumed: the data gives no weights). Body-loot and worn-gear
//! rows of the same NPC link to each other ([`NpcInfo`]). Which places spawn which NPC is
//! not in the data, so no NPC row names one.
//!
//! The list answers ([`LootList`]) carry summaries only, each with its place in the screen's
//! trees ([`crate::loot_tree`]: kind of place or category, a middle level, the row);
//! [`LootDetail`] is one area's or one item's drops, optionally filtered, in groups
//! ([`LootGroupBy`]: by container, by place, or one flat group). Every group's header (its
//! size and best odds) covers all the entry's drops; rows come only for the groups asked for
//! (or a few opened by default), each capped, so a big area answers small and a group's rows
//! load when it is opened.

use crate::hints::loot_hint;
pub use crate::loot_tree::{GEAR_KIND, GEAR_PREFIX, NPC_KIND, NPC_PREFIX, is_gear, is_npc};
use crate::shops::{ItemRecord, by_game_type};
use serde::{Deserialize, Serialize};
use starorg_gamedata::domains::npc_loadouts::{GearItem, NpcLoadouts, slot_rank};
pub use starorg_gamedata::domains::npc_loadouts::{SLOTS as GEAR_SLOTS, Wear};
use starorg_gamedata::query::obtain::{
    AreaSummary, BodyHit, BulkObtain, Datasets, LootIndex, SpawnKind, loot_eligibility,
};
use starorg_gamedata::query::types::ItemTags;
use std::collections::{BTreeSet, HashMap, HashSet};

/// What a caller without `starorg-gamedata` needs to build a [`LootScreen`] (the app's mock).
pub mod input {
    pub use starorg_gamedata::domains::npc_loadouts::{GearItem, NpcActor, NpcLoadouts, Wear};
    pub use starorg_gamedata::query::obtain::{Datasets, LootData, build_loot_index};
    pub use starorg_gamedata::query::types::{
        ItemTags, LootArea, SpawnConfig, SpawnLoot, SpawnSlot,
    };
}

/// The dataset the worn-gear rows come from.
pub const GEAR_DATASET: &str = starorg_gamedata::domains::npc_loadouts::NAME;

/// Drops kept in an ungrouped detail ([`LootGroupBy::None`]; the rest are counted in
/// [`LootGroup::count`]; a filter reaches them).
pub const MAX_DROPS: usize = 2000;

/// Rows kept per group of a grouped detail (the rest are counted in [`LootGroup::count`]).
pub const MAX_GROUP_ROWS: usize = 500;

/// Without a filter, the groups opened by default: the first, and the next while all the
/// opened ones hold at most this many drops together (a few small groups).
pub const OPEN_BUDGET: usize = 40;

/// Key of the one group of an ungrouped detail.
pub const ALL_GROUP: &str = "all";

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

/// How the item is there: rolled by a loot table, held by a rack's loadout, lying loose, or
/// worn or wielded by an NPC (its loadout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LootDropKind {
    Loot,
    Fixed,
    Loose,
    Worn,
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

/// How a detail's drops are grouped: by the spot they are in (`Weapons rack`, `Lying
/// loose`), by the top level of the other list's tree (an area's kind of place, an item's
/// category), or not at all (one group, likeliest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LootGroupBy {
    Container,
    Place,
    None,
}

impl LootGroupBy {
    /// Areas: by container (`Where in it`); items: by place.
    pub fn default_for(view: LootView) -> Self {
        match view {
            LootView::Areas => Self::Container,
            LootView::Items => Self::Place,
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
    /// Items: how many of `count` are NPC bodies ([`NPC_KIND`]); areas: 0.
    #[serde(default)]
    pub npcs: usize,
    /// Items: how many of `count` are NPC worn-gear rows ([`GEAR_KIND`]); areas: 0.
    #[serde(default)]
    pub worn: usize,
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
    /// Whether the NPC worn-gear data (`game-npc-loadouts`) is read: `missing` lists no
    /// [`GEAR_KIND`] rows until "Read game data" adds it.
    pub gear: LootDataState,
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
    /// What the odds depend on, when they do (worn gear of gated loadout groups: `if
    /// AI/NpcClass/Heavy`).
    pub note: Option<String>,
}

/// A link to another row of the areas list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootLink {
    pub id: String,
    pub name: String,
}

/// What an NPC row of the areas list says beyond its drops.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NpcInfo {
    /// Worn gear: the actor classes the row stands for (same name and gear).
    pub classes: Vec<String>,
    /// Worn gear: the faction (`Criminals`).
    pub faction: Option<String>,
    /// Worn gear: how worn the gear spawns, 0 (new) to 1 (`WearRange`).
    pub wear: Option<Wear>,
    /// Worn gear: the body-loot row of its loot table, when it has one.
    pub body_loot: Option<LootLink>,
    /// Body loot: the worn-gear rows of the NPCs whose bodies roll this table.
    pub worn_gear: Vec<LootLink>,
}

/// One group of a detail's drops: its header over every matching drop, and its rows when
/// they were asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootGroup {
    /// What `groups` takes to load its rows: the label, or [`ALL_GROUP`] ungrouped.
    pub key: String,
    /// `Weapons rack`, `Caves`, `FPS weapons`; `All drops` ungrouped.
    pub label: String,
    /// Drops in the group matching the filter (all of them without one).
    pub count: usize,
    /// Best rough odds per box in the group, 0 to 1.
    pub best: f64,
    /// Likeliest first, at most [`MAX_GROUP_ROWS`] ([`MAX_DROPS`] ungrouped); `None` when
    /// the group's rows were not asked for.
    pub rows: Option<Vec<LootDrop>>,
}

/// `loot_detail`: one area's items or one item's areas, in groups, likeliest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootDetail {
    pub state: LootDataState,
    pub view: LootView,
    /// `None` when nothing has that id (or the data is missing).
    pub entry: Option<LootSummary>,
    /// Items: the game record (what the Explorer opens).
    pub record: Option<ItemRecord>,
    /// NPC rows: classes, faction, condition and the links between body loot and worn gear.
    pub npc: Option<NpcInfo>,
    pub group_by: LootGroupBy,
    /// Every group with a drop matching the filter, best odds first, then by name.
    pub groups: Vec<LootGroup>,
    /// Drops matching the filter (all of them without one).
    pub total: usize,
    /// Every drop of the entry, filter or not.
    pub all: usize,
}

impl LootDetail {
    /// The rows of every loaded group, in group order.
    pub fn rows(&self) -> impl Iterator<Item = &LootDrop> {
        self.groups.iter().filter_map(|g| g.rows.as_ref()).flatten()
    }

    /// The group with that key.
    pub fn group(&self, key: &str) -> Option<&LootGroup> {
        self.groups.iter().find(|g| g.key == key)
    }
}

/// What a detail is asked for beyond the entry.
#[derive(Debug, Clone, Copy, Default)]
pub struct DetailQuery<'a> {
    /// Only the drops holding every word of it (name, tree path, container, rarity, id).
    pub filter: Option<&'a str>,
    /// `None`: [`LootGroupBy::default_for`] the view.
    pub group_by: Option<LootGroupBy>,
    /// The groups (by key) whose rows to answer; `None`: the default ones
    /// ([`LootScreen::detail`]).
    pub groups: Option<&'a [String]>,
}

impl<'a> DetailQuery<'a> {
    /// Only a filter: the view's default grouping and groups.
    pub fn filter(filter: Option<&'a str>) -> Self {
        Self {
            filter,
            ..Self::default()
        }
    }
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
    note: Option<u32>,
}

/// One worn-gear row: the actors of one NPC type with the same name and gear.
#[derive(Debug, Clone)]
struct GearRow {
    id: String,
    classes: Vec<String>,
    place: crate::loot_tree::GearPlace,
    items: Vec<GearItem>,
    wear: Option<Wear>,
    /// Lower-case loot table names of its actors.
    tables: BTreeSet<String>,
}

/// The worn-gear rows of a build: test actors left out, actors with the same faction, role,
/// name and gear collapsed into one row, names made unique within a faction (the body's
/// sex where that tells rows apart, else a variant number).
fn gear_rows(gear: &NpcLoadouts) -> Vec<GearRow> {
    use crate::loot_tree::{GearPlace, faction_label, gear_hidden, gear_name, npc_role};
    let mut rows: Vec<GearRow> = Vec::new();
    let mut sexes: Vec<BTreeSet<String>> = Vec::new();
    let mut at: HashMap<(String, &'static str, String, String), usize> = HashMap::new();
    // A body that cannot be looted gives nothing it wears.
    for a in gear
        .actors
        .iter()
        .filter(|a| a.lootable && !gear_hidden(&a.class))
    {
        let faction = faction_label(a.faction.as_deref());
        let role = npc_role(&a.class);
        let (name, sex) = gear_name(&a.name, a.faction.as_deref());
        let sig: Vec<String> = a
            .items
            .iter()
            .map(|i| {
                format!(
                    "{}|{}|{:.6}|{}",
                    i.item.to_ascii_lowercase(),
                    i.slot,
                    i.chance,
                    i.requires.join(",")
                )
            })
            .collect();
        let key = (faction.clone(), role, name.clone(), sig.join(";"));
        let ri = *at.entry(key).or_insert_with(|| {
            rows.push(GearRow {
                id: format!("{GEAR_PREFIX}{}", a.class),
                classes: Vec::new(),
                place: GearPlace {
                    faction,
                    role,
                    name,
                },
                items: a.items.clone(),
                wear: None,
                tables: BTreeSet::new(),
            });
            sexes.push(BTreeSet::new());
            rows.len() - 1
        });
        let r = &mut rows[ri];
        r.classes.push(a.class.clone());
        if let Some(w) = a.wear {
            r.wear = Some(match r.wear {
                Some(o) => Wear {
                    min: o.min.min(w.min),
                    max: o.max.max(w.max),
                },
                None => w,
            });
        }
        if let Some(t) = &a.loot_table {
            r.tables.insert(t.to_ascii_lowercase());
        }
        if let Some(s) = sex {
            sexes[ri].insert(s);
        }
    }
    // Same name in a faction: the sex when each says one and they differ, then a number.
    let mut same: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        same.entry((r.place.faction.clone(), r.place.name.clone()))
            .or_default()
            .push(i);
    }
    let mut clashes: Vec<Vec<usize>> = same.into_values().filter(|v| v.len() > 1).collect();
    clashes.sort();
    for group in clashes {
        let one = |i: usize| {
            (sexes[i].len() == 1)
                .then(|| sexes[i].iter().next().cloned())
                .flatten()
        };
        let labels: Vec<Option<String>> = group.iter().map(|&i| one(i)).collect();
        let distinct: BTreeSet<&Option<String>> = labels.iter().collect();
        let by_sex = labels.iter().all(Option::is_some) && distinct.len() == labels.len();
        for (n, (&i, sex)) in group.iter().zip(&labels).enumerate() {
            let extra = match sex {
                Some(s) if by_sex => s.clone(),
                _ => format!("variant {}", n + 1),
            };
            rows[i].place.name = format!("{} ({extra})", rows[i].place.name);
        }
    }
    rows
}

/// The Loot screen of one build: both lists and every pair, built once.
#[derive(Debug, Default)]
pub struct LootScreen {
    pub build: Option<String>,
    /// Whether the worn-gear rows were built (`game-npc-loadouts` was there).
    gear_ready: bool,
    /// Per area: what an NPC row says beyond its drops.
    area_npc: Vec<Option<NpcInfo>>,
    areas: Vec<LootSummary>,
    items: Vec<LootSummary>,
    records: Vec<ItemRecord>,
    /// Interned container, rarity and place labels, and the same in lower case.
    labels: Vec<String>,
    labels_lc: Vec<String>,
    /// Per area / item: the label of its place (the top of its tree path, else its group).
    area_place: Vec<u32>,
    item_place: Vec<u32>,
    /// Per area / item: name, tree path and id in lower case (what a filter searches).
    area_hay: Vec<String>,
    item_hay: Vec<String>,
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
    /// Every area spawn of every non-template item ([`BulkObtain::areas_all`], parallel)
    /// and what every NPC body can hold ([`BulkObtain::bodies_all`] over the actor tables),
    /// inverted per area or NPC. `build`: the label the lists carry.
    pub fn build(d: &Datasets, index: Option<&LootIndex>, build: Option<String>) -> Self {
        Self::build_with_gear(d, index, None, build)
    }

    /// [`Self::build`] plus the NPC worn-gear rows of `gear` (`game-npc-loadouts`; `None`:
    /// not read, [`LootList::gear`] says so).
    pub fn build_with_gear(
        d: &Datasets,
        index: Option<&LootIndex>,
        gear: Option<&NpcLoadouts>,
        build: Option<String>,
    ) -> Self {
        let bulk = BulkObtain::new(d, index);
        let list: Vec<&ItemTags> = d.items.iter().filter(|i| !i.template()).collect();
        let all = bulk.areas_all(&list);
        let npcs = index.map(npc_tables).unwrap_or_default();
        let tables: Vec<&str> = npcs.iter().map(String::as_str).collect();
        let bodies = if tables.is_empty() {
            Vec::new()
        } else {
            bulk.bodies_all(&tables, &list)
        };
        Self::from_sources(&list, &all, &tables, &bodies, gear, build)
    }

    /// [`Self::build`] from the ranked areas of each item.
    pub fn from_areas(
        items: &[&ItemTags],
        all: &[Vec<AreaSummary>],
        build: Option<String>,
    ) -> Self {
        Self::from_sources(items, all, &[], &[], None, build)
    }

    /// [`Self::build`] from the ranked areas of each item, the NPC bodies that can hold it
    /// (`bodies`, per item: a position in `npcs`, the actor tables, and its best slot;
    /// empty: none) and the NPC loadouts (`gear`; `None`: not read). A worn item item-tags
    /// lacks is listed by the name the loadouts give it.
    pub fn from_sources(
        items: &[&ItemTags],
        all: &[Vec<AreaSummary>],
        npcs: &[&str],
        bodies: &[Vec<(usize, BodyHit)>],
        gear: Option<&NpcLoadouts>,
        build: Option<String>,
    ) -> Self {
        let mut s = Self {
            build,
            gear_ready: gear.is_some(),
            ..Default::default()
        };
        let gear_rows = gear.map(gear_rows).unwrap_or_default();
        // Per lower-case item class: (row, chance, slot, note) of every row wearing it.
        type Worn = (usize, f64, String, Option<String>);
        let mut worn: HashMap<String, Vec<Worn>> = HashMap::new();
        for (ri, r) in gear_rows.iter().enumerate() {
            for it in &r.items {
                worn.entry(it.item.to_ascii_lowercase()).or_default().push((
                    ri,
                    it.chance,
                    it.slot.clone(),
                    crate::loot_tree::gear_note(&it.requires),
                ));
            }
        }
        // Worn items item-tags does not list: named from the loadouts.
        let listed: HashSet<String> = items
            .iter()
            .map(|i| i.class_name.to_ascii_lowercase())
            .collect();
        let mut extra: Vec<ItemTags> = Vec::new();
        let mut extra_seen: HashSet<String> = HashSet::new();
        for r in &gear_rows {
            for it in &r.items {
                let k = it.item.to_ascii_lowercase();
                if !listed.contains(&k) && extra_seen.insert(k) {
                    extra.push(ItemTags {
                        name: it.name.clone().unwrap_or_else(|| it.item.clone()),
                        class_name: it.item.clone(),
                        ..Default::default()
                    });
                }
            }
        }
        let no_areas: Vec<AreaSummary> = Vec::new();
        let all_items: Vec<&ItemTags> = items.iter().copied().chain(extra.iter()).collect();
        let mut label_ix: HashMap<String, u32> = HashMap::new();
        // Areas in first-seen order; sorted (and renumbered) below.
        let mut area_names: Vec<String> = Vec::new();
        let mut area_seen: HashMap<String, u32> = HashMap::new();
        let makers = maker_codes(items);
        // (item, its name, its pairs)
        let mut kept_items: Vec<(&ItemTags, String, Vec<Pair>)> = Vec::new();
        let areas_of = |ii: usize| all.get(ii).unwrap_or(&no_areas);
        for (ii, item) in all_items.iter().enumerate() {
            let areas = areas_of(ii);
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
                    note: None,
                });
            }
            for (ti, h) in bodies.get(ii).into_iter().flatten() {
                if h.odds <= 0.0 {
                    continue;
                }
                let id = format!("{NPC_PREFIX}{}", npcs[*ti]);
                let ai = *area_seen.entry(id.clone()).or_insert_with(|| {
                    area_names.push(id);
                    (area_names.len() - 1) as u32
                });
                let label = crate::loot_tree::npc_slot_label(npcs[*ti], &h.label);
                pairs.push(Pair {
                    item: 0,
                    area: ai,
                    odds: h.odds,
                    container: intern(&mut s.labels, &mut label_ix, label),
                    rarity: loot_eligibility(&item.tags)
                        .rarity
                        .map(|r| intern(&mut s.labels, &mut label_ix, r)),
                    kind: LootDropKind::Loot,
                    note: None,
                });
            }
            for (ri, chance, slot, note) in worn
                .get(&item.class_name.to_ascii_lowercase())
                .into_iter()
                .flatten()
            {
                if *chance <= 0.0 {
                    continue;
                }
                let id = &gear_rows[*ri].id;
                let ai = *area_seen.entry(id.clone()).or_insert_with(|| {
                    area_names.push(id.clone());
                    (area_names.len() - 1) as u32
                });
                pairs.push(Pair {
                    item: 0,
                    area: ai,
                    odds: *chance,
                    container: intern(&mut s.labels, &mut label_ix, slot.clone()),
                    rarity: None,
                    kind: LootDropKind::Worn,
                    note: note
                        .clone()
                        .map(|n| intern(&mut s.labels, &mut label_ix, n)),
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
        // Places, NPC bodies and NPC gear have trees of their own.
        let (gear_ix, rest): (Vec<usize>, Vec<usize>) =
            (0..area_names.len()).partition(|&i| is_gear(&area_names[i]));
        let (npc_ix, place_ix): (Vec<usize>, Vec<usize>) =
            rest.into_iter().partition(|&i| is_npc(&area_names[i]));
        let ids: Vec<&str> = place_ix.iter().map(|&i| area_names[i].as_str()).collect();
        let npc_ids: Vec<&str> = npc_ix
            .iter()
            .map(|&i| &area_names[i][NPC_PREFIX.len()..])
            .collect();
        let mut tree = vec![(String::new(), Vec::new(), String::new()); area_names.len()];
        for (&i, (path, name)) in place_ix.iter().zip(crate::loot_tree::area_tree(&ids)) {
            tree[i] = (area_group(&area_names[i]), path, name);
        }
        for (&i, (path, name)) in npc_ix.iter().zip(crate::loot_tree::npc_tree(&npc_ids)) {
            tree[i] = (NPC_KIND.to_string(), path, name);
        }
        // Gear rows that wear something listed, in row order.
        let row_of: HashMap<&str, usize> = gear_rows
            .iter()
            .enumerate()
            .map(|(ri, r)| (r.id.as_str(), ri))
            .collect();
        let gear_places: Vec<crate::loot_tree::GearPlace> = gear_ix
            .iter()
            .map(|&i| gear_rows[row_of[area_names[i].as_str()]].place.clone())
            .collect();
        for (&i, (path, name)) in gear_ix
            .iter()
            .zip(crate::loot_tree::gear_tree(&gear_places))
        {
            tree[i] = (GEAR_KIND.to_string(), path, name);
        }
        let mut area_order: Vec<(String, Vec<String>, String, u32)> = tree
            .into_iter()
            .enumerate()
            .map(|(i, (group, path, name))| (group, path, name, i as u32))
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
                npcs: 0,
                worn: 0,
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
            let npcs = mine
                .iter()
                .filter(|&&pi| is_npc(&s.areas[s.pairs[pi as usize].area as usize].id))
                .count();
            s.items.push(LootSummary {
                id: item.class_name.clone(),
                name,
                group,
                path,
                count: mine.len(),
                npcs,
                worn: mine
                    .iter()
                    .filter(|&&pi| is_gear(&s.areas[s.pairs[pi as usize].area as usize].id))
                    .count(),
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
        // What NPC rows say beyond their drops: a gear row's classes, faction, condition and
        // body-loot row; a body-loot row's gear rows (in list order).
        let link = |i: usize| LootLink {
            id: s.areas[i].id.clone(),
            name: s.areas[i].name.clone(),
        };
        let table_row: HashMap<String, usize> = (0..s.areas.len())
            .filter(|&i| is_npc(&s.areas[i].id))
            .map(|i| (s.areas[i].id[NPC_PREFIX.len()..].to_ascii_lowercase(), i))
            .collect();
        let mut infos: Vec<Option<NpcInfo>> = (0..s.areas.len())
            .map(|i| is_npc(&s.areas[i].id).then(NpcInfo::default))
            .collect();
        let mut worn_by: HashMap<usize, Vec<usize>> = HashMap::new();
        for r in &gear_rows {
            let Some(&ai) = s.area_ix.get(&r.id.to_ascii_lowercase()) else {
                continue;
            };
            let body = r.tables.iter().find_map(|t| table_row.get(t).copied());
            if let Some(b) = body {
                worn_by.entry(b).or_default().push(ai);
            }
            infos[ai] = Some(NpcInfo {
                classes: r.classes.clone(),
                faction: Some(r.place.faction.clone()),
                wear: r.wear,
                body_loot: body.map(link),
                worn_gear: Vec::new(),
            });
        }
        for (b, mut list) in worn_by {
            list.sort_unstable();
            if let Some(info) = infos[b].as_mut() {
                info.worn_gear = list.into_iter().map(link).collect();
            }
        }
        s.area_npc = infos;
        let place = |r: &LootSummary| r.path.first().unwrap_or(&r.group).clone();
        let hay =
            |r: &LootSummary| format!("{} {} {}", r.name, where_label(r), r.id).to_lowercase();
        for a in &s.areas {
            let p = intern(&mut s.labels, &mut label_ix, place(a));
            s.area_place.push(p);
            s.area_hay.push(hay(a));
        }
        for i in &s.items {
            let p = intern(&mut s.labels, &mut label_ix, place(i));
            s.item_place.push(p);
            s.item_hay.push(hay(i));
        }
        s.labels_lc = s.labels.iter().map(|l| l.to_lowercase()).collect();
        s
    }

    /// Whether a pair's drop (as `view` shows it) holds every (lower-case) word in its name,
    /// tree path, id, container or rarity.
    fn pair_matches(&self, view: LootView, pi: u32, words: &[String]) -> bool {
        let p = &self.pairs[pi as usize];
        let other = match view {
            LootView::Areas => &self.item_hay[p.item as usize],
            LootView::Items => &self.area_hay[p.area as usize],
        };
        let container = &self.labels_lc[p.container as usize];
        let rarity = p.rarity.map_or("", |r| self.labels_lc[r as usize].as_str());
        let note = p.note.map_or("", |r| self.labels_lc[r as usize].as_str());
        words.iter().all(|w| {
            let w = w.as_str();
            other.contains(w) || container.contains(w) || rarity.contains(w) || note.contains(w)
        })
    }

    fn drop_of(&self, view: LootView, pi: u32) -> LootDrop {
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
            note: p.note.map(|r| self.labels[r as usize].clone()),
        }
    }

    pub fn list(&self, view: LootView) -> LootList {
        LootList {
            state: LootDataState::Ready,
            view,
            build: self.build.clone(),
            gear: if self.gear_ready {
                LootDataState::Ready
            } else {
                LootDataState::Missing
            },
            rows: match view {
                LootView::Areas => self.areas.clone(),
                LootView::Items => self.items.clone(),
            },
        }
    }

    /// One area's items or one item's areas, in groups ([`DetailQuery::group_by`]), each
    /// likeliest first. `filter`: only the drops holding every word of it (in the name, tree
    /// path, container, rarity or id), over all the entry's drops. Every group with a match
    /// gets its header (count and best odds of all its matching drops); rows come for the
    /// groups in [`DetailQuery::groups`], else for the default ones ([`default_open`]). Each
    /// group's rows are capped at [`MAX_GROUP_ROWS`] ([`MAX_DROPS`] ungrouped). One pass
    /// over the entry's pair index: nothing is rebuilt.
    pub fn detail(&self, view: LootView, id: &str, q: &DetailQuery) -> LootDetail {
        let group_by = q.group_by.unwrap_or(LootGroupBy::default_for(view));
        let key = id.trim().to_ascii_lowercase();
        let found = match view {
            LootView::Areas => self.area_ix.get(&key),
            LootView::Items => self.item_ix.get(&key),
        };
        let Some(&i) = found else {
            return LootDetail {
                state: LootDataState::Ready,
                group_by,
                ..missing_detail(view)
            };
        };
        let (entry, record, list) = match view {
            LootView::Areas => (self.areas[i].clone(), None, &self.by_area[i]),
            LootView::Items => (
                self.items[i].clone(),
                // A worn item item-tags lacks has no record to open.
                Some(self.records[i].clone()).filter(|r| !r.guid.is_empty()),
                &self.by_item[i],
            ),
        };
        let npc = match view {
            LootView::Areas => self.area_npc.get(i).cloned().flatten(),
            LootView::Items => None,
        };
        // A gear row's slots in body order (Undersuit, Helmet, ...), not by odds.
        let by_slot =
            view == LootView::Areas && group_by == LootGroupBy::Container && is_gear(&entry.id);
        let words: Vec<String> = q
            .filter
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        // (label, the group's matching pairs likeliest first), first seen first.
        let mut at: HashMap<u32, usize> = HashMap::new();
        let mut acc: Vec<(u32, Vec<u32>)> = Vec::new();
        for &pi in list {
            if !words.is_empty() && !self.pair_matches(view, pi, &words) {
                continue;
            }
            let p = &self.pairs[pi as usize];
            let g = match (group_by, view) {
                (LootGroupBy::Container, _) => p.container,
                (LootGroupBy::Place, LootView::Areas) => self.item_place[p.item as usize],
                (LootGroupBy::Place, LootView::Items) => self.area_place[p.area as usize],
                (LootGroupBy::None, _) => u32::MAX,
            };
            let gi = *at.entry(g).or_insert_with(|| {
                acc.push((g, Vec::new()));
                acc.len() - 1
            });
            acc[gi].1.push(pi);
        }
        let flat = group_by == LootGroupBy::None;
        let label = |g: u32| {
            if flat {
                "All drops"
            } else {
                self.labels[g as usize].as_str()
            }
        };
        let key_of = |g: u32| if flat { ALL_GROUP } else { label(g) };
        let best = |pis: &[u32]| pis.first().map_or(0.0, |&p| self.pairs[p as usize].odds);
        acc.sort_by(|a, b| {
            let slots = if by_slot {
                slot_rank(label(a.0)).cmp(&slot_rank(label(b.0)))
            } else {
                std::cmp::Ordering::Equal
            };
            slots
                .then_with(|| best(&b.1).total_cmp(&best(&a.1)))
                .then_with(|| cmp_name(label(a.0), label(b.0)))
        });
        let cap = if flat { MAX_DROPS } else { MAX_GROUP_ROWS };
        let counts: Vec<usize> = acc.iter().map(|(_, pis)| pis.len()).collect();
        let open: Vec<bool> = match q.groups {
            Some(keys) => acc
                .iter()
                .map(|(g, _)| keys.iter().any(|k| k == key_of(*g)))
                .collect(),
            None => default_open(&counts, cap, !words.is_empty()),
        };
        let groups = acc
            .iter()
            .zip(open)
            .map(|((g, pis), open)| LootGroup {
                key: key_of(*g).to_string(),
                label: label(*g).to_string(),
                count: pis.len(),
                best: best(pis),
                rows: open.then(|| {
                    pis.iter()
                        .take(cap)
                        .map(|&pi| self.drop_of(view, pi))
                        .collect()
                }),
            })
            .collect();
        LootDetail {
            state: LootDataState::Ready,
            view,
            entry: Some(entry),
            record,
            npc,
            group_by,
            groups,
            total: counts.iter().sum(),
            all: list.len(),
        }
    }
}

/// The groups a detail opens when none are asked for (`counts`: each group's drops): the
/// first, then the next while the opened ones show at most [`OPEN_BUDGET`] rows together
/// (a few small groups), or [`MAX_DROPS`] with a filter (its matches show open).
fn default_open(counts: &[usize], cap: usize, filtered: bool) -> Vec<bool> {
    let budget = if filtered { MAX_DROPS } else { OPEN_BUDGET };
    let mut used = 0;
    let mut stop = false;
    counts
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let n = n.min(cap);
            if stop || (i > 0 && used + n > budget) {
                stop = true;
                return false;
            }
            used += n;
            true
        })
        .collect()
}

/// The actor loot tables of NPC bodies, in index order ([`crate::loot_tree::is_actor_table`]).
pub fn npc_tables(index: &LootIndex) -> Vec<String> {
    index
        .tables
        .iter()
        .filter(|t| crate::loot_tree::is_actor_table(&t.name))
        .map(|t| t.name.clone())
        .collect()
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
        gear: LootDataState::Missing,
        rows: Vec::new(),
    }
}

pub fn missing_detail(view: LootView) -> LootDetail {
    LootDetail {
        state: LootDataState::Missing,
        view,
        entry: None,
        record: None,
        npc: None,
        group_by: LootGroupBy::default_for(view),
        groups: Vec::new(),
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

    /// One flat group (ungrouped), every row.
    fn flat(s: &LootScreen, view: LootView, id: &str, filter: Option<&str>) -> LootDetail {
        s.detail(
            view,
            id,
            &DetailQuery {
                filter,
                group_by: Some(LootGroupBy::None),
                groups: None,
            },
        )
    }

    fn rows(d: &LootDetail) -> Vec<&LootDrop> {
        d.rows().collect()
    }

    #[test]
    fn details_are_likeliest_first() {
        let s = screen();
        let cave = flat(&s, LootView::Areas, "CAVES/loot_caves_occupied", None);
        assert_eq!(cave.entry.as_ref().unwrap().name, "Caves: Occupied");
        assert_eq!(cave.group_by, LootGroupBy::None);
        assert_eq!(cave.groups.len(), 1);
        assert_eq!(cave.groups[0].key, ALL_GROUP);
        let r = rows(&cave);
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["FS-9 LMG", "Arclight Pistol"]);
        assert_eq!(r[0].container, "Large weapons crate");
        assert_eq!(r[0].kind, LootDropKind::Loot);
        assert_eq!((cave.total, cave.all), (2, 2));
        assert_eq!(r[0].group, "FPS weapons");

        let lmg = flat(&s, LootView::Items, "behr_lmg_01", None);
        assert_eq!(lmg.record.as_ref().unwrap().guid, "guid-behr_lmg_01");
        // Caves: 0.75 per box; the derelict outpost's crate is 3 of 4 spawns: 0.5625.
        let r = rows(&lmg);
        assert!(r.windows(2).all(|w| w[0].odds >= w[1].odds));
        assert_eq!(r[0].id, "caves/loot_caves_occupied");
        assert!((r[1].odds - 0.5625).abs() < 1e-9);

        let helmet = flat(&s, LootView::Items, "rsi_helmet_01", None);
        assert_eq!(rows(&helmet)[0].kind, LootDropKind::Fixed);
        assert!((rows(&helmet)[0].odds - 0.25).abs() < 1e-9);

        let none = s.detail(LootView::Items, "store_only", &DetailQuery::default());
        assert!(none.entry.is_none() && none.groups.is_empty());
        assert_eq!(none.state, LootDataState::Ready);
        assert_eq!(none.group_by, LootGroupBy::Place);
    }

    #[test]
    fn detail_filter_reaches_every_drop() {
        let s = screen();
        let cave = "caves/loot_caves_occupied";
        // Every word must match (name, path, container, rarity or id), any case.
        let d = flat(&s, LootView::Areas, cave, Some("  ARC  "));
        let names: Vec<&str> = d.rows().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Arclight Pistol"]);
        assert_eq!((d.total, d.all), (1, 2));
        let d = flat(&s, LootView::Areas, cave, Some("weapons ksar"));
        assert_eq!(rows(&d)[0].id, "ksar_pistol_01");
        // A word of the tree path, and one of the container.
        assert_eq!(flat(&s, LootView::Areas, cave, Some("fps")).total, 2);
        assert_eq!(
            flat(&s, LootView::Areas, cave, Some("large crate")).total,
            2
        );
        let d = flat(&s, LootView::Areas, cave, Some("nothing"));
        assert!(d.entry.is_some() && d.groups.is_empty());
        assert_eq!((d.total, d.all), (0, 2));
        let d = flat(&s, LootView::Items, "behr_lmg_01", Some("derelict"));
        assert_eq!(rows(&d).len(), 1);
        assert_eq!(rows(&d)[0].id, "lootgeneration_slotpreset_derelictoutpost");
        // A blank filter is none.
        assert_eq!(flat(&s, LootView::Items, "behr_lmg_01", Some(" ")).total, 2);
    }

    #[test]
    fn details_group_by_container_and_place() {
        let s = screen();
        // Areas default to the container: the crate (0.5625 and less), then the rack (0.25).
        let d = s.detail(
            LootView::Areas,
            "lootgeneration_slotpreset_derelictoutpost",
            &DetailQuery::default(),
        );
        assert_eq!(d.group_by, LootGroupBy::Container);
        let heads: Vec<(&str, usize)> = d
            .groups
            .iter()
            .map(|g| (g.label.as_str(), g.count))
            .collect();
        assert_eq!(heads.len(), 2, "{heads:?}");
        assert_eq!(heads[1].0, "Weapons rack");
        assert!(d.groups[0].best > d.groups[1].best);
        assert_eq!(d.groups.iter().map(|g| g.count).sum::<usize>(), d.total);
        // Both small: open by default; every row is in its group's container.
        assert!(d.groups.iter().all(|g| g.rows.is_some()));
        for g in &d.groups {
            assert!(
                g.rows
                    .as_ref()
                    .unwrap()
                    .iter()
                    .all(|r| r.container == g.label)
            );
        }
        // Items default to the place: the top of the area's tree, else its kind of place.
        let d = s.detail(LootView::Items, "behr_lmg_01", &DetailQuery::default());
        assert_eq!(d.group_by, LootGroupBy::Place);
        let labels: Vec<&str> = d.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Caves", "Derelict outposts"]);
        // Only the groups asked for carry rows; an unknown key is ignored.
        let want = vec!["Derelict outposts".to_string(), "nope".to_string()];
        let d = s.detail(
            LootView::Items,
            "behr_lmg_01",
            &DetailQuery {
                groups: Some(&want),
                ..DetailQuery::default()
            },
        );
        assert!(d.groups[0].rows.is_none());
        assert_eq!(d.groups[1].rows.as_ref().unwrap().len(), 1);
        // Areas by place: the item's category.
        let d = s.detail(
            LootView::Areas,
            "lootgeneration_slotpreset_derelictoutpost",
            &DetailQuery {
                group_by: Some(LootGroupBy::Place),
                ..DetailQuery::default()
            },
        );
        let labels: Vec<&str> = d.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["FPS weapons", "Armour & clothing"]);
    }

    #[test]
    fn groups_open_the_first_and_a_few_small_ones() {
        assert_eq!(
            default_open(&[3, 5, 30, 5], 500, false),
            [true, true, true, false]
        );
        assert_eq!(default_open(&[900, 1, 1], 500, false), [true, false, false]);
        assert_eq!(default_open(&[3, 50, 1], 500, false), [true, false, false]);
        // With a filter, as many as fit MAX_DROPS shown rows (each group capped).
        assert_eq!(
            default_open(&[900, 900, 900, 900, 900], 500, true),
            [true, true, true, true, false]
        );
        assert!(default_open(&[], 500, true).is_empty());
    }

    fn hit(area: &str, kind: SpawnKind, score: f64) -> AreaSummary {
        AreaSummary {
            area: area.to_string(),
            best: SpawnHit {
                area: area.to_string(),
                config: None,
                spawn_tags: Vec::new(),
                entity: "crate_weapons".into(),
                entity_name: None,
                slot_share: 1.0,
                kind,
                table: None,
                per_roll: None,
                max_results: None,
                score,
                paths: None,
            },
            configs: 1,
        }
    }

    fn guns(n: usize) -> Vec<ItemTags> {
        (0..n)
            .map(|i| {
                item(
                    &format!("gun_{i:05}"),
                    &format!("Gun {i:05}"),
                    "WeaponPersonal",
                    &[],
                )
            })
            .collect()
    }

    /// More drops than [`MAX_DROPS`]: the flat list stops there, a filter reaches the tail.
    #[test]
    fn detail_is_capped_and_the_filter_reaches_the_tail() {
        let n = MAX_DROPS + 5;
        let items = guns(n);
        let refs: Vec<&ItemTags> = items.iter().collect();
        let all: Vec<Vec<AreaSummary>> = (0..n)
            .map(|i| {
                let score = 1.0 - i as f64 / (n as f64 * 2.0);
                vec![hit("caves/loot_caves_occupied", SpawnKind::Loot, score)]
            })
            .collect();
        let s = LootScreen::from_areas(&refs, &all, None);
        let d = flat(&s, LootView::Areas, "caves/loot_caves_occupied", None);
        assert_eq!((rows(&d).len(), d.total, d.all), (MAX_DROPS, n, n));
        assert_eq!(d.groups[0].count, n);
        let last = format!("gun_{:05}", n - 1);
        assert!(d.rows().all(|r| r.id != last));
        let d = flat(
            &s,
            LootView::Areas,
            "caves/loot_caves_occupied",
            Some(&last),
        );
        assert_eq!(rows(&d).len(), 1);
        assert_eq!(rows(&d)[0].id, last);
    }

    /// Group headers count every drop, past [`MAX_DROPS`] and [`MAX_GROUP_ROWS`]; rows stop
    /// at the group cap; the filter narrows the headers too.
    #[test]
    fn group_headers_cover_every_drop() {
        let n = MAX_DROPS + 1000;
        let items = guns(n);
        let refs: Vec<&ItemTags> = items.iter().collect();
        let score = |i: usize| 0.9 - i as f64 / (n as f64 * 2.0);
        // Every third gun lies loose; the rest are in the crate. The loose ones' best is
        // gun 0; the crate's is gun 1.
        let all: Vec<Vec<AreaSummary>> = (0..n)
            .map(|i| {
                let kind = if i % 3 == 0 {
                    SpawnKind::Loose
                } else {
                    SpawnKind::Loot
                };
                vec![hit("caves/loot_caves_occupied", kind, score(i))]
            })
            .collect();
        let s = LootScreen::from_areas(&refs, &all, None);
        let area = "caves/loot_caves_occupied";
        let d = s.detail(LootView::Areas, area, &DetailQuery::default());
        assert_eq!((d.total, d.all), (n, n));
        assert_eq!(d.groups.len(), 2);
        let loose = n.div_ceil(3);
        assert_eq!(d.groups[0].label, "Lying loose");
        assert_eq!(d.groups[0].count, loose);
        assert_eq!(d.groups[1].count, n - loose);
        assert!((d.groups[0].best - score(0)).abs() < 1e-12);
        assert!((d.groups[1].best - score(1)).abs() < 1e-12);
        // Only the first opens (it is big); its rows stop at the group cap, likeliest first.
        let first = d.groups[0].rows.as_ref().unwrap();
        assert_eq!(first.len(), MAX_GROUP_ROWS);
        assert!(first.windows(2).all(|w| w[0].odds >= w[1].odds));
        assert!(d.groups[1].rows.is_none());
        // Asking for the second loads it alone.
        let key = vec![d.groups[1].key.clone()];
        let d2 = s.detail(
            LootView::Areas,
            area,
            &DetailQuery {
                groups: Some(&key),
                ..DetailQuery::default()
            },
        );
        assert!(d2.groups[0].rows.is_none());
        assert_eq!(d2.groups[1].rows.as_ref().unwrap().len(), MAX_GROUP_ROWS);
        assert_eq!(d2.groups[1].count, n - loose);
        // A filter: headers count the matches only, and the matching groups open.
        let f = s.detail(LootView::Areas, area, &DetailQuery::filter(Some("gun_029")));
        // gun_02900..gun_02999: 100 guns, a third of them loose (multiples of 3).
        assert_eq!(f.total, 100);
        let loose_hits = (2900..3000).filter(|i| i % 3 == 0).count();
        assert_eq!(f.group("Lying loose").unwrap().count, loose_hits);
        assert!(
            f.groups
                .iter()
                .all(|g| g.rows.as_ref().unwrap().len() == g.count)
        );
        // Groups without a match are left out.
        let f = s.detail(LootView::Areas, area, &DetailQuery::filter(Some("loose")));
        assert_eq!(f.groups.len(), 1);
        assert_eq!(f.total, loose);
    }

    /// The test build plus NPC bodies: a v1 pilot whose scrip slot is always filled, a V3
    /// soldier and its council-scrip variant (a priority slot that exists half the time,
    /// naming its item by class), and a template that is no NPC.
    fn npc_screen_with(gear: Option<&NpcLoadouts>) -> LootScreen {
        let mut d = datasets();
        let scrip = "LootGeneration/LootType/Scrip";
        d.items.push(item(
            "scrip_council",
            "Council Scrip",
            "Misc",
            &[CAN, COMMON, scrip],
        ));
        // Same tags as the council scrip, another class: an entity-class entry leaves it out.
        d.items.push(item(
            "scrip_merc",
            "Merc Scrip",
            "Misc",
            &[CAN, COMMON, scrip],
        ));
        let loot: LootData = serde_json::from_value(json!({
            "tables": [
                {"name": "Crate_Weapons", "lootArchetypes": [{"archetype": "Arch_Guns", "weight": 1}]},
                {"name": "LootTable_Actor_AcePilot", "version": "v1", "lootArchetypes": [
                    {"archetype": "lootarchetype_actor_acepilot_scrip", "weight": 1,
                     "numberOfResultsConstraints": {"minResults": 1, "maxResults": 1}},
                    {"archetype": "Arch_Guns", "weight": 0.5,
                     "numberOfResultsConstraints": {"minResults": 0, "maxResults": 1}}]},
                {"name": "V3LootTable_Actor_Soldier", "version": "v3", "lootArchetypes": [
                    {"name": "Ammo", "weight": 3, "archetype": {"entries": [
                        {"name": "Guns", "weight": 1, "selector": {"tags": {"positiveTags": ["Weapon/FPS/Stocked/LMG"]}}}]}},
                    {"name": "Sidearm", "weight": 1, "archetype": {"entries": [
                        {"name": "Pistol", "weight": 1, "selector": {"tags": {"positiveTags": ["Weapon/FPS/Pistol"]}}}]}}]},
                {"name": "V3LootTable_Actor_Soldier_CouncilScrip", "version": "v3", "lootArchetypes": [
                    {"name": "Ammo", "weight": 3, "archetype": {"entries": [
                        {"name": "Guns", "weight": 1, "selector": {"tags": {"positiveTags": ["Weapon/FPS/Stocked/LMG"]}}}]}},
                    {"name": "Unique item", "weight": 1, "archetype": {"entries": [
                        {"name": "Council scrip", "weight": 1, "selector": {"entityClasses": {"entityClasses": ["Scrip_Council"]}}}]},
                     "optionalData": [{"priorityCount": 1}, {"choiceLimit": 1}, {"chanceToExist": 0.4}]}]},
                {"name": "V3LootTable_Actor_TEMPLATE", "version": "v3", "lootArchetypes": [
                    {"name": "Ammo", "weight": 1, "archetype": {"entries": [
                        {"name": "Guns", "weight": 1, "selector": {"tags": {"positiveTags": ["Weapon/FPS/Pistol"]}}}]}}]}
            ],
            "archetypes": [
                {"name": "Arch_Guns", "primaryOrGroup": {"entries": [
                    {"name": "LMG", "tag": "Weapon/FPS/Stocked/LMG", "weight": 3},
                    {"name": "Pistol", "tag": "Weapon/FPS/Pistol", "weight": 1}]}},
                {"name": "LootArchetype_Actor_AcePilot_Scrip", "primaryOrGroup": {"entries": [
                    {"name": "Scrip", "tag": scrip, "weight": 1}]}}
            ]
        }))
        .unwrap();
        d.loot = Some(loot);
        let index = build_loot_index(d.loot.as_ref().unwrap());
        LootScreen::build_with_gear(&d, Some(&index), gear, None)
    }

    fn npc_screen() -> LootScreen {
        npc_screen_with(None)
    }

    fn gear_item(item: &str, slot: &str, chance: f64, requires: &[&str]) -> GearItem {
        GearItem {
            item: item.into(),
            name: None,
            slot: slot.into(),
            chance,
            requires: requires.iter().map(|r| r.to_string()).collect(),
        }
    }

    /// NPC loadouts: the ace pilot (its body-loot table, a gated sidearm, a suit item-tags
    /// lacks), two grunts with the same gear (male and female: one row), a third grunt
    /// with other gear (the same name: a variant), and a test actor.
    fn gear() -> NpcLoadouts {
        use starorg_gamedata::domains::npc_loadouts::NpcActor;
        let actor = |class: &str, name: &str, table: Option<&str>, items: Vec<GearItem>| NpcActor {
            class: class.into(),
            name: name.into(),
            faction: Some("genericcriminal".into()),
            loot_table: table.map(str::to_string),
            lootable: true,
            wear: Some(Wear { min: 0.0, max: 0.5 }),
            items,
        };
        let mut suit = gear_item("syn_suit_01", "Undersuit", 1.0, &[]);
        suit.name = Some("Synthetic Suit".into());
        let grunt = || {
            vec![
                gear_item("rsi_helmet_01", "Helmet", 1.0, &[]),
                gear_item("ksar_pistol_01", "Sidearm", 0.5, &[]),
            ]
        };
        NpcLoadouts {
            actors: vec![
                actor(
                    "PU_Human_Enemy_GroundCombat_NPC_GenericCriminal_AcePilot",
                    "Generic criminal ace pilot",
                    Some("loottable_actor_acepilot"),
                    vec![
                        suit,
                        gear_item("rsi_helmet_01", "Helmet", 1.0, &[]),
                        gear_item("behr_lmg_01", "Primary weapon", 0.5, &[]),
                        gear_item("ksar_pistol_01", "Sidearm", 1.0, &["AI/NpcClass/Heavy"]),
                    ],
                ),
                actor(
                    "PU_Human-Criminal-Grunt-Female-Grunt_01",
                    "Criminal grunt female 01",
                    None,
                    grunt(),
                ),
                actor(
                    "PU_Human-Criminal-Grunt-Male-Grunt_01",
                    "Criminal grunt male 01",
                    None,
                    grunt(),
                ),
                actor(
                    "PU_Human-Criminal-Grunt-Male-Stanton-Grunt_01",
                    "Criminal grunt male 01",
                    None,
                    vec![gear_item("behr_lmg_01", "Primary weapon", 1.0, &[])],
                ),
                actor(
                    "FeatureTest_Human_Male-Grunt",
                    "Feature test grunt",
                    None,
                    grunt(),
                ),
                // A body that cannot be looted: no row.
                NpcActor {
                    lootable: false,
                    ..actor(
                        "PU_Pilots-Vanduul-Pilot_Light",
                        "Pilots vanduul pilot light",
                        None,
                        grunt(),
                    )
                },
            ],
            ..Default::default()
        }
    }

    fn gear_screen() -> LootScreen {
        npc_screen_with(Some(&gear()))
    }

    const ACE: &str = "gear:PU_Human_Enemy_GroundCombat_NPC_GenericCriminal_AcePilot";

    #[test]
    fn worn_gear_rows_sit_by_faction_and_collapse() {
        assert_eq!(
            npc_screen().list(LootView::Areas).gear,
            LootDataState::Missing
        );
        let s = gear_screen();
        let list = s.list(LootView::Areas);
        assert_eq!(list.gear, LootDataState::Ready);
        let rows: Vec<&LootSummary> = list.rows.iter().filter(|r| is_gear(&r.id)).collect();
        let shown: Vec<(&str, Vec<String>)> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.path.clone()))
            .collect();
        // One faction (all rows): no faction level; the grunts' role holds two of three.
        let k = GEAR_KIND.to_string();
        let ground = crate::loot_tree::ROLE_COMBAT.to_string();
        assert_eq!(
            shown,
            [
                ("Ace pilot", vec![k.clone()]),
                ("Grunt 01 (variant 1)", vec![k.clone(), ground.clone()]),
                ("Grunt 01 (variant 2)", vec![k, ground]),
            ],
            "{shown:?}"
        );
        assert!(rows.iter().all(|r| r.group == GEAR_KIND));
        // Male and female with the same gear: one row listing both classes.
        let twins = rows
            .iter()
            .map(|r| {
                s.detail(LootView::Areas, &r.id, &DetailQuery::default())
                    .npc
                    .unwrap()
                    .classes
            })
            .find(|c| c.len() == 2)
            .unwrap();
        assert_eq!(
            twins,
            [
                "PU_Human-Criminal-Grunt-Female-Grunt_01",
                "PU_Human-Criminal-Grunt-Male-Grunt_01"
            ]
        );
        // The test actor is no row.
        assert!(
            list.rows
                .iter()
                .all(|r| !r.id.contains("FeatureTest") && !r.id.contains("Vanduul"))
        );
    }

    #[test]
    fn worn_gear_detail_is_by_slot_with_links() {
        let s = gear_screen();
        let d = s.detail(LootView::Areas, ACE, &DetailQuery::default());
        let labels: Vec<&str> = d.groups.iter().map(|g| g.label.as_str()).collect();
        // Body order, not odds order.
        assert_eq!(labels, ["Undersuit", "Helmet", "Primary weapon", "Sidearm"]);
        let r: Vec<&LootDrop> = d.rows().collect();
        assert!(r.iter().all(|r| r.kind == LootDropKind::Worn));
        let pistol = r.iter().find(|r| r.id == "ksar_pistol_01").unwrap();
        assert_eq!(pistol.note.as_deref(), Some("if AI/NpcClass/Heavy"));
        assert!(close(pistol.odds, 1.0));
        let suit = r.iter().find(|r| r.id == "syn_suit_01").unwrap();
        assert_eq!(suit.name, "Synthetic Suit");
        let npc = d.npc.as_ref().unwrap();
        assert_eq!(npc.faction.as_deref(), Some("Criminals"));
        assert_eq!(npc.wear, Some(Wear { min: 0.0, max: 0.5 }));
        assert_eq!(
            npc.body_loot,
            Some(LootLink {
                id: "npc:LootTable_Actor_AcePilot".into(),
                name: "Ace pilot".into()
            })
        );
        // The body-loot row links back.
        let b = s.detail(
            LootView::Areas,
            "npc:LootTable_Actor_AcePilot",
            &DetailQuery::default(),
        );
        let back = b.npc.unwrap().worn_gear;
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].id, ACE);
        // Grouped by place, the order is by odds again.
        let p = s.detail(
            LootView::Areas,
            ACE,
            &DetailQuery {
                group_by: Some(LootGroupBy::None),
                ..DetailQuery::default()
            },
        );
        assert_eq!(p.total, 4);
        // A filter word of the note finds the gated row.
        let f = s.detail(LootView::Areas, ACE, &DetailQuery::filter(Some("npcclass")));
        assert_eq!(f.total, 1);
    }

    #[test]
    fn items_list_the_npcs_that_wear_them() {
        let s = gear_screen();
        let d = s.detail(LootView::Items, "rsi_helmet_01", &DetailQuery::default());
        let g = d.group(GEAR_KIND).unwrap();
        assert_eq!(g.count, 2, "the ace pilot and the two grunts of one row");
        let names: Vec<&str> = g
            .rows
            .as_ref()
            .unwrap()
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert!(names.contains(&"Ace pilot"), "{names:?}");
        assert!(
            g.rows
                .as_ref()
                .unwrap()
                .iter()
                .all(|r| r.container == "Helmet")
        );
        // The suit item-tags lacks is listed by its loadout name, with no record to open.
        let items = s.list(LootView::Items).rows;
        let suit = items.iter().find(|r| r.id == "syn_suit_01").unwrap();
        assert_eq!(suit.name, "Synthetic Suit");
        let d = s.detail(LootView::Items, "syn_suit_01", &DetailQuery::default());
        assert!(d.record.is_none());
        assert_eq!(d.rows().next().unwrap().id, ACE);
        // A filter on the NPC's name finds its row.
        let f = s.detail(
            LootView::Items,
            "rsi_helmet_01",
            &DetailQuery::filter(Some("ace")),
        );
        assert_eq!(f.total, 1);
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn npc_bodies_are_rows_under_their_own_kind() {
        let s = npc_screen();
        let areas = s.list(LootView::Areas).rows;
        let npcs: Vec<&LootSummary> = areas.iter().filter(|r| is_npc(&r.id)).collect();
        let names: Vec<(&str, Vec<String>)> = npcs
            .iter()
            .map(|r| (r.name.as_str(), r.path.clone()))
            .collect();
        // No template; the pilot alone in its group sits right under the kind.
        assert_eq!(
            names,
            [
                ("Ace pilot", vec![NPC_KIND.to_string()]),
                (
                    "Soldier",
                    vec![NPC_KIND.to_string(), "Ground combat".to_string()]
                ),
                (
                    "Soldier (with council scrip)",
                    vec![NPC_KIND.to_string(), "Ground combat".to_string()]
                ),
            ]
        );
        assert!(npcs.iter().all(|r| r.group == NPC_KIND));
        assert_eq!(npcs[0].id, "npc:LootTable_Actor_AcePilot");
        // Places keep their rows.
        assert!(areas.iter().any(|r| r.id == "caves/loot_caves_occupied"));
    }

    #[test]
    fn npc_odds_follow_the_table_slots() {
        let s = npc_screen();
        // Pilot: the scrip slot is always filled; two scrips by tag share it evenly.
        let d = s.detail(
            LootView::Areas,
            "npc:LootTable_Actor_AcePilot",
            &DetailQuery::default(),
        );
        assert_eq!(d.group_by, LootGroupBy::Container);
        let scrip = d.group("Scrip").unwrap();
        assert_eq!(scrip.count, 2);
        assert!(close(scrip.best, 0.5));
        // The guns slot: weight 0.5 of 1.5, then the LMG entry 3 of 4.
        let guns = d.group("Arch guns").unwrap();
        let lmg = guns
            .rows
            .as_ref()
            .unwrap()
            .iter()
            .find(|r| r.id == "behr_lmg_01")
            .unwrap();
        assert!(close(lmg.odds, 1.0 / 3.0 * 0.75));
        // Soldier variant: the council scrip by class only, there 40% of the time whatever
        // the weights; ammo the only other slot (the priority one is not in its weights).
        let d = flat(
            &s,
            LootView::Areas,
            "npc:V3LootTable_Actor_Soldier_CouncilScrip",
            None,
        );
        let r = rows(&d);
        let sc = r.iter().find(|r| r.id == "scrip_council").unwrap();
        assert_eq!(sc.container, "Unique item");
        assert!(close(sc.odds, 0.4));
        assert!(r.iter().all(|r| r.id != "scrip_merc"));
        let lmg = r.iter().find(|r| r.id == "behr_lmg_01").unwrap();
        assert!(close(lmg.odds, 1.0));
        assert_eq!(lmg.container, "Ammo");
        // Base soldier: grouped by slot.
        let d = s.detail(
            LootView::Areas,
            "npc:V3LootTable_Actor_Soldier",
            &DetailQuery::default(),
        );
        let labels: Vec<&str> = d.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Ammo", "Sidearm"]);
        assert!(close(d.groups[1].best, 0.25));
    }

    #[test]
    fn items_list_the_npcs_that_carry_them() {
        let s = npc_screen();
        // The council scrip is in no area: it is listed for its NPC alone.
        let items = s.list(LootView::Items).rows;
        let scrip = items.iter().find(|r| r.id == "scrip_council").unwrap();
        assert_eq!((scrip.count, scrip.npcs), (2, 2));
        let lmg = items.iter().find(|r| r.id == "behr_lmg_01").unwrap();
        assert_eq!((lmg.count, lmg.npcs), (5, 3));
        let d = s.detail(LootView::Items, "scrip_council", &DetailQuery::default());
        let labels: Vec<&str> = d.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, [NPC_KIND]);
        let names: Vec<&str> = d.rows().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Ace pilot", "Soldier (with council scrip)"]);
        let r: Vec<&LootDrop> = d.rows().collect();
        assert_eq!(r[0].group, NPC_KIND);
        assert_eq!(r[1].group, format!("{NPC_KIND}{PATH_SEP}Ground combat"));
        assert_eq!(r[1].container, "Unique item");
        // The LMG: places and NPCs side by side, by place.
        let d = s.detail(LootView::Items, "behr_lmg_01", &DetailQuery::default());
        let labels: Vec<&str> = d.groups.iter().map(|g| g.label.as_str()).collect();
        assert!(
            labels.contains(&NPC_KIND) && labels.contains(&"Caves"),
            "{labels:?}"
        );
        // A filter word of the kind finds the NPC rows.
        let f = s.detail(
            LootView::Items,
            "behr_lmg_01",
            &DetailQuery::filter(Some("npcs")),
        );
        assert_eq!(f.total, 3);
    }

    #[test]
    fn missing_answers_are_empty() {
        assert_eq!(missing_list(LootView::Items).state, LootDataState::Missing);
        assert!(missing_detail(LootView::Areas).groups.is_empty());
    }
}
