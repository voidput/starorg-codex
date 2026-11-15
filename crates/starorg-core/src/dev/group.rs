//! Group trees (addendum 7.15): the Datasets screen groups a table's rows by a chain of
//! columns (`type > subType > size`, `manufacturer`, the folders of a `path`), the Diff
//! screen groups the changed records by dataset, table and the same chains. Only the
//! visible nodes are sent (the roots and the children of open nodes), with exact counts;
//! rows are never sent with the tree.
//!
//! A node id is the JSON array of the keys from the root (`["WeaponGun","Gun","3"]`, `null`
//! for a missing value), so a row is in a node when its key path starts with the node's.
//! The structure comes from every row; counts follow the search and filters (facet rule), so
//! nodes do not jump while typing and zero-count nodes are dimmed rather than removed.

use super::dataset::{
    ColType, RowFilter, Table, TableKind, identity_column, row_matches, scalar_text,
};
use super::diff::{RecordState, TableDiff};
use super::readable::{self, Lookup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Most children listed under one node; the rest are counted in `more`.
pub const MAX_CHILDREN: usize = 500;
/// A column with more distinct values than this is not offered as a group.
pub const MAX_GROUP_VALUES: usize = 200;
/// Most presets offered for one table.
pub const MAX_PRESETS: usize = 12;

/// The chain tiers: the first groupable column of each tier, in order.
pub const CHAIN: [&[&str]; 3] = [
    &["type", "category", "kind", "classification", "itemType"],
    &["subType", "subCategory", "subKind", "subtype"],
    &["size"],
];

/// One level of a group-by: a column's value, or (`folders`, last level only) the folders of
/// a path in it (`entities/scitem/ships/weapons/x` -> `entities` > `scitem` > ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupLevel {
    pub column: String,
    #[serde(default)]
    pub folders: bool,
}

impl GroupLevel {
    pub fn col(c: &str) -> Self {
        Self {
            column: c.to_string(),
            folders: false,
        }
    }
}

/// A group-by offered for a table. The first is the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPreset {
    /// `chain`, `col:<column>` or `folders:<column>`; remembered per table by the app.
    pub id: String,
    pub label: String,
    pub levels: Vec<GroupLevel>,
}

/// Rows of one node: the group-by and the node id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPick {
    pub by: Vec<GroupLevel>,
    pub node: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffCounts {
    pub added: u32,
    pub removed: u32,
    pub changed: u32,
}

/// One visible node of a group tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataGroup {
    pub id: String,
    pub parent: Option<String>,
    pub label: String,
    /// 1 = top level.
    pub depth: u32,
    /// Rows (records) in the node that match the search and filters.
    pub count: u32,
    /// Child nodes (0 = a leaf).
    pub children: u32,
    /// Children not listed (more than [`MAX_CHILDREN`]); only for open nodes.
    pub more: u32,
    /// Diff trees: what the records in the node are.
    pub diff: Option<DiffCounts>,
}

/// The visible nodes, depth first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPage {
    pub nodes: Vec<DataGroup>,
    /// Rows matching the search and filters (the "All" count).
    pub total: u32,
    /// Rows in the table (records in the diff).
    pub all: u32,
}

/// How the Diff tree groups below dataset and table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffGroupBy {
    /// Datasets and tables only.
    Tables,
    /// Each table's default preset.
    Auto,
    /// Each table's path folders (tables without one: tables only).
    Folders,
}

/// What a value reads as (addendum 7.14 readable labels): `(column, value)`.
pub type LabelFn<'a> = &'a dyn Fn(&str, &str) -> Option<String>;

/// No readable labels.
pub fn no_labels(_: &str, _: &str) -> Option<String> {
    None
}

/// Node labels from a lookup: what a value reads as (a record's display name, a
/// manufacturer's name, stock text of a key).
pub fn lookup_label(l: &dyn Lookup, column: &str, value: &str) -> Option<String> {
    readable::label(l, value, Some(column)).map(|x| x.text)
}

// ---------------------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------------------

/// Distinct scalar values of a column, counted up to `MAX_GROUP_VALUES + 1`.
fn distinct(t: &Table, column: &str) -> (usize, u32) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut filled = 0u32;
    for r in &t.rows {
        if let Some(s) = r.get(column).and_then(scalar_text) {
            filled += 1;
            if seen.len() <= MAX_GROUP_VALUES {
                seen.insert(s);
            }
        }
    }
    (seen.len(), filled)
}

/// Whether a column groups rows: scalar, filled in at least half the rows, 2 to
/// [`MAX_GROUP_VALUES`] distinct values that repeat.
fn groupable(t: &Table, column: &str) -> bool {
    let Some(c) = t.column(column) else {
        return false;
    };
    if !c.kind.scalar() {
        return false;
    }
    let (n, filled) = distinct(t, column);
    (2..=MAX_GROUP_VALUES).contains(&n) && (n as u32) < filled && filled * 2 >= t.rows.len() as u32
}

/// Whether a text column holds paths with folders (at least half of its values have a `/`).
fn path_like(t: &Table, column: &str) -> bool {
    let lower = column.to_ascii_lowercase();
    let named = matches!(
        lower.as_str(),
        "path" | "file" | "filepath" | "filename" | "folder"
    ) || lower.ends_with("path");
    if !named || t.column(column).is_none_or(|c| c.kind != ColType::Text) {
        return false;
    }
    let (mut with, mut filled) = (0usize, 0usize);
    for r in &t.rows {
        if let Some(Value::String(s)) = r.get(column) {
            filled += 1;
            if s.contains(['/', '\\']) {
                with += 1;
            }
        }
    }
    filled > 0 && with * 2 >= filled
}

/// The group-bys offered for a table, the default first: the `type > subType > size` chain
/// (from [`CHAIN`]), `manufacturer`, path folders, then each other groupable column.
pub fn presets(t: &Table) -> Vec<GroupPreset> {
    if t.kind == TableKind::Tree || t.rows.len() < 2 {
        return vec![];
    }
    let mut out = Vec::new();
    let chain: Vec<(usize, &str)> = CHAIN
        .iter()
        .enumerate()
        .filter_map(|(tier, names)| names.iter().find(|c| groupable(t, c)).map(|c| (tier, *c)))
        .collect();
    if chain.len() >= 2 || chain.first().is_some_and(|c| c.0 == 0) {
        out.push(GroupPreset {
            id: "chain".into(),
            label: chain.iter().map(|c| c.1).collect::<Vec<_>>().join(" › "),
            levels: chain.iter().map(|c| GroupLevel::col(c.1)).collect(),
        });
    }
    let mut singles: Vec<&str> = Vec::new();
    for c in &t.columns {
        if c.name.to_ascii_lowercase().contains("manufacturer") && groupable(t, &c.name) {
            singles.push(&c.name);
        }
    }
    for c in &singles {
        out.push(GroupPreset {
            id: format!("col:{c}"),
            label: c.to_string(),
            levels: vec![GroupLevel::col(c)],
        });
    }
    for c in &t.columns {
        if path_like(t, &c.name) {
            out.push(GroupPreset {
                id: format!("folders:{}", c.name),
                label: format!("{} folders", c.name),
                levels: vec![GroupLevel {
                    column: c.name.clone(),
                    folders: true,
                }],
            });
        }
    }
    for c in &t.columns {
        if out.len() >= MAX_PRESETS {
            break;
        }
        if !singles.contains(&c.name.as_str()) && groupable(t, &c.name) {
            out.push(GroupPreset {
                id: format!("col:{}", c.name),
                label: c.name.clone(),
                levels: vec![GroupLevel::col(&c.name)],
            });
        }
    }
    out.truncate(MAX_PRESETS);
    out
}

// ---------------------------------------------------------------------------------------
// Key paths
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegKind {
    /// Dataset and table names of the diff tree: the key is the label.
    Fixed,
    Folder,
    Number,
    Text,
}

#[derive(Debug, Clone)]
struct Seg<'a> {
    key: Option<String>,
    column: &'a str,
    kind: SegKind,
}

/// The segments of a row: one per level (a folders level, last, gives one per folder; a
/// file at the root gives none). A level after a folders level is ignored.
fn segments<'a>(row: &Value, by: &'a [GroupLevel]) -> Vec<Seg<'a>> {
    let mut out = Vec::with_capacity(by.len());
    for l in by {
        let v = row.get(&l.column).filter(|v| !v.is_null());
        let text = v.and_then(scalar_text).filter(|s| !s.is_empty());
        if l.folders {
            match text {
                Some(s) => {
                    let mut parts: Vec<&str> =
                        s.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
                    parts.pop();
                    out.extend(parts.into_iter().map(|p| Seg {
                        key: Some(p.to_string()),
                        column: &l.column,
                        kind: SegKind::Folder,
                    }));
                }
                None => out.push(Seg {
                    key: None,
                    column: &l.column,
                    kind: SegKind::Folder,
                }),
            }
            break;
        }
        let kind = if v.is_some_and(|v| v.is_number() || v.is_boolean()) {
            SegKind::Number
        } else {
            SegKind::Text
        };
        out.push(Seg {
            key: text,
            column: &l.column,
            kind,
        });
    }
    out
}

/// The key path of a row under a group-by.
pub fn row_path(row: &Value, by: &[GroupLevel]) -> Vec<Option<String>> {
    segments(row, by).into_iter().map(|s| s.key).collect()
}

/// The key path of a node id; `None` when it is not one.
pub fn node_path(id: &str) -> Option<Vec<Option<String>>> {
    serde_json::from_str(id).ok()
}

/// A node id from its key path.
pub fn node_id(path: &[Option<String>]) -> String {
    serde_json::to_string(path).unwrap_or_default()
}

/// Whether a key path is in a node (starts with the node's path).
pub fn in_node(path: &[Option<String>], node: &[Option<String>]) -> bool {
    path.len() >= node.len() && path[..node.len()] == *node
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The label of a new node: "No size" for a missing value, "Size 3" for a number, the
/// readable text of a value when there is one, else the value.
fn seg_label(s: &Seg<'_>, label: LabelFn<'_>) -> String {
    match (&s.key, s.kind) {
        (None, _) => format!("No {}", s.column),
        (Some(k), SegKind::Fixed | SegKind::Folder) => k.clone(),
        (Some(k), SegKind::Number) => format!("{} {k}", cap(s.column)),
        (Some(k), SegKind::Text) => label(s.column, k).unwrap_or_else(|| k.clone()),
    }
}

/// A plain decimal number (`3`, `-1.5`); the same rule in the TS mock.
fn plain_number(s: &str) -> Option<f64> {
    let b = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = b.split_once('.').unwrap_or((b, "0"));
    let digits = |x: &str| !x.is_empty() && x.bytes().all(|c| c.is_ascii_digit());
    (digits(int) && digits(frac))
        .then(|| s.parse().ok())
        .flatten()
}

/// Sibling order in the tree, a total order (the TS mock uses the same rule): plain numbers
/// first, by value; then text by label (case-insensitive, then exact); a missing key last.
/// Equal numbers and equal labels fall back to the raw key, so `1` and `1.0` stay apart.
pub(crate) fn sibling_order(
    ka: Option<&str>,
    la: &str,
    kb: Option<&str>,
    lb: &str,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (p, q) = match (ka, kb) {
        (None, None) => return la.cmp(lb),
        (None, Some(_)) => return Ordering::Greater,
        (Some(_), None) => return Ordering::Less,
        (Some(p), Some(q)) => (p, q),
    };
    let text = || {
        la.to_lowercase()
            .cmp(&lb.to_lowercase())
            .then_with(|| la.cmp(lb))
            .then_with(|| p.cmp(q))
    };
    match (plain_number(p), plain_number(q)) {
        (Some(m), Some(n)) => m.total_cmp(&n).then_with(text),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => text(),
    }
}

// ---------------------------------------------------------------------------------------
// The tree
// ---------------------------------------------------------------------------------------

struct Node {
    /// This node's keys (more than one after folders are joined).
    keys: Vec<Option<String>>,
    label: String,
    kind: SegKind,
    kids: Vec<usize>,
    index: HashMap<Option<String>, usize>,
    count: u32,
    /// Rows whose path ends here (any match).
    ends: u32,
    diff: DiffCounts,
}

struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    fn new() -> Self {
        Tree {
            nodes: vec![Node {
                keys: vec![],
                label: String::new(),
                kind: SegKind::Fixed,
                kids: vec![],
                index: HashMap::new(),
                count: 0,
                ends: 0,
                diff: DiffCounts::default(),
            }],
        }
    }

    fn add(&mut self, segs: &[Seg<'_>], hit: bool, state: Option<RecordState>, label: LabelFn<'_>) {
        let mut at = 0usize;
        self.bump(at, hit, state);
        for s in segs {
            let next = match self.nodes[at].index.get(&s.key) {
                Some(&i) => i,
                None => {
                    let i = self.nodes.len();
                    self.nodes.push(Node {
                        keys: vec![s.key.clone()],
                        label: seg_label(s, label),
                        kind: s.kind,
                        kids: vec![],
                        index: HashMap::new(),
                        count: 0,
                        ends: 0,
                        diff: DiffCounts::default(),
                    });
                    self.nodes[at].index.insert(s.key.clone(), i);
                    self.nodes[at].kids.push(i);
                    i
                }
            };
            at = next;
            self.bump(at, hit, state);
        }
        self.nodes[at].ends += 1;
    }

    fn bump(&mut self, i: usize, hit: bool, state: Option<RecordState>) {
        let n = &mut self.nodes[i];
        if hit {
            n.count += 1;
        }
        match state {
            Some(RecordState::Added) => n.diff.added += 1,
            Some(RecordState::Removed) => n.diff.removed += 1,
            Some(RecordState::Changed) => n.diff.changed += 1,
            None => {}
        }
    }

    /// Join folder chains: a folder whose only child is a folder and that holds no row of its
    /// own becomes `a/b`. Then sort children with [`sibling_order`].
    fn finish(&mut self) {
        for i in 0..self.nodes.len() {
            loop {
                let n = &self.nodes[i];
                if n.kind != SegKind::Folder
                    || n.kids.len() != 1
                    || n.ends != 0
                    || n.keys[0].is_none()
                {
                    break;
                }
                let k = n.kids[0];
                if self.nodes[k].kind != SegKind::Folder || self.nodes[k].keys[0].is_none() {
                    break;
                }
                let kid = std::mem::replace(
                    &mut self.nodes[k],
                    Node {
                        keys: vec![],
                        label: String::new(),
                        kind: SegKind::Fixed,
                        kids: vec![],
                        index: HashMap::new(),
                        count: 0,
                        ends: 0,
                        diff: DiffCounts::default(),
                    },
                );
                let n = &mut self.nodes[i];
                n.keys.extend(kid.keys);
                n.label = format!("{}/{}", n.label, kid.label);
                n.kids = kid.kids;
                n.index = kid.index;
                n.ends = kid.ends;
            }
        }
        for i in 0..self.nodes.len() {
            let mut kids = std::mem::take(&mut self.nodes[i].kids);
            kids.sort_by(|&x, &y| {
                let (a, b) = (&self.nodes[x], &self.nodes[y]);
                let ka = a.keys.last().cloned().flatten();
                let kb = b.keys.last().cloned().flatten();
                sibling_order(ka.as_deref(), &a.label, kb.as_deref(), &b.label)
            });
            self.nodes[i].kids = kids;
        }
    }

    /// The visible nodes: the root's children (or, with `skip_single`, its only child's),
    /// and the children of every open node, depth first.
    fn emit(&self, open: &HashSet<&str>, skip_single: bool, diff: bool) -> Vec<DataGroup> {
        let mut out = Vec::new();
        let (start, prefix) = match self.nodes[0].kids.as_slice() {
            [only] if skip_single => (*only, self.nodes[*only].keys.clone()),
            _ => (0, vec![]),
        };
        self.walk(start, &prefix, None, 1, open, diff, &mut out);
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &self,
        i: usize,
        path: &[Option<String>],
        parent: Option<&str>,
        depth: u32,
        open: &HashSet<&str>,
        diff: bool,
        out: &mut Vec<DataGroup>,
    ) {
        let kids = &self.nodes[i].kids;
        for &k in kids.iter().take(MAX_CHILDREN) {
            let n = &self.nodes[k];
            let mut p = path.to_vec();
            p.extend(n.keys.iter().cloned());
            let id = node_id(&p);
            let is_open = open.contains(id.as_str());
            out.push(DataGroup {
                id: id.clone(),
                parent: parent.map(str::to_string),
                label: n.label.clone(),
                depth,
                count: n.count,
                children: n.kids.len() as u32,
                more: if is_open {
                    n.kids.len().saturating_sub(MAX_CHILDREN) as u32
                } else {
                    0
                },
                diff: diff.then_some(n.diff),
            });
            if is_open && !n.kids.is_empty() {
                self.walk(k, &p, Some(&id), depth + 1, open, diff, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Datasets
// ---------------------------------------------------------------------------------------

/// What the Datasets tree is asked for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupQuery {
    pub by: Vec<GroupLevel>,
    /// Node ids whose children are listed.
    #[serde(default)]
    pub open: Vec<String>,
    /// Columns searched (empty = the table's default columns), as for rows.
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub filters: Vec<RowFilter>,
}

/// The visible nodes of a table's group tree. With a lookup, the search also matches what
/// cells read as (as the rows do) and node labels read the values.
pub fn group_rows(t: &Table, q: &GroupQuery, l: Option<&dyn Lookup>) -> GroupPage {
    let named = |c: &str, v: &str| l.and_then(|l| lookup_label(l, c, v));
    let label: LabelFn<'_> = &named;
    let identity = identity_column(&t.rows, None);
    let columns = if q.columns.is_empty() {
        super::dataset::default_columns(&t.columns)
    } else {
        q.columns.clone()
    };
    let needle = q.search.trim().to_lowercase();
    let mut tree = Tree::new();
    let mut total = 0u32;
    if !q.by.is_empty() {
        for r in &t.rows {
            let hit = row_matches(r, &columns, &needle, &q.filters, l, identity.as_deref());
            total += u32::from(hit);
            tree.add(&segments(r, &q.by), hit, None, label);
        }
    } else {
        total = t
            .rows
            .iter()
            .filter(|r| row_matches(r, &columns, &needle, &q.filters, l, identity.as_deref()))
            .count() as u32;
    }
    tree.finish();
    let open: HashSet<&str> = q.open.iter().map(String::as_str).collect();
    GroupPage {
        nodes: tree.emit(&open, false, false),
        total,
        all: t.rows.len() as u32,
    }
}

/// Whether a row is in the picked node (always, when the id is not a node id).
pub fn row_in(row: &Value, pick: &GroupPick) -> bool {
    match node_path(&pick.node) {
        Some(node) => in_node(&row_path(row, &pick.by), &node),
        None => true,
    }
}

// ---------------------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------------------

/// The levels below dataset and table for one compared table (B's columns, else A's).
pub fn diff_levels(by: DiffGroupBy, a: Option<&Table>, b: Option<&Table>) -> Vec<GroupLevel> {
    let Some(t) = b.filter(|t| !t.rows.is_empty()).or(a) else {
        return vec![];
    };
    match by {
        DiffGroupBy::Tables => vec![],
        DiffGroupBy::Auto => presets(t)
            .into_iter()
            .next()
            .map(|p| p.levels)
            .unwrap_or_default(),
        DiffGroupBy::Folders => presets(t)
            .into_iter()
            .find(|p| p.id.starts_with("folders:"))
            .map(|p| p.levels)
            .unwrap_or_default(),
    }
}

/// The row a diff record is grouped by: B's, else A's (a removed record).
pub fn diff_row<'t>(
    r: &super::diff::DiffRecord,
    a: Option<&'t Table>,
    b: Option<&'t Table>,
) -> Option<&'t Value> {
    r.b.and_then(|i| b?.rows.get(i as usize))
        .or_else(|| r.a.and_then(|i| a?.rows.get(i as usize)))
}

/// One compared table with its two sides.
pub struct DiffSide<'a> {
    pub diff: &'a TableDiff,
    pub a: Option<&'a Table>,
    pub b: Option<&'a Table>,
}

/// The visible nodes of the Diff tree: dataset (left out when there is only one), table,
/// then the table's levels. Only records that differ count; tables without any are left out.
pub fn diff_groups(
    tables: &[DiffSide<'_>],
    by: DiffGroupBy,
    open: &[String],
    label: LabelFn<'_>,
) -> GroupPage {
    let mut tree = Tree::new();
    let mut all = 0u32;
    for s in tables {
        if s.diff.records.is_empty() {
            continue;
        }
        let levels = diff_levels(by, s.a, s.b);
        let head = [
            Seg {
                key: Some(s.diff.summary.dataset.clone()),
                column: "dataset",
                kind: SegKind::Fixed,
            },
            Seg {
                key: Some(s.diff.summary.table.clone()),
                column: "table",
                kind: SegKind::Fixed,
            },
        ];
        for r in &s.diff.records {
            all += 1;
            let mut segs = head.to_vec();
            if let Some(row) = diff_row(r, s.a, s.b) {
                segs.extend(segments(row, &levels));
            }
            tree.add(&segs, true, Some(r.state), label);
        }
    }
    tree.finish();
    let open: HashSet<&str> = open.iter().map(String::as_str).collect();
    GroupPage {
        nodes: tree.emit(&open, true, true),
        total: all,
        all,
    }
}

/// A Diff tree node: its dataset, table (when the node is that deep) and the keys below.
pub fn diff_node(id: &str) -> Option<(String, Option<String>, Vec<Option<String>>)> {
    let p = node_path(id)?;
    let mut it = p.into_iter();
    let dataset = it.next()??;
    let table = it.next().flatten();
    Some((dataset, table, it.collect()))
}

/// Whether a diff record is in the keys below its table's node.
pub fn record_in(
    r: &super::diff::DiffRecord,
    a: Option<&Table>,
    b: Option<&Table>,
    levels: &[GroupLevel],
    rest: &[Option<String>],
) -> bool {
    if rest.is_empty() {
        return true;
    }
    diff_row(r, a, b).is_some_and(|row| in_node(&row_path(row, levels), rest))
}
