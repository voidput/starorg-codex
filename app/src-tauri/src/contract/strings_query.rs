//! The Strings query (search, facets, groups, sort, paging), shared by the real and the mock
//! backend so both answer the same way (addendum 8.4).
//!
//! Facet rule: each facet's counts reflect every filter except its own, so a chip or tree
//! node shows what picking it would give, and the tree does not change when a group is
//! selected.

use super::types::*;
use starorg_core::groups::rules::HIERARCHY;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

/// What the rows do not carry: the manufacturer names and whether groups are exact.
#[derive(Debug, Clone)]
pub struct GroupMeta {
    pub state: GroupState,
    /// `(code, label)`, sorted by label.
    pub manufacturers: Vec<(String, String)>,
}

/// Token for "no manufacturer" / "no size" in node ids and the manufacturer filter.
pub const NONE: &str = "none";

fn kebab<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn hierarchy_pos() -> HashMap<&'static str, usize> {
    HIERARCHY
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id, i))
        .collect()
}

#[derive(Default, Clone, Copy)]
struct Tally {
    count: u32,
    enhanced: u32,
    edited: u32,
}

impl Tally {
    fn add(&mut self, r: &StringRow) {
        self.count += 1;
        self.enhanced += u32::from(r.source == StringSource::Enhanced);
        self.edited += u32::from(r.source == StringSource::Edited);
    }
}

fn in_groups(groups: &[String], g: &str) -> bool {
    groups.is_empty()
        || groups.iter().any(|s| {
            g == s
                || (g.len() > s.len() && g.starts_with(s.as_str()) && g.as_bytes()[s.len()] == b'/')
        })
}

/// The node id of a row in a flat `group_by`.
fn flat_id(by: GroupBy, r: &StringRow) -> String {
    match by {
        GroupBy::Manufacturer => r.manufacturer.clone().unwrap_or_else(|| NONE.into()),
        GroupBy::Size => r.size.map(|s| s.to_string()).unwrap_or_else(|| NONE.into()),
        GroupBy::Type | GroupBy::None => r.group.clone(),
    }
}

/// Search: key, stock and result text, owner name and manufacturer. `needle` is lower case.
fn matches(r: &StringRow, needle: &str) -> bool {
    let has = |s: &str| s.to_lowercase().contains(needle);
    has(&r.key)
        || has(&r.original)
        || has(&r.result)
        || r.owner
            .as_deref()
            .is_some_and(|o| has(&o.name) || o.detail.as_deref().is_some_and(has))
}

/// A page row without an owner gets the readable key (`kind: key`).
pub fn with_fallback(mut r: StringRow) -> StringRow {
    if r.owner.is_none() {
        r.owner = Some(Box::new(KeyOwner {
            name: starorg_core::groups::owners::pretty_key(&r.key),
            field: None,
            kind: OwnerKind::Key,
            detail: None,
        }));
    }
    r
}

pub fn run<'a>(
    rows: impl Iterator<Item = Cow<'a, StringRow>>,
    all: u32,
    req: &StringsQuery,
    meta: &GroupMeta,
) -> StringsPage {
    let needle = req.search.to_lowercase();
    let mut src_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut cat_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut tree: HashMap<String, Tally> = HashMap::new();
    let mut sizes_seen: BTreeMap<u32, ()> = BTreeMap::new();
    let mut hits: Vec<Cow<'a, StringRow>> = Vec::new();
    for r in rows {
        if let Some(s) = r.size {
            sizes_seen.insert(s, ());
        }
        if req.changed_only && r.source == StringSource::Unmodified {
            continue;
        }
        if !needle.is_empty() && !matches(&r, &needle) {
            continue;
        }
        let src_ok = req.sources.is_empty() || req.sources.contains(&r.source);
        let cat_ok = req.categories.is_empty() || req.categories.contains(&r.category);
        let type_ok = in_groups(&req.groups, &r.group);
        let mfr_ok = req.manufacturers.is_empty()
            || req.manufacturers.iter().any(|m| {
                Some(m.as_str()) == r.manufacturer.as_deref()
                    || (m == NONE && r.manufacturer.is_none())
            });
        let size_ok = req.sizes.is_empty() || req.sizes.contains(&r.size.unwrap_or(0));
        let groups_ok = type_ok && mfr_ok && size_ok;
        if cat_ok && groups_ok {
            *src_counts.entry(kebab(&r.source)).or_default() += 1;
        }
        if src_ok && groups_ok {
            *cat_counts.entry(r.category.clone()).or_default() += 1;
        }
        if src_ok && cat_ok {
            let own_ok = match req.group_by {
                GroupBy::None => false,
                GroupBy::Type => mfr_ok && size_ok,
                GroupBy::Manufacturer => type_ok && size_ok,
                GroupBy::Size => type_ok && mfr_ok,
            };
            if own_ok {
                tree.entry(flat_id(req.group_by, &r)).or_default().add(&r);
            }
        }
        if src_ok && cat_ok && groups_ok {
            hits.push(r);
        }
    }
    let pos = hierarchy_pos();
    let mfr_pos: HashMap<&str, usize> = meta
        .manufacturers
        .iter()
        .enumerate()
        .map(|(i, (c, _))| (c.as_str(), i))
        .collect();
    match req.sort {
        StringSort::Key => hits.sort_by(|a, b| a.key.cmp(&b.key)),
        StringSort::KeyDesc => hits.sort_by(|a, b| b.key.cmp(&a.key)),
        StringSort::Category => {
            hits.sort_by(|a, b| (&a.category, &a.key).cmp(&(&b.category, &b.key)))
        }
        StringSort::Source => {
            hits.sort_by(|a, b| (kebab(&a.source), &a.key).cmp(&(kebab(&b.source), &b.key)))
        }
        StringSort::Group => {
            let rank = |r: &StringRow| match req.group_by {
                GroupBy::Manufacturer => r
                    .manufacturer
                    .as_deref()
                    .and_then(|m| mfr_pos.get(m).copied())
                    .unwrap_or(usize::MAX),
                GroupBy::Size => r.size.map(|s| s as usize).unwrap_or(usize::MAX),
                GroupBy::Type | GroupBy::None => {
                    pos.get(r.group.as_str()).copied().unwrap_or(usize::MAX)
                }
            };
            hits.sort_by(|a, b| (rank(a), &a.key).cmp(&(rank(b), &b.key)));
        }
        StringSort::Owner => {
            // Key order first; the owner sort is stable, so it stays the tie-break without a
            // key copy per row.
            hits.sort_by(|a, b| a.key.cmp(&b.key));
            hits.sort_by_cached_key(|r| {
                let o = r.owner.as_deref();
                (
                    o.is_none(),
                    o.map(|o| o.name.to_lowercase()).unwrap_or_default(),
                    o.and_then(|o| o.field).map(|f| f as u8),
                )
            });
        }
    }
    let total = hits.len() as u32;
    let limit = req.limit.clamp(1, 1000) as usize;
    let fc = |m: BTreeMap<String, u32>| {
        m.into_iter()
            .map(|(id, count)| FacetCount { id, count })
            .collect()
    };
    let node = |id: String, parent: Option<String>, label: String, t: Tally| GroupNode {
        id,
        parent,
        label,
        count: t.count,
        enhanced: t.enhanced,
        edited: t.edited,
    };
    let groups = match req.group_by {
        GroupBy::None => Vec::new(),
        GroupBy::Type => {
            let mut out: Vec<GroupNode> = Vec::with_capacity(HIERARCHY.len());
            for g in HIERARCHY {
                let t = if g.parent.is_none() && starorg_core::groups::has_children(g.id) {
                    HIERARCHY
                        .iter()
                        .filter(|c| c.parent == Some(g.id))
                        .filter_map(|c| tree.get(c.id))
                        .fold(Tally::default(), |a, b| Tally {
                            count: a.count + b.count,
                            enhanced: a.enhanced + b.enhanced,
                            edited: a.edited + b.edited,
                        })
                } else {
                    tree.get(g.id).copied().unwrap_or_default()
                };
                out.push(node(
                    g.id.into(),
                    g.parent.map(str::to_string),
                    g.label.into(),
                    t,
                ));
            }
            out
        }
        GroupBy::Manufacturer => meta
            .manufacturers
            .iter()
            .map(|(c, l)| {
                node(
                    c.clone(),
                    None,
                    l.clone(),
                    tree.get(c).copied().unwrap_or_default(),
                )
            })
            .chain([node(
                NONE.into(),
                None,
                "No manufacturer".into(),
                tree.get(NONE).copied().unwrap_or_default(),
            )])
            .collect(),
        GroupBy::Size => sizes_seen
            .keys()
            .map(|s| {
                let id = s.to_string();
                let t = tree.get(&id).copied().unwrap_or_default();
                node(id, None, format!("Size {s}"), t)
            })
            .chain([node(
                NONE.into(),
                None,
                "No size".into(),
                tree.get(NONE).copied().unwrap_or_default(),
            )])
            .collect(),
    };
    StringsPage {
        total,
        all,
        offset: req.offset,
        rows: hits
            .into_iter()
            .skip(req.offset as usize)
            .take(limit)
            .map(|r| with_fallback(r.into_owned()))
            .collect(),
        sources: fc(src_counts),
        categories: fc(cat_counts),
        groups,
        group_state: meta.state,
    }
}
