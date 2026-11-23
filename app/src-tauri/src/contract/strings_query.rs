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
fn flat_id(by: GroupBy, r: &StringRow) -> Cow<'_, str> {
    match by {
        GroupBy::Manufacturer => Cow::Borrowed(r.manufacturer.as_deref().unwrap_or(NONE)),
        GroupBy::Size => r
            .size
            .map_or(Cow::Borrowed(NONE), |s| Cow::Owned(s.to_string())),
        GroupBy::Type | GroupBy::None => Cow::Borrowed(&r.group),
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
        || r.other_owners
            .as_deref()
            .is_some_and(|s| s.contains(needle))
}

/// Separates the fields of [`search_text`]; a needle holding it falls back to [`matches`].
const SEP: char = '\u{1f}';

/// Everything [`matches`] looks at, lower case, one string per row: worked out once per
/// table so a search does not lower-case about 91,000 rows again on every keystroke.
pub fn search_text(r: &StringRow) -> Box<str> {
    let mut out = String::with_capacity(r.key.len() + r.original.len() + r.result.len() + 8);
    let mut push = |s: &str| {
        out.push_str(&s.to_lowercase());
        out.push(SEP);
    };
    push(&r.key);
    push(&r.original);
    push(&r.result);
    if let Some(o) = r.owner.as_deref() {
        push(&o.name);
        if let Some(d) = &o.detail {
            push(d);
        }
    }
    if let Some(s) = r.other_owners.as_deref() {
        // Already lower case.
        out.push_str(s);
    }
    out.into()
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

/// `StringSource` in the order of its serde names (`edited`, `enhanced`, `patched`,
/// `unmodified`): the source sort and the source facet keep that order without a
/// serialisation per row.
fn source_rank(s: StringSource) -> usize {
    match s {
        StringSource::Edited => 0,
        StringSource::Enhanced => 1,
        StringSource::Patched => 2,
        StringSource::Unmodified => 3,
    }
}

const SOURCES_BY_NAME: [StringSource; 4] = [
    StringSource::Edited,
    StringSource::Enhanced,
    StringSource::Patched,
    StringSource::Unmodified,
];

/// A query worked out over the whole table: the matching rows in sort order and the facets.
/// Paging only slices `hits`, so the backend keeps one per query (minus offset and limit).
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    /// Row indices into the table, sorted.
    pub hits: Vec<u32>,
    pub sources: Vec<FacetCount>,
    pub categories: Vec<FacetCount>,
    pub groups: Vec<GroupNode>,
}

/// What a cached [`Prepared`] answers: the query without its page (`offset`, `limit`).
pub fn cache_key(req: &StringsQuery) -> String {
    let q = StringsQuery {
        offset: 0,
        limit: 0,
        ..req.clone()
    };
    serde_json::to_string(&q).unwrap_or_default()
}

/// Filter, count and sort `rows`. `search` is [`search_text`] of every row (same order),
/// when the caller keeps it; without it a search lower-cases each row.
pub fn prepare<R: std::borrow::Borrow<StringRow>>(
    rows: &[R],
    search: Option<&[Box<str>]>,
    req: &StringsQuery,
    meta: &GroupMeta,
) -> Prepared {
    let needle = req.search.to_lowercase();
    let search = search.filter(|s| s.len() == rows.len() && !needle.contains(SEP));
    let mut src_counts = [0u32; 4];
    let mut cat_counts: BTreeMap<&str, u32> = BTreeMap::new();
    let mut tree: HashMap<Cow<'_, str>, Tally> = HashMap::new();
    let mut sizes_seen: BTreeMap<u32, ()> = BTreeMap::new();
    let mut hits: Vec<u32> = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let r: &StringRow = r.borrow();
        if let Some(s) = r.size {
            sizes_seen.insert(s, ());
        }
        if req.changed_only && r.source == StringSource::Unmodified {
            continue;
        }
        if !needle.is_empty() {
            let hit = match search {
                Some(s) => s[i].contains(needle.as_str()),
                None => matches(r, &needle),
            };
            if !hit {
                continue;
            }
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
            src_counts[source_rank(r.source)] += 1;
        }
        if src_ok && groups_ok {
            *cat_counts.entry(r.category.as_str()).or_default() += 1;
        }
        if src_ok && cat_ok {
            let own_ok = match req.group_by {
                GroupBy::None => false,
                GroupBy::Type => mfr_ok && size_ok,
                GroupBy::Manufacturer => type_ok && size_ok,
                GroupBy::Size => type_ok && mfr_ok,
            };
            if own_ok {
                tree.entry(flat_id(req.group_by, r)).or_default().add(r);
            }
        }
        if src_ok && cat_ok && groups_ok {
            hits.push(i as u32);
        }
    }
    let row = |i: &u32| -> &StringRow { rows[*i as usize].borrow() };
    let pos = hierarchy_pos();
    let mfr_pos: HashMap<&str, usize> = meta
        .manufacturers
        .iter()
        .enumerate()
        .map(|(i, (c, _))| (c.as_str(), i))
        .collect();
    match req.sort {
        StringSort::Key => hits.sort_by(|a, b| row(a).key.cmp(&row(b).key)),
        StringSort::KeyDesc => hits.sort_by(|a, b| row(b).key.cmp(&row(a).key)),
        StringSort::Category => hits.sort_by(|a, b| {
            let (a, b) = (row(a), row(b));
            (&a.category, &a.key).cmp(&(&b.category, &b.key))
        }),
        StringSort::Source => hits.sort_by(|a, b| {
            let (a, b) = (row(a), row(b));
            (source_rank(a.source), &a.key).cmp(&(source_rank(b.source), &b.key))
        }),
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
            hits.sort_by(|a, b| {
                let (a, b) = (row(a), row(b));
                (rank(a), &a.key).cmp(&(rank(b), &b.key))
            });
        }
        StringSort::Owner => {
            // Key order first; the owner sort is stable, so it stays the tie-break without a
            // key copy per row.
            hits.sort_by(|a, b| row(a).key.cmp(&row(b).key));
            hits.sort_by_cached_key(|i| {
                let o = row(i).owner.as_deref();
                (
                    o.is_none(),
                    o.map(|o| o.name.to_lowercase()).unwrap_or_default(),
                    o.and_then(|o| o.field).map(|f| f as u8),
                )
            });
        }
    }
    let sources = SOURCES_BY_NAME
        .iter()
        .filter(|s| src_counts[source_rank(**s)] > 0)
        .map(|s| FacetCount {
            id: kebab(s),
            count: src_counts[source_rank(*s)],
        })
        .collect();
    let categories = cat_counts
        .into_iter()
        .map(|(id, count)| FacetCount {
            id: id.to_string(),
            count,
        })
        .collect();
    let node = |id: String, parent: Option<String>, label: String, t: Tally| GroupNode {
        id,
        parent,
        label,
        count: t.count,
        enhanced: t.enhanced,
        edited: t.edited,
    };
    let get = |id: &str| tree.get(id).copied().unwrap_or_default();
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
                    get(g.id)
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
            .map(|(c, l)| node(c.clone(), None, l.clone(), get(c)))
            .chain([node(NONE.into(), None, "No manufacturer".into(), get(NONE))])
            .collect(),
        GroupBy::Size => sizes_seen
            .keys()
            .map(|s| {
                let id = s.to_string();
                let t = get(&id);
                node(id, None, format!("Size {s}"), t)
            })
            .chain([node(NONE.into(), None, "No size".into(), get(NONE))])
            .collect(),
    };
    Prepared {
        hits,
        sources,
        categories,
        groups,
    }
}

/// One page of a [`Prepared`] query. Facets and the group tree come with the first page
/// only (`offset` 0): they do not depend on the page, and the screen keeps them for the
/// pages after it.
pub fn page<R: std::borrow::Borrow<StringRow>>(
    p: &Prepared,
    rows: &[R],
    all: u32,
    req: &StringsQuery,
    state: GroupState,
) -> StringsPage {
    let limit = req.limit.clamp(1, 1000) as usize;
    let first = req.offset == 0;
    let facets = |v: &Vec<FacetCount>| if first { v.clone() } else { Vec::new() };
    StringsPage {
        total: p.hits.len() as u32,
        all,
        offset: req.offset,
        rows: p
            .hits
            .iter()
            .skip(req.offset as usize)
            .take(limit)
            .map(|i| with_fallback(rows[*i as usize].borrow().clone()))
            .collect(),
        sources: facets(&p.sources),
        categories: facets(&p.categories),
        groups: if first { p.groups.clone() } else { Vec::new() },
        group_state: state,
    }
}

/// [`prepare`] and [`page`] in one go, for a backend without a query cache (the mock).
pub fn run<'a>(
    rows: impl Iterator<Item = Cow<'a, StringRow>>,
    all: u32,
    req: &StringsQuery,
    meta: &GroupMeta,
) -> StringsPage {
    let rows: Vec<Cow<'a, StringRow>> = rows.collect();
    let p = prepare(&rows, None, req, meta);
    page(&p, &rows, all, req, meta.state)
}

#[cfg(test)]
mod tests {
    //! The cached path ([`prepare`] + [`page`], with [`search_text`]) answers exactly as the
    //! single-pass query it replaced (kept below as `reference`), page by page.
    use super::*;
    use crate::contract::mock::{MOCK_MANUFACTURERS, synthetic_strings};

    fn meta() -> GroupMeta {
        GroupMeta {
            state: GroupState::Exact,
            manufacturers: MOCK_MANUFACTURERS
                .iter()
                .map(|(c, l)| (c.to_string(), l.to_string()))
                .collect(),
        }
    }

    fn q(search: &str) -> StringsQuery {
        StringsQuery {
            target: ChannelRef {
                install: "i".into(),
                channel: "LIVE".into(),
            },
            search: search.into(),
            sources: vec![],
            categories: vec![],
            changed_only: false,
            sort: StringSort::Key,
            offset: 0,
            limit: 37,
            groups: vec![],
            manufacturers: vec![],
            sizes: vec![],
            group_by: GroupBy::Type,
        }
    }

    #[test]
    fn cached_pages_match_the_single_pass_query() {
        // Every 23rd row of the mock table (about 4,000, every kind of row), so the
        // reference pass per page stays quick in a debug build.
        let rows: Vec<StringRow> = synthetic_strings().into_iter().step_by(23).collect();
        let search: Vec<Box<str>> = rows.iter().map(search_text).collect();
        let m = meta();
        let sorts = [
            StringSort::Key,
            StringSort::KeyDesc,
            StringSort::Category,
            StringSort::Source,
            StringSort::Group,
            StringSort::Owner,
        ];
        let bys = [
            GroupBy::None,
            GroupBy::Type,
            GroupBy::Manufacturer,
            GroupBy::Size,
        ];
        let mut queries = Vec::new();
        for sort in sorts {
            for by in bys {
                for search in ["", "cooler", "SYN_Laser", "repeater 10 turret", "\u{1f}x"] {
                    queries.push(StringsQuery {
                        sort,
                        group_by: by,
                        ..q(search)
                    });
                }
            }
        }
        queries.push(StringsQuery {
            changed_only: true,
            ..q("")
        });
        queries.push(StringsQuery {
            sources: vec![StringSource::Enhanced, StringSource::Unmodified],
            groups: vec!["components".into()],
            ..q("")
        });
        queries.push(StringsQuery {
            manufacturers: vec!["SYNA".into(), NONE.into()],
            sizes: vec![0, 2],
            group_by: GroupBy::Manufacturer,
            sort: StringSort::Group,
            ..q("")
        });
        for req in queries {
            let p = prepare(&rows, Some(&search), &req, &m);
            assert_eq!(p, prepare(&rows, None, &req, &m), "{req:?}");
            let total = p.hits.len() as u32;
            // First page, second page, a middle one, the last one and one past the end.
            let lim = req.limit;
            let mut offsets = vec![
                0,
                lim,
                (total / 2 / lim) * lim,
                total.saturating_sub(1),
                total,
            ];
            offsets.dedup();
            for offset in offsets {
                let req = StringsQuery {
                    offset,
                    ..req.clone()
                };
                let want = reference(rows.iter().map(Cow::Borrowed), rows.len() as u32, &req, &m);
                let got = page(&p, &rows, rows.len() as u32, &req, m.state);
                assert_eq!(
                    (got.total, got.all, got.offset),
                    (want.total, want.all, want.offset)
                );
                assert_eq!(got.rows, want.rows, "{req:?}");
                if offset == 0 {
                    assert_eq!(got.sources, want.sources, "{req:?}");
                    assert_eq!(got.categories, want.categories, "{req:?}");
                    assert_eq!(got.groups, want.groups, "{req:?}");
                } else {
                    assert!(got.sources.is_empty() && got.categories.is_empty());
                    assert!(got.groups.is_empty());
                }
            }
        }
    }

    #[test]
    fn source_rank_follows_the_serde_names() {
        let mut names: Vec<String> = SOURCES_BY_NAME.iter().map(kebab).collect();
        let sorted = {
            let mut s = names.clone();
            s.sort();
            s
        };
        assert_eq!(names, sorted);
        for (i, s) in SOURCES_BY_NAME.iter().enumerate() {
            assert_eq!(source_rank(*s), i);
        }
        names.dedup();
        assert_eq!(names.len(), 4);
    }

    /// Timing on the full mock table (91,000 rows): the single-pass query per page against a
    /// prepared query sliced per page. `cargo test --release -p starorg-codex-app --lib
    /// measure_strings_paging -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn measure_strings_paging() {
        let rows = synthetic_strings();
        let m = meta();
        let pages = 10u32;
        for (label, req) in [
            (
                "group sort",
                StringsQuery {
                    sort: StringSort::Group,
                    limit: 200,
                    ..q("")
                },
            ),
            (
                "source sort",
                StringsQuery {
                    sort: StringSort::Source,
                    limit: 200,
                    ..q("")
                },
            ),
            (
                "search",
                StringsQuery {
                    limit: 200,
                    ..q("syn")
                },
            ),
        ] {
            let page_req = |i: u32| StringsQuery {
                offset: i * 200,
                ..req.clone()
            };
            let t = std::time::Instant::now();
            for i in 0..pages {
                reference(
                    rows.iter().map(Cow::Borrowed),
                    rows.len() as u32,
                    &page_req(i),
                    &m,
                );
            }
            let old = t.elapsed() / pages;
            let t = std::time::Instant::now();
            let search: Vec<Box<str>> = rows.iter().map(search_text).collect();
            let search_build = t.elapsed();
            let t = std::time::Instant::now();
            let p = prepare(&rows, Some(&search), &req, &m);
            let first = t.elapsed();
            let t = std::time::Instant::now();
            for i in 1..pages {
                page(&p, &rows, rows.len() as u32, &page_req(i), m.state);
            }
            let later = t.elapsed() / (pages - 1);
            println!(
                "{label}: before {old:?} per page | after: first page {first:?}, later pages {later:?} (search text once {search_build:?})"
            );
        }
    }

    #[test]
    fn cache_key_ignores_the_page_only() {
        let a = q("x");
        let b = StringsQuery {
            offset: 400,
            limit: 200,
            ..a.clone()
        };
        assert_eq!(cache_key(&a), cache_key(&b));
        let c = StringsQuery {
            sort: StringSort::Owner,
            ..a.clone()
        };
        assert_ne!(cache_key(&a), cache_key(&c));
    }

    fn old_flat_id(by: GroupBy, r: &StringRow) -> String {
        match by {
            GroupBy::Manufacturer => r.manufacturer.clone().unwrap_or_else(|| NONE.into()),
            GroupBy::Size => r.size.map(|s| s.to_string()).unwrap_or_else(|| NONE.into()),
            GroupBy::Type | GroupBy::None => r.group.clone(),
        }
    }

    fn old_matches(r: &StringRow, needle: &str) -> bool {
        let has = |s: &str| s.to_lowercase().contains(needle);
        has(&r.key)
            || has(&r.original)
            || has(&r.result)
            || r.owner
                .as_deref()
                .is_some_and(|o| has(&o.name) || o.detail.as_deref().is_some_and(has))
            || r.other_owners
                .as_deref()
                .is_some_and(|s| s.contains(needle))
    }

    fn reference<'a>(
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
            if !needle.is_empty() && !old_matches(&r, &needle) {
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
                    tree.entry(old_flat_id(req.group_by, &r))
                        .or_default()
                        .add(&r);
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
}
