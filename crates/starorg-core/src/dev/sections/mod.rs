//! Explorer sections and quick filters (addendum 7.17): a player-meaningful tree over every
//! DataCore record (Ships & vehicles, Ship weapons, …, Other), the Models and Sounds views,
//! and the filtered, sorted record list with facet counts. Pure: the dev worker fills
//! [`Facts`] from the records (and the archive's file names), the mock from synthetic ones.
//!
//! Every record lands in exactly one section ([`rules::section_of`], fallback "Other").
//! Node ids: `s:<json>` for a section and its groups (`["ships","NOITEM_Vehicle",null]`),
//! `m:models:<json>` / `m:sounds:<json>` for the media views (`[]`, then a section, then
//! its groups), `r:<guid>` for a record.

pub mod rules;
pub mod stats;

use super::explorer::{ExplorerIndex, MAX_PAGE, NodeKind, TreeNode, TreePage, relative_path};
use super::group::{node_id, node_path};
use rules::{Level, SECTIONS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// What the worker knows about one record beyond the index entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    pub item_type: Option<String>,
    pub sub_type: Option<String>,
    pub size: Option<i64>,
    /// Manufacturer code (`AEGS`).
    pub manufacturer: Option<String>,
    /// The record's display name, already read (stock text of its own name key).
    pub display: Option<String>,
    /// `Grade: C` of its description text.
    pub grade: Option<String>,
    /// `Class: Military` of its description text.
    pub class: Option<String>,
    /// A geometry file it names exists in the archive (or it is a vehicle with a definition).
    pub model: bool,
    /// One of its own audio triggers exists in the archive's ATL.
    pub sounds: bool,
}

/// `Grade` and `Class` lines of an item description (`Item Type: Cooler\nGrade: C\nClass:
/// Military\n\n...`; the stock text keeps `\n` as two characters).
pub fn desc_facts(text: &str) -> (Option<String>, Option<String>) {
    let mut grade = None;
    let mut class = None;
    for line in text.replace("\\n", "\n").lines().take(12) {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Grade:") {
            grade.get_or_insert(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Class:") {
            class.get_or_insert(v.trim().to_string());
        }
    }
    let ok = |v: Option<String>| v.filter(|v| !v.is_empty() && v.len() <= 24);
    (ok(grade), ok(class))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SortKey {
    /// Readable name (a search ranks first).
    #[default]
    Name,
    Size,
    Manufacturer,
}

/// How the list is grouped (headers with counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ListGroup {
    #[default]
    None,
    Section,
    Kind,
    Manufacturer,
    Size,
}

/// The filters (chips and facet sidebar). Within a facet the values are OR, across facets
/// AND. Values compare case-insensitively.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    pub sections: Vec<String>,
    /// Kind values (`AttachDef.Type`, or the struct type).
    pub kinds: Vec<String>,
    pub sub_types: Vec<String>,
    pub sizes: Vec<i64>,
    pub manufacturers: Vec<String>,
    pub grades: Vec<String>,
    pub classes: Vec<String>,
    /// DCB record tags.
    pub tags: Vec<String>,
    pub model: bool,
    pub sounds: bool,
    /// Hide AI, NPC, test and template variants.
    pub player_only: bool,
    /// `added` / `changed` against the compared channel (see [`SectionIndex::set_changes`]).
    pub changes: Vec<String>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListQuery {
    /// A section, group or media node; `""` = every record.
    pub node: String,
    /// Words (fuzzy) and field prefixes: `type:` `maker:` `size:` `tag:` `section:`
    /// `grade:` `class:` `has:model` `has:sounds` (see [`parse_query`]).
    pub query: String,
    pub filters: Filters,
    pub sort: SortKey,
    pub group: ListGroup,
    /// Group keys whose rows are left out (collapsed headers keep their counts).
    pub collapsed: Vec<String>,
    pub offset: u32,
    pub limit: u32,
    pub main_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub guid: String,
    /// The record name (`EntityClassDefinition.AEGS_Gladius`).
    pub name: String,
    /// What it reads as: its display name, else its short name.
    pub label: String,
    /// The short name when `label` is a display name.
    pub detail: Option<String>,
    #[serde(rename = "type")]
    pub struct_type: String,
    pub section: String,
    pub kind: Option<String>,
    /// Readable kind (`Coolers`), when the table has one.
    pub kind_label: Option<String>,
    pub sub_type: Option<String>,
    pub size: Option<i64>,
    pub manufacturer: Option<String>,
    /// `Aegis Dynamics`, when known.
    pub maker_label: Option<String>,
    pub grade: Option<String>,
    pub class: Option<String>,
    pub tag: Option<String>,
    pub model: bool,
    pub sounds: bool,
    pub variant: bool,
    pub main: bool,
    /// The group header this row sits under (with [`ListQuery::group`]).
    pub group: Option<String>,
    /// `added` / `changed` against the compared channel.
    pub change: Option<String>,
    /// Headline numbers from the build's GameData (filled by the app, 7.17).
    #[serde(default)]
    pub stats: Vec<Stat>,
}

/// One headline number of a list row (`DPS 412`, `SCM 220 m/s`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    pub key: String,
    pub label: String,
    pub value: f64,
    pub unit: Option<String>,
}

/// A record's state against the compared channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Change {
    Added,
    Changed,
}

impl Change {
    pub fn id(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Changed => "changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacetValue {
    pub value: String,
    pub label: String,
    pub count: u32,
}

/// Counts per filter value under the facet rule: each facet counts the records that pass
/// every *other* filter (and the search), so a value shows what picking it would give.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacetCounts {
    pub sections: Vec<FacetValue>,
    pub kinds: Vec<FacetValue>,
    pub sub_types: Vec<FacetValue>,
    pub sizes: Vec<FacetValue>,
    pub manufacturers: Vec<FacetValue>,
    pub grades: Vec<FacetValue>,
    pub classes: Vec<FacetValue>,
    pub tags: Vec<FacetValue>,
    pub model: u32,
    pub sounds: u32,
    pub player: u32,
    /// `added` / `changed` against the compared channel; empty while nothing is compared.
    pub changes: Vec<FacetValue>,
}

/// One group header of the list, in list order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupHead {
    pub key: String,
    pub label: String,
    pub count: u32,
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListPage {
    pub items: Vec<ListItem>,
    pub offset: u32,
    /// Rows shown (collapsed groups left out).
    pub total: u32,
    /// Records that match, collapsed groups included.
    pub matched: u32,
    pub facets: FacetCounts,
    pub groups: Vec<GroupHead>,
    /// The node's own label (`Ships & vehicles › Vehicles`), empty for every record.
    pub title: String,
    /// Field prefixes of the query that were understood (`size:3`), for the chips.
    pub understood: Vec<String>,
    /// Words of the query used for matching (lower case), for highlighting.
    pub words: Vec<String>,
}

/// A search query split into words and field filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    pub words: Vec<String>,
    /// Substring matches (lower case) on kind or struct type.
    pub types: Vec<String>,
    /// Substring matches on manufacturer code or name.
    pub makers: Vec<String>,
    /// Inclusive size ranges.
    pub sizes: Vec<(i64, i64)>,
    pub tags: Vec<String>,
    pub sections: Vec<String>,
    pub grades: Vec<String>,
    pub classes: Vec<String>,
    pub model: bool,
    pub sounds: bool,
    /// The prefixes taken, as typed.
    pub understood: Vec<String>,
}

/// Split a query: `type:cooler maker:aegs size:1-2 tag:systems section:ships grade:a
/// class:military has:model has:sounds` and words. Unknown prefixes stay words.
pub fn parse_query(q: &str) -> ParsedQuery {
    let mut p = ParsedQuery::default();
    for t in q.split_whitespace() {
        let Some((k, v)) = t.split_once(':') else {
            p.words.push(t.to_lowercase());
            continue;
        };
        let v = v.trim().to_lowercase();
        let k = k.to_ascii_lowercase();
        let took = match k.as_str() {
            _ if v.is_empty() => false,
            "type" | "kind" => {
                p.types.push(v);
                true
            }
            "maker" | "mfr" | "manufacturer" => {
                p.makers.push(v);
                true
            }
            "size" | "s" => {
                let r = match v.split_once('-') {
                    Some((a, b)) => a.parse().ok().zip(b.parse().ok()),
                    None => v.parse().ok().map(|n| (n, n)),
                };
                match r {
                    Some(r) => {
                        p.sizes.push(r);
                        true
                    }
                    None => false,
                }
            }
            "tag" => {
                p.tags.push(v);
                true
            }
            "section" | "in" => {
                p.sections.push(v);
                true
            }
            "grade" => {
                p.grades.push(v);
                true
            }
            "class" => {
                p.classes.push(v);
                true
            }
            "has" if v == "model" || v == "models" => {
                p.model = true;
                true
            }
            "has" if v == "sound" || v == "sounds" => {
                p.sounds = true;
                true
            }
            _ => false,
        };
        if took {
            p.understood.push(t.to_string());
        } else {
            p.words.push(t.to_lowercase());
        }
    }
    p
}

/// How the section index was built (7.17), reported in `status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionStats {
    /// Facet scan of the entities plus the index build, milliseconds.
    pub build_ms: u64,
    pub scan_ms: u64,
    /// Bytes held by the index, the owner map and the media name sets, roughly.
    pub bytes: u64,
    pub entities: u32,
    pub models: u32,
    pub sounds: u32,
    /// The archive's names were checked (false for a bare `--dcb`: then "has model" means a
    /// geometry file is named and "has sounds" an audio trigger is set).
    pub verified: bool,
}

/// Most values listed per facet (the rest are still filterable by search).
pub const MAX_FACET_VALUES: usize = 60;

const NONE: u32 = u32::MAX;
const NO_SIZE: u8 = u8::MAX;
const F_MODEL: u8 = 1;
const F_SOUNDS: u8 = 2;
const F_VARIANT: u8 = 4;

#[derive(Debug, Clone, Copy)]
struct Rec {
    section: u8,
    size: u8,
    flags: u8,
    kind: u32,
    sub: u32,
    maker: u32,
    folder: u32,
    grade: u32,
    class: u32,
    tag: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Media {
    Models,
    Sounds,
}

/// A parsed node id.
#[derive(Debug, Clone, PartialEq)]
struct Node {
    media: Option<Media>,
    section: Option<usize>,
    keys: Vec<Option<String>>,
}

/// The section index: one compact row per record (about 22 bytes) plus interned strings.
#[derive(Debug, Default, Clone)]
pub struct SectionIndex {
    recs: Vec<Rec>,
    strings: Vec<String>,
    display: HashMap<u32, String>,
    /// Manufacturer code -> readable name.
    makers: HashMap<String, String>,
    /// Records of each section, by readable name (lower case), then index.
    by_section: Vec<Vec<u32>>,
    /// Position of each record in readable-name order over all records.
    name_pos: Vec<u32>,
    /// Records added or changed against the compared channel (empty = none compared).
    changes: HashMap<u32, Change>,
}

struct Interner<'a> {
    strings: &'a mut Vec<String>,
    map: HashMap<String, u32>,
}

impl Interner<'_> {
    fn id(&mut self, s: Option<&str>) -> u32 {
        let Some(s) = s.filter(|s| !s.is_empty()) else {
            return NONE;
        };
        if let Some(&i) = self.map.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.map.insert(s.to_string(), i);
        i
    }
}

fn short_of(name: &str) -> &str {
    name.split_once('.').map_or(name, |(_, s)| s)
}

fn folder_of(rel: &str) -> String {
    let mut segs: Vec<&str> = rel.split('/').collect();
    segs.pop();
    segs.truncate(rules::FOLDER_DEPTH);
    segs.join("/")
}

impl SectionIndex {
    /// Build over `ix`; `facts[i]` belongs to entry `i` (missing = nothing known). `makers`
    /// reads a manufacturer code (`manufacturer_Name<CODE>` in the stock text).
    pub fn new(
        ix: &ExplorerIndex,
        facts: &[Option<Facts>],
        makers: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let mut strings = Vec::new();
        let mut display = HashMap::new();
        let mut maker_names = HashMap::new();
        let mut recs = Vec::with_capacity(ix.len());
        {
            let mut int = Interner {
                strings: &mut strings,
                map: HashMap::new(),
            };
            for (i, e) in ix.entries().iter().enumerate() {
                let f = facts.get(i).and_then(Option::as_ref);
                let entity = e.struct_type == "EntityClassDefinition";
                let rel = relative_path(&e.path).replace('\\', "/").to_lowercase();
                let item = f
                    .and_then(|f| f.item_type.as_deref())
                    .filter(|s| !s.is_empty());
                let section = rules::section_of(&e.struct_type, &rel, item, entity) as u8;
                let mut flags = 0;
                if entity && rules::is_variant(&e.name, &rel) {
                    flags |= F_VARIANT;
                }
                if f.is_some_and(|f| f.model) {
                    flags |= F_MODEL;
                }
                if f.is_some_and(|f| f.sounds) {
                    flags |= F_SOUNDS;
                }
                let maker = f.and_then(|f| f.manufacturer.as_deref());
                if let Some(code) = maker
                    && !maker_names.contains_key(code)
                {
                    let name = makers(code).unwrap_or_else(|| code.to_string());
                    maker_names.insert(code.to_string(), name);
                }
                if let Some(d) = f
                    .and_then(|f| f.display.as_deref())
                    .filter(|d| !d.is_empty())
                {
                    display.insert(i as u32, d.to_string());
                }
                recs.push(Rec {
                    section,
                    size: f
                        .and_then(|f| f.size)
                        .and_then(|s| u8::try_from(s).ok())
                        .filter(|&s| s != NO_SIZE)
                        .unwrap_or(NO_SIZE),
                    flags,
                    kind: int.id(Some(item.unwrap_or(&e.struct_type))),
                    sub: int.id(f.and_then(|f| f.sub_type.as_deref())),
                    maker: int.id(maker),
                    folder: int.id(Some(&folder_of(&rel))),
                    grade: int.id(f.and_then(|f| f.grade.as_deref())),
                    class: int.id(f.and_then(|f| f.class.as_deref())),
                    tag: int.id(e.tag.as_deref()),
                });
            }
        }
        let mut out = Self {
            recs,
            strings,
            display,
            makers: maker_names,
            by_section: vec![Vec::new(); SECTIONS.len()],
            name_pos: Vec::new(),
            changes: HashMap::new(),
        };
        let mut order: Vec<u32> = (0..out.recs.len() as u32).collect();
        order.sort_by_cached_key(|&i| (out.label(ix, i as usize).to_lowercase(), i));
        let mut by_section = std::mem::take(&mut out.by_section);
        out.name_pos = vec![0; out.recs.len()];
        for (pos, &i) in order.iter().enumerate() {
            out.name_pos[i as usize] = pos as u32;
            by_section[out.recs[i as usize].section as usize].push(i);
        }
        out.by_section = by_section;
        out
    }

    pub fn len(&self) -> usize {
        self.recs.len()
    }

    /// Mark records added or changed against another channel; replaces earlier marks.
    pub fn set_changes(&mut self, marks: impl IntoIterator<Item = (usize, Change)>) {
        self.changes = marks
            .into_iter()
            .filter(|(i, _)| *i < self.recs.len())
            .map(|(i, c)| (i as u32, c))
            .collect();
        self.changes.shrink_to_fit();
    }

    pub fn change(&self, i: usize) -> Option<Change> {
        self.changes.get(&(i as u32)).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.recs.is_empty()
    }

    /// Built over this index (an unbuilt index answers nothing).
    pub fn covers(&self, ix: &ExplorerIndex) -> bool {
        self.recs.len() == ix.len() && self.by_section.len() == SECTIONS.len()
    }

    /// Bytes held, roughly (for the worker's measurement).
    pub fn bytes(&self) -> usize {
        self.recs.capacity() * std::mem::size_of::<Rec>()
            + self
                .strings
                .iter()
                .map(|s| s.capacity() + 24)
                .sum::<usize>()
            + self
                .display
                .values()
                .map(|s| s.capacity() + 32)
                .sum::<usize>()
            + self
                .by_section
                .iter()
                .map(|v| v.capacity() * 4)
                .sum::<usize>()
            + self.name_pos.capacity() * 4
    }

    /// The section id of entry `i`.
    pub fn section_id(&self, i: usize) -> &'static str {
        self.recs
            .get(i)
            .map_or(rules::OTHER, |r| SECTIONS[r.section as usize].id)
    }

    /// Records per section (all records, main or not), in table order.
    pub fn counts(&self) -> Vec<(&'static str, u32)> {
        SECTIONS
            .iter()
            .zip(&self.by_section)
            .map(|(s, v)| (s.id, v.len() as u32))
            .collect()
    }

    pub fn has_model(&self, i: usize) -> bool {
        self.recs.get(i).is_some_and(|r| r.flags & F_MODEL != 0)
    }

    pub fn has_sounds(&self, i: usize) -> bool {
        self.recs.get(i).is_some_and(|r| r.flags & F_SOUNDS != 0)
    }

    pub fn is_variant(&self, i: usize) -> bool {
        self.recs.get(i).is_some_and(|r| r.flags & F_VARIANT != 0)
    }

    fn s(&self, id: u32) -> Option<&str> {
        (id != NONE).then(|| self.strings[id as usize].as_str())
    }

    /// What entry `i` reads as: display name, else short name.
    pub fn label<'a>(&'a self, ix: &'a ExplorerIndex, i: usize) -> &'a str {
        self.display
            .get(&(i as u32))
            .map(String::as_str)
            .unwrap_or_else(|| ix.entry(i).map_or("", |e| short_of(&e.name)))
    }

    pub fn display(&self, i: usize) -> Option<&str> {
        self.display.get(&(i as u32)).map(String::as_str)
    }

    pub fn maker_label(&self, code: &str) -> String {
        self.makers
            .get(code)
            .filter(|n| *n != code)
            .map_or_else(|| code.to_string(), |n| format!("{n} ({code})"))
    }

    fn key(&self, ix: &ExplorerIndex, i: usize, level: Level) -> Option<String> {
        let r = &self.recs[i];
        match level {
            Level::Kind => self.s(r.kind).map(str::to_string),
            Level::SubType => self
                .s(r.sub)
                .filter(|s| !s.eq_ignore_ascii_case("UNDEFINED"))
                .map(str::to_string),
            Level::Size => (r.size != NO_SIZE).then(|| r.size.to_string()),
            Level::Manufacturer => self.s(r.maker).map(str::to_string),
            Level::Type => ix.entry(i).map(|e| e.struct_type.clone()),
            Level::Folder => self.s(r.folder).map(str::to_string),
        }
    }

    fn key_label(&self, level: Level, key: Option<&str>) -> String {
        match (level, key) {
            (Level::Size, Some(k)) => format!("Size {k}"),
            (Level::Size, None) => "No size".into(),
            (Level::Manufacturer, Some(k)) => self.maker_label(k),
            (Level::Manufacturer, None) => "No manufacturer".into(),
            (Level::SubType, None) => "No sub-type".into(),
            (Level::Folder, Some(k)) => k.to_string(),
            (_, Some(k)) => rules::value_label(k).map_or_else(|| k.to_string(), str::to_string),
            (_, None) => "None".into(),
        }
    }

    fn parse(id: &str) -> Option<Node> {
        let (media, rest) = if let Some(r) = id.strip_prefix("m:models:") {
            (Some(Media::Models), r)
        } else if let Some(r) = id.strip_prefix("m:sounds:") {
            (Some(Media::Sounds), r)
        } else if let Some(r) = id.strip_prefix("s:") {
            (None, r)
        } else if id.is_empty() {
            return Some(Node {
                media: None,
                section: None,
                keys: vec![],
            });
        } else {
            return None;
        };
        let mut path = node_path(rest)?;
        if path.is_empty() {
            return media.map(|m| Node {
                media: Some(m),
                section: None,
                keys: vec![],
            });
        }
        let section = rules::section_index(path.remove(0)?.as_str())?;
        Some(Node {
            media,
            section: Some(section),
            keys: path,
        })
    }

    fn id_of(media: Option<Media>, section: Option<usize>, keys: &[Option<String>]) -> String {
        let mut path: Vec<Option<String>> = section
            .map(|s| Some(SECTIONS[s].id.to_string()))
            .into_iter()
            .collect();
        path.extend(keys.iter().cloned());
        let json = node_id(&path);
        match media {
            Some(Media::Models) => format!("m:models:{json}"),
            Some(Media::Sounds) => format!("m:sounds:{json}"),
            None => format!("s:{json}"),
        }
    }

    fn media_ok(&self, i: usize, m: Option<Media>) -> bool {
        match m {
            None => true,
            Some(Media::Models) => self.recs[i].flags & F_MODEL != 0,
            Some(Media::Sounds) => self.recs[i].flags & F_SOUNDS != 0,
        }
    }

    fn base_ok(&self, ix: &ExplorerIndex, i: usize, main_only: bool, player_only: bool) -> bool {
        (!main_only || ix.entry(i).is_some_and(|e| e.main))
            && (!player_only || self.recs[i].flags & F_VARIANT == 0)
    }

    /// Records under a node (any order is fine for counting; this is section name order).
    fn members<'s>(
        &'s self,
        ix: &'s ExplorerIndex,
        n: &'s Node,
    ) -> Box<dyn Iterator<Item = usize> + 's> {
        let sections: Vec<usize> = match n.section {
            Some(s) => vec![s],
            None => (0..SECTIONS.len()).collect(),
        };
        Box::new(
            sections
                .into_iter()
                .flat_map(move |s| self.by_section[s].iter().map(|&i| i as usize))
                .filter(move |&i| self.media_ok(i, n.media))
                .filter(move |&i| match n.section {
                    Some(s) => {
                        let levels = SECTIONS[s].levels;
                        n.keys
                            .iter()
                            .zip(levels)
                            .all(|(k, l)| self.key(ix, i, *l) == *k)
                    }
                    None => true,
                }),
        )
    }

    /// The node's readable title (`Ships & vehicles › Spaceships`).
    fn title(&self, n: &Node) -> String {
        let mut parts: Vec<String> = Vec::new();
        match n.media {
            Some(Media::Models) => parts.push("Models".into()),
            Some(Media::Sounds) => parts.push("Sounds".into()),
            None => {}
        }
        if let Some(s) = n.section {
            parts.push(SECTIONS[s].label.into());
            for (k, l) in n.keys.iter().zip(SECTIONS[s].levels) {
                if k.is_some() {
                    parts.push(self.key_label(*l, k.as_deref()));
                }
            }
        }
        parts.join(" › ")
    }

    /// Children of `node` in the Sections view: the Models and Sounds views and the sections
    /// at the root; groups below a section; records at the last level. Paged like
    /// [`ExplorerIndex::tree`]. Unknown ids give an empty page.
    pub fn tree(
        &self,
        ix: &ExplorerIndex,
        node: &str,
        offset: usize,
        limit: usize,
        main_only: bool,
        player_only: bool,
    ) -> TreePage {
        let all = self.children(ix, node, main_only, player_only);
        let total = all.len();
        TreePage {
            nodes: all
                .into_iter()
                .skip(offset)
                .take(limit.min(MAX_PAGE))
                .collect(),
            offset: offset as u32,
            total: total as u32,
        }
    }

    fn children(
        &self,
        ix: &ExplorerIndex,
        node: &str,
        main_only: bool,
        player_only: bool,
    ) -> Vec<TreeNode> {
        let Some(mut n) = Self::parse(node).filter(|_| self.covers(ix)) else {
            return vec![];
        };
        let ok = |i: usize| self.base_ok(ix, i, main_only, player_only);
        let group = |id: String, label: String, count: usize, kind: NodeKind| TreeNode {
            id,
            label,
            kind,
            count: count as u32,
            main: false,
        };
        if n.section.is_none() {
            let mut out = Vec::new();
            if n.media.is_none() {
                for (m, label) in [(Media::Models, "Models"), (Media::Sounds, "Sounds")] {
                    let c = (0..self.recs.len())
                        .filter(|&i| self.media_ok(i, Some(m)) && ok(i))
                        .count();
                    if c > 0 {
                        out.push(group(
                            Self::id_of(Some(m), None, &[]),
                            label.into(),
                            c,
                            NodeKind::Section,
                        ));
                    }
                }
            }
            for (s, def) in SECTIONS.iter().enumerate() {
                let c = self.by_section[s]
                    .iter()
                    .filter(|&&i| self.media_ok(i as usize, n.media) && ok(i as usize))
                    .count();
                if c > 0 {
                    out.push(group(
                        Self::id_of(n.media, Some(s), &[]),
                        def.label.into(),
                        c,
                        NodeKind::Section,
                    ));
                }
            }
            return out;
        }
        let s = n.section.expect("section");
        let levels = SECTIONS[s].levels;
        let members: Vec<usize> = self.members(ix, &n).filter(|&i| ok(i)).collect();
        // A level where no record has a value is skipped (its key is `null`).
        while n.keys.len() < levels.len()
            && members
                .iter()
                .all(|&i| self.key(ix, i, levels[n.keys.len()]).is_none())
        {
            n.keys.push(None);
        }
        if n.keys.len() >= levels.len() {
            return members
                .into_iter()
                .map(|i| {
                    let e = &ix.entries()[i];
                    TreeNode {
                        id: format!("r:{}", e.guid),
                        label: self.label(ix, i).to_string(),
                        kind: NodeKind::Record,
                        count: 0,
                        main: e.main,
                    }
                })
                .collect();
        }
        let level = levels[n.keys.len()];
        let mut groups: Vec<(Option<String>, usize)> = Vec::new();
        let mut at: HashMap<Option<String>, usize> = HashMap::new();
        for &i in &members {
            let k = self.key(ix, i, level);
            match at.get(&k) {
                Some(&g) => groups[g].1 += 1,
                None => {
                    at.insert(k.clone(), groups.len());
                    groups.push((k, 1));
                }
            }
        }
        let mut nodes: Vec<(Option<String>, String, usize)> = groups
            .into_iter()
            .map(|(k, c)| {
                let label = self.key_label(level, k.as_deref());
                (k, label, c)
            })
            .collect();
        nodes.sort_by(|a, b| match (level, &a.0, &b.0) {
            (_, None, Some(_)) => std::cmp::Ordering::Greater,
            (_, Some(_), None) => std::cmp::Ordering::Less,
            (Level::Size, Some(x), Some(y)) => x
                .parse::<i64>()
                .unwrap_or(0)
                .cmp(&y.parse::<i64>().unwrap_or(0)),
            _ => a.1.to_lowercase().cmp(&b.1.to_lowercase()),
        });
        nodes
            .into_iter()
            .map(|(k, label, c)| {
                let mut keys = n.keys.clone();
                keys.push(k);
                group(
                    Self::id_of(n.media, Some(s), &keys),
                    label,
                    c,
                    NodeKind::Folder,
                )
            })
            .collect()
    }

    fn kind_label(&self, k: &str) -> String {
        rules::value_label(k).map_or_else(|| k.to_string(), str::to_string)
    }

    /// How well entry `i` matches the words: `None` = no match, lower is better. Every word
    /// must appear in the readable name, short name, type, manufacturer or tag; failing
    /// that, the words run together must be an in-order subsequence of the readable or
    /// short name (fuzzy: `glds` finds Gladius).
    fn rank(&self, ix: &ExplorerIndex, i: usize, words: &[String], joined: &str) -> Option<u8> {
        if words.is_empty() {
            return Some(0);
        }
        let e = &ix.entries()[i];
        let r = &self.recs[i];
        let label = self.label(ix, i).to_lowercase();
        let short = short_of(&e.name).to_lowercase();
        let words_of = |t: &str| -> Vec<String> {
            t.split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect()
        };
        let single = !joined.contains(' ');
        // Readable name first, then the short name: exact, a whole word, a prefix, a word
        // prefix, anywhere.
        for (base, text) in [(0u8, &label), (5u8, &short)] {
            if *text == joined {
                return Some(base);
            }
            let w = words_of(text);
            if single && w.iter().any(|x| x == joined) {
                return Some(base + 1);
            }
            if text.starts_with(joined) {
                return Some(base + 2);
            }
            if w.iter().any(|x| x.starts_with(joined)) {
                return Some(base + 3);
            }
            if text.contains(joined) {
                return Some(base + 4);
            }
        }
        let mut other: Option<String> = None;
        let mut other_text = || {
            other
                .get_or_insert_with(|| {
                    let mut t = e.struct_type.to_lowercase();
                    for x in [
                        self.s(r.kind),
                        self.s(r.sub),
                        self.s(r.tag),
                        self.s(r.class),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        t.push(' ');
                        t.push_str(&x.to_lowercase());
                    }
                    if let Some(m) = self.s(r.maker) {
                        t.push(' ');
                        t.push_str(&self.maker_label(m).to_lowercase());
                    }
                    if let Some(k) = self.s(r.kind).and_then(rules::value_label) {
                        t.push(' ');
                        t.push_str(&k.to_lowercase());
                    }
                    t
                })
                .clone()
        };
        if words
            .iter()
            .all(|w| label.contains(w.as_str()) || short.contains(w.as_str()))
        {
            return Some(10);
        }
        let ot = other_text();
        if words.iter().all(|w| {
            label.contains(w.as_str()) || short.contains(w.as_str()) || ot.contains(w.as_str())
        }) {
            return Some(11);
        }
        let compact: String = words.concat();
        if compact.len() >= 3 && (subsequence(&compact, &label) || subsequence(&compact, &short)) {
            return Some(12);
        }
        None
    }

    fn group_key(&self, i: usize, g: ListGroup) -> Option<(String, String, u64)> {
        let r = &self.recs[i];
        match g {
            ListGroup::None => None,
            ListGroup::Section => {
                let s = r.section as usize;
                Some((SECTIONS[s].id.into(), SECTIONS[s].label.into(), s as u64))
            }
            ListGroup::Kind => Some(match self.s(r.kind) {
                Some(k) => (k.into(), self.kind_label(k), 0),
                None => ("".into(), "None".into(), 1),
            }),
            ListGroup::Manufacturer => Some(match self.s(r.maker) {
                Some(m) => (m.into(), self.maker_label(m), 0),
                None => ("".into(), "No manufacturer".into(), 1),
            }),
            ListGroup::Size => Some(if r.size == NO_SIZE {
                ("".into(), "No size".into(), u64::MAX)
            } else {
                (
                    r.size.to_string(),
                    format!("Size {}", r.size),
                    u64::from(r.size),
                )
            }),
        }
    }

    /// The filtered, sorted (and optionally grouped) record list with facet counts.
    pub fn list(&self, ix: &ExplorerIndex, q: &ListQuery) -> ListPage {
        let pq = parse_query(&q.query);
        let empty = |title: String| ListPage {
            items: vec![],
            offset: q.offset,
            total: 0,
            matched: 0,
            facets: FacetCounts::default(),
            groups: vec![],
            title,
            understood: pq.understood.clone(),
            words: pq.words.clone(),
        };
        let Some(n) = Self::parse(&q.node).filter(|_| self.covers(ix)) else {
            return empty(String::new());
        };
        let joined = pq.words.join(" ");
        let f = &q.filters;
        let low = |v: &[String]| v.iter().map(|x| x.to_lowercase()).collect::<Vec<_>>();
        let (f_sec, f_kind, f_sub, f_maker, f_grade, f_class, f_tag) = (
            low(&f.sections),
            low(&f.kinds),
            low(&f.sub_types),
            low(&f.manufacturers),
            low(&f.grades),
            low(&f.classes),
            low(&f.tags),
        );
        let any = |list: &[String], v: Option<&str>| {
            list.is_empty() || v.is_some_and(|v| list.iter().any(|x| x.eq_ignore_ascii_case(v)))
        };
        // Facet dimensions: 0 section, 1 kind, 2 sub-type, 3 size, 4 maker, 5 grade,
        // 6 class, 7 tag, 8 model, 9 sounds, 10 player, 11 change.
        const DIMS: usize = 12;
        let f_change = low(&f.changes);
        let mut change_counts: HashMap<Change, u32> = HashMap::new();
        let mut counts: Vec<HashMap<u32, u32>> = vec![HashMap::new(); 8];
        let mut flags = [0u32; 3];
        let mut hits: Vec<(u8, usize)> = Vec::new();
        for i in self.members(ix, &n) {
            let e = &ix.entries()[i];
            if q.main_only && !e.main {
                continue;
            }
            let r = &self.recs[i];
            // The query's own field prefixes are hard filters (not facets).
            let lk = self.s(r.kind).map(str::to_lowercase);
            if !pq.types.iter().all(|t| {
                lk.as_deref().is_some_and(|k| k.contains(t.as_str()))
                    || e.struct_type.to_lowercase().contains(t.as_str())
                    || self
                        .s(r.kind)
                        .and_then(rules::value_label)
                        .is_some_and(|l| l.to_lowercase().contains(t.as_str()))
            }) {
                continue;
            }
            if !pq.makers.is_empty() {
                let Some(m) = self.s(r.maker) else { continue };
                let ml = self.maker_label(m).to_lowercase();
                if !pq.makers.iter().all(|x| ml.contains(x.as_str())) {
                    continue;
                }
            }
            if !pq.sizes.is_empty()
                && !(r.size != NO_SIZE
                    && pq
                        .sizes
                        .iter()
                        .any(|(a, b)| (*a..=*b).contains(&i64::from(r.size))))
            {
                continue;
            }
            let has = |list: &[String], v: Option<&str>| {
                list.is_empty()
                    || v.is_some_and(|v| {
                        let v = v.to_lowercase();
                        list.iter().all(|x| v.contains(x.as_str()))
                    })
            };
            let sec = &SECTIONS[r.section as usize];
            if !pq.sections.is_empty()
                && !pq.sections.iter().any(|x| {
                    sec.id.starts_with(x.as_str()) || sec.label.to_lowercase().contains(x.as_str())
                })
            {
                continue;
            }
            if !has(&pq.tags, self.s(r.tag))
                || !has(&pq.grades, self.s(r.grade))
                || !has(&pq.classes, self.s(r.class))
                || (pq.model && r.flags & F_MODEL == 0)
                || (pq.sounds && r.flags & F_SOUNDS == 0)
            {
                continue;
            }
            let Some(rank) = self.rank(ix, i, &pq.words, &joined) else {
                continue;
            };
            let fails: [bool; DIMS] = [
                !any(&f_sec, Some(sec.id)),
                !any(&f_kind, self.s(r.kind)),
                !any(&f_sub, self.s(r.sub)),
                !f.sizes.is_empty() && !(r.size != NO_SIZE && f.sizes.contains(&i64::from(r.size))),
                !any(&f_maker, self.s(r.maker)),
                !any(&f_grade, self.s(r.grade)),
                !any(&f_class, self.s(r.class)),
                !any(&f_tag, self.s(r.tag)),
                f.model && r.flags & F_MODEL == 0,
                f.sounds && r.flags & F_SOUNDS == 0,
                f.player_only && r.flags & F_VARIANT != 0,
                !f_change.is_empty()
                    && !self
                        .change(i)
                        .is_some_and(|c| f_change.iter().any(|x| x == c.id())),
            ];
            let n_fail = fails.iter().filter(|x| **x).count();
            if n_fail > 1 {
                continue;
            }
            let ok = |own: usize| fails.iter().enumerate().all(|(k, x)| k == own || !x);
            let vals = [
                u32::from(r.section),
                r.kind,
                r.sub,
                u32::from(r.size),
                r.maker,
                r.grade,
                r.class,
                r.tag,
            ];
            for (d, v) in vals.iter().enumerate() {
                let none = if d == 3 { u32::from(NO_SIZE) } else { NONE };
                if ok(d) && *v != none {
                    *counts[d].entry(*v).or_default() += 1;
                }
            }
            for (k, bit) in [F_MODEL, F_SOUNDS].iter().enumerate() {
                if ok(8 + k) && r.flags & bit != 0 {
                    flags[k] += 1;
                }
            }
            if ok(10) && r.flags & F_VARIANT == 0 {
                flags[2] += 1;
            }
            if ok(11)
                && let Some(c) = self.change(i)
            {
                *change_counts.entry(c).or_default() += 1;
            }
            if n_fail == 0 {
                hits.push((rank, i));
            }
        }
        let size_of = |i: usize| {
            let s = self.recs[i].size;
            if s == NO_SIZE { i64::MAX } else { i64::from(s) }
        };
        let maker_of = |i: usize| {
            self.s(self.recs[i].maker).map_or_else(
                || "\u{10ffff}".to_string(),
                |m| self.maker_label(m).to_lowercase(),
            )
        };
        // Members come in readable-name order already; the sorts are stable.
        match q.sort {
            // A search ranks first, then player items over variants, named records over raw
            // ones, shorter names ("Aegis Gladius" before its paints).
            SortKey::Name if !pq.words.is_empty() => hits.sort_by_key(|h| {
                let variant = self.recs[h.1].flags & F_VARIANT != 0;
                let unnamed = !self.display.contains_key(&(h.1 as u32));
                (
                    h.0,
                    variant,
                    unnamed,
                    self.label(ix, h.1).len(),
                    self.name_pos[h.1],
                )
            }),
            SortKey::Name => hits.sort_by_key(|h| (h.0, self.name_pos[h.1])),
            SortKey::Size => hits.sort_by_key(|h| (size_of(h.1), h.0, self.name_pos[h.1])),
            SortKey::Manufacturer => {
                hits.sort_by_cached_key(|h| (maker_of(h.1), h.0, self.name_pos[h.1]))
            }
        }
        let mut groups: Vec<GroupHead> = Vec::new();
        let mut keyed: Vec<Option<String>> = Vec::new();
        if q.group != ListGroup::None {
            let mut order: HashMap<String, (u64, String, u32)> = HashMap::new();
            let keys: Vec<(u64, String, String)> = hits
                .iter()
                .map(|h| {
                    let (k, l, o) = self.group_key(h.1, q.group).expect("grouped");
                    (o, l, k)
                })
                .collect();
            for (o, l, k) in &keys {
                order
                    .entry(k.clone())
                    .and_modify(|g| g.2 += 1)
                    .or_insert((*o, l.clone(), 1));
            }
            // Stable: rows keep their sort order inside a group.
            let mut zipped: Vec<(GroupSortKey, (u8, usize))> = keys.into_iter().zip(hits).collect();
            zipped.sort_by_cached_key(|a| (a.0.0, a.0.1.to_lowercase(), a.0.2.clone()));
            let mut heads: Vec<(String, (u64, String, u32))> = order.into_iter().collect();
            heads.sort_by_cached_key(|a| (a.1.0, a.1.1.to_lowercase(), a.0.clone()));
            groups = heads
                .into_iter()
                .map(|(key, (_, label, count))| GroupHead {
                    collapsed: q.collapsed.contains(&key),
                    key,
                    label,
                    count,
                })
                .collect();
            hits = Vec::with_capacity(zipped.len());
            for ((_, _, k), h) in zipped {
                if !q.collapsed.contains(&k) {
                    keyed.push(Some(k));
                    hits.push(h);
                }
            }
        }
        let matched = groups
            .iter()
            .map(|g| g.count)
            .sum::<u32>()
            .max(hits.len() as u32);
        let facet = |d: usize, label: &dyn Fn(&str) -> String| {
            let mut v: Vec<FacetValue> = counts[d]
                .iter()
                .map(|(&k, &c)| {
                    let value = self.strings[k as usize].clone();
                    FacetValue {
                        label: label(&value),
                        value,
                        count: c,
                    }
                })
                .collect();
            v.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then(a.label.cmp(&b.label))
                    .then(a.value.cmp(&b.value))
            });
            v.truncate(MAX_FACET_VALUES);
            v
        };
        let mut sections: Vec<FacetValue> = counts[0]
            .iter()
            .map(|(&s, &c)| FacetValue {
                value: SECTIONS[s as usize].id.into(),
                label: SECTIONS[s as usize].label.into(),
                count: c,
            })
            .collect();
        sections.sort_by_key(|v| rules::section_index(&v.value));
        let mut sizes: Vec<FacetValue> = counts[3]
            .iter()
            .map(|(&s, &c)| FacetValue {
                value: s.to_string(),
                label: format!("S{s}"),
                count: c,
            })
            .collect();
        sizes.sort_by_key(|v| v.value.parse::<i64>().unwrap_or(0));
        let mut grades = facet(5, &|g| format!("Grade {g}"));
        grades.sort_by(|a, b| a.value.cmp(&b.value));
        let facets = FacetCounts {
            sections,
            kinds: facet(1, &|k| self.kind_label(k)),
            sub_types: facet(2, &|k| self.kind_label(k)),
            sizes,
            manufacturers: facet(4, &|c| self.maker_label(c)),
            grades,
            classes: facet(6, &|c| c.to_string()),
            tags: facet(7, &|t| t.to_string()),
            model: flags[0],
            sounds: flags[1],
            player: flags[2],
            changes: [Change::Added, Change::Changed]
                .into_iter()
                .filter(|_| !self.changes.is_empty())
                .map(|c| FacetValue {
                    value: c.id().into(),
                    label: if c == Change::Added {
                        "Added"
                    } else {
                        "Changed"
                    }
                    .into(),
                    count: change_counts.get(&c).copied().unwrap_or(0),
                })
                .collect(),
        };
        let total = hits.len();
        let items = hits
            .into_iter()
            .enumerate()
            .skip(q.offset as usize)
            .take((q.limit as usize).min(MAX_PAGE))
            .map(|(k, (_, i))| {
                let mut it = self.item(ix, i);
                it.group = keyed.get(k).cloned().flatten();
                it
            })
            .collect();
        ListPage {
            items,
            offset: q.offset,
            total: total as u32,
            matched,
            facets,
            groups,
            title: self.title(&n),
            understood: pq.understood,
            words: pq.words,
        }
    }

    /// One list row for entry `i`.
    pub fn item(&self, ix: &ExplorerIndex, i: usize) -> ListItem {
        let e = &ix.entries()[i];
        let r = &self.recs[i];
        let display = self.display(i);
        let kind = self.s(r.kind);
        let maker = self.s(r.maker);
        ListItem {
            guid: e.guid.clone(),
            name: e.name.clone(),
            label: self.label(ix, i).to_string(),
            detail: display.map(|_| short_of(&e.name).to_string()),
            struct_type: e.struct_type.clone(),
            section: SECTIONS[r.section as usize].id.to_string(),
            kind: kind.map(str::to_string),
            kind_label: kind.and_then(rules::value_label).map(str::to_string),
            sub_type: self
                .s(r.sub)
                .filter(|s| !s.eq_ignore_ascii_case("UNDEFINED"))
                .map(str::to_string),
            size: (r.size != NO_SIZE).then_some(i64::from(r.size)),
            manufacturer: maker.map(str::to_string),
            maker_label: maker
                .and_then(|m| self.makers.get(m))
                .filter(|n| Some(n.as_str()) != maker)
                .cloned(),
            grade: self.s(r.grade).map(str::to_string),
            class: self.s(r.class).map(str::to_string),
            tag: self.s(r.tag).map(str::to_string),
            model: r.flags & F_MODEL != 0,
            sounds: r.flags & F_SOUNDS != 0,
            variant: r.flags & F_VARIANT != 0,
            main: e.main,
            group: None,
            change: self.change(i).map(|c| c.id().to_string()),
            stats: Vec::new(),
        }
    }
}

/// A row's group: (order, label, key).
type GroupSortKey = (u64, String, String);

/// `needle` appears in `hay` in order.
fn subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

#[cfg(test)]
mod tests;
