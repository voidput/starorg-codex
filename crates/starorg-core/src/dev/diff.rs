//! Dataset diff (addendum 7.6): per table added / removed / changed / same, records matched
//! by an identity column (or by position), field changes as JSON-pointer paths. Envelope
//! metadata (`_extracted`, `_fetched`, `_source`) is never a table, so it never counts.

use super::dataset::{Dataset, IDENTITY, Table, identity_column, identity_of};
use super::readable::{self, Lookup, Readable};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Presence {
    Both,
    OnlyA,
    OnlyB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordState {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffTable {
    pub dataset: String,
    pub table: String,
    pub presence: Presence,
    pub added: u32,
    pub removed: u32,
    pub changed: u32,
    pub same: u32,
    /// Column records were matched by; `None` = by position.
    pub identity: Option<String>,
}

impl DiffTable {
    pub fn weight(&self) -> u32 {
        self.added + self.removed + self.changed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRecord {
    /// Identity value, or `#<index>` by position.
    pub id: String,
    pub state: RecordState,
    /// Changed paths (changed records only).
    pub fields: Vec<String>,
    /// Row positions on each side.
    pub a: Option<u32>,
    pub b: Option<u32>,
    /// The record's display name (addendum 7.14), filled per page.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    pub path: String,
    pub old: Option<Value>,
    pub new: Option<Value>,
    pub delta: Option<f64>,
    pub percent: Option<f64>,
    /// The path with its `[column=value]` segments read (addendum 7.14).
    #[serde(default)]
    pub path_label: Option<String>,
    /// What the change is shown under (see [`readable::group_of`]).
    #[serde(default)]
    pub group: Option<String>,
    /// Unit of a numeric field, when its name says it.
    #[serde(default)]
    pub unit: Option<String>,
    /// What the old value reads as (side A's lookup) and the new one (side B's).
    #[serde(default)]
    pub old_label: Option<Readable>,
    #[serde(default)]
    pub new_label: Option<Readable>,
}

/// One table's result: the summary row and the records that are not the same, changed first
/// (most changed fields first), then added, then removed.
#[derive(Debug, Clone, PartialEq)]
pub struct TableDiff {
    pub summary: DiffTable,
    pub records: Vec<DiffRecord>,
}

fn esc(seg: &str) -> String {
    seg.replace('~', "~0").replace('/', "~1")
}

/// Identity among the elements of an array of objects (same rule as tables).
fn array_identity(a: &[Value]) -> Option<String> {
    if a.is_empty() || !a.iter().all(Value::is_object) {
        return None;
    }
    identity_column(a, None)
}

/// Scalars (and empty containers) by path. Arrays of objects with a unique identity use
/// `[column=value]` segments, so a reordered list is not a change; others use the index.
pub fn flatten(v: &Value) -> BTreeMap<String, Value> {
    fn go(v: &Value, path: &mut String, out: &mut BTreeMap<String, Value>) {
        let len = path.len();
        match v {
            Value::Object(m) if !m.is_empty() => {
                for (k, x) in m {
                    path.push('/');
                    path.push_str(&esc(k));
                    go(x, path, out);
                    path.truncate(len);
                }
            }
            Value::Array(a) if !a.is_empty() => {
                let id = array_identity(a);
                for (i, x) in a.iter().enumerate() {
                    path.push('/');
                    match id
                        .as_deref()
                        .and_then(|c| identity_of(x, c).map(|v| (c, v)))
                    {
                        Some((c, val)) => path.push_str(&format!("[{c}={}]", esc(&val))),
                        None => path.push_str(&i.to_string()),
                    }
                    go(x, path, out);
                    path.truncate(len);
                }
            }
            other => {
                out.insert(path.clone(), other.clone());
            }
        }
    }
    let mut out = BTreeMap::new();
    go(v, &mut String::new(), &mut out);
    out
}

/// Field changes between two values, path order. Numbers get the delta and the percent of
/// the old value.
pub fn field_changes(a: &Value, b: &Value) -> Vec<FieldChange> {
    let fa = flatten(a);
    let fb = flatten(b);
    let mut keys: Vec<&String> = fa.keys().chain(fb.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|k| {
            let (x, y) = (fa.get(k), fb.get(k));
            if x == y {
                return None;
            }
            let (delta, percent) = match (x.and_then(Value::as_f64), y.and_then(Value::as_f64)) {
                (Some(p), Some(q)) => (Some(q - p), (p != 0.0).then(|| (q - p) / p.abs() * 100.0)),
                _ => (None, None),
            };
            let numeric = x.or(y).is_some_and(Value::is_number);
            Some(FieldChange {
                path: k.clone(),
                old: x.cloned(),
                new: y.cloned(),
                delta,
                percent,
                path_label: None,
                group: None,
                unit: numeric
                    .then(|| readable::unit_of(k))
                    .flatten()
                    .map(str::to_string),
                old_label: None,
                new_label: None,
            })
        })
        .collect()
}

/// Fill the readable parts of field changes: path labels and groups (B's lookup, else A's),
/// and what old and new values read as on their own side.
pub fn annotate_changes(changes: &mut [FieldChange], a: &dyn Lookup, b: &dyn Lookup) {
    let key_of = |p: &str| -> String {
        let last = p.rsplit('/').next().unwrap_or(p);
        match last.strip_prefix('[').and_then(|x| x.split_once('=')) {
            Some((c, _)) => c.to_string(),
            None => last.to_string(),
        }
    };
    let read = |l: &dyn Lookup, v: &Option<Value>, key: &str| -> Option<Readable> {
        v.as_ref()
            .and_then(Value::as_str)
            .and_then(|s| readable::label(l, s, Some(key)))
    };
    for c in changes {
        c.path_label =
            readable::path_label(b, &c.path).or_else(|| readable::path_label(a, &c.path));
        c.group = readable::group_of(b, &c.path);
        let key = key_of(&c.path);
        c.old_label = read(a, &c.old, &key);
        c.new_label = read(b, &c.new, &key);
    }
}

/// Name the positional list elements in change paths (`/loadout/weapons/3/dps` reads
/// `loadout › weapons › Draugar Gatling › dps`) from the rows themselves: B's element (B's
/// lookup), else A's. Changes whose path has no positional segment keep their label.
pub fn name_positions(
    changes: &mut [FieldChange],
    a: &Value,
    b: &Value,
    la: &dyn Lookup,
    lb: &dyn Lookup,
) {
    for c in changes {
        let positional = c
            .path
            .split('/')
            .any(|s| !s.is_empty() && s.bytes().all(|x| x.is_ascii_digit()));
        if !positional {
            continue;
        }
        let named = if c.new.is_some() {
            readable::path_label_in(lb, &c.path, b)
        } else {
            None
        }
        .or_else(|| readable::path_label_in(la, &c.path, a));
        if named.is_some() {
            c.path_label = named;
        }
    }
}

/// Paths that differ (no values).
fn changed_paths(a: &Value, b: &Value) -> Vec<String> {
    field_changes(a, b).into_iter().map(|c| c.path).collect()
}

/// Pick the identity column for two versions of a table: the override when it is unique on
/// both sides, else the first of [`IDENTITY`] that is.
pub fn pick_identity(a: &[Value], b: &[Value], over: Option<&str>) -> Option<String> {
    if let Some(c) = over {
        let ok = |rows: &[Value]| {
            let mut seen = std::collections::HashSet::new();
            rows.iter()
                .all(|r| identity_of(r, c).is_some_and(|k| seen.insert(k)))
        };
        if (!a.is_empty() || !b.is_empty()) && ok(a) && ok(b) {
            return Some(c.to_string());
        }
        return None;
    }
    let ok = |rows: &[Value], c: &str| {
        let mut seen = std::collections::HashSet::new();
        rows.iter()
            .all(|r| identity_of(r, c).is_some_and(|k| seen.insert(k)))
    };
    if a.is_empty() && b.is_empty() {
        return None;
    }
    IDENTITY
        .iter()
        .find(|c| ok(a, c) && ok(b, c))
        .map(|c| c.to_string())
}

/// Compare one table present on either side.
pub fn diff_table(
    dataset: &str,
    name: &str,
    a: Option<&Table>,
    b: Option<&Table>,
    identity_override: Option<&str>,
) -> TableDiff {
    let empty: Vec<Value> = Vec::new();
    let ra = a.map_or(&empty, |t| &t.rows);
    let rb = b.map_or(&empty, |t| &t.rows);
    let presence = match (a, b) {
        (Some(_), Some(_)) => Presence::Both,
        (Some(_), None) => Presence::OnlyA,
        _ => Presence::OnlyB,
    };
    let identity = pick_identity(ra, rb, identity_override);
    let mut records = Vec::new();
    let mut same = 0u32;
    match &identity {
        Some(col) => {
            let ib: HashMap<String, usize> = rb
                .iter()
                .enumerate()
                .filter_map(|(i, r)| identity_of(r, col).map(|k| (k, i)))
                .collect();
            let mut seen_b = vec![false; rb.len()];
            for (i, r) in ra.iter().enumerate() {
                let id = identity_of(r, col).unwrap_or_default();
                match ib.get(&id) {
                    Some(&j) => {
                        seen_b[j] = true;
                        if r == &rb[j] {
                            same += 1;
                        } else {
                            let fields = changed_paths(r, &rb[j]);
                            if fields.is_empty() {
                                same += 1;
                            } else {
                                records.push(DiffRecord {
                                    id,
                                    state: RecordState::Changed,
                                    fields,
                                    a: Some(i as u32),
                                    b: Some(j as u32),
                                    label: None,
                                });
                            }
                        }
                    }
                    None => records.push(DiffRecord {
                        id,
                        state: RecordState::Removed,
                        fields: vec![],
                        a: Some(i as u32),
                        b: None,
                        label: None,
                    }),
                }
            }
            for (j, r) in rb.iter().enumerate() {
                if !seen_b[j] {
                    records.push(DiffRecord {
                        id: identity_of(r, col).unwrap_or_default(),
                        state: RecordState::Added,
                        fields: vec![],
                        a: None,
                        b: Some(j as u32),
                        label: None,
                    });
                }
            }
        }
        None => {
            for i in 0..ra.len().max(rb.len()) {
                let id = format!("#{i}");
                match (ra.get(i), rb.get(i)) {
                    (Some(x), Some(y)) if x == y => same += 1,
                    (Some(x), Some(y)) => records.push(DiffRecord {
                        id,
                        state: RecordState::Changed,
                        fields: changed_paths(x, y),
                        a: Some(i as u32),
                        b: Some(i as u32),
                        label: None,
                    }),
                    (Some(_), None) => records.push(DiffRecord {
                        id,
                        state: RecordState::Removed,
                        fields: vec![],
                        a: Some(i as u32),
                        b: None,
                        label: None,
                    }),
                    (None, Some(_)) => records.push(DiffRecord {
                        id,
                        state: RecordState::Added,
                        fields: vec![],
                        a: None,
                        b: Some(i as u32),
                        label: None,
                    }),
                    (None, None) => {}
                }
            }
        }
    }
    let rank = |s: RecordState| match s {
        RecordState::Changed => 0,
        RecordState::Added => 1,
        RecordState::Removed => 2,
    };
    records.sort_by(|x, y| {
        rank(x.state)
            .cmp(&rank(y.state))
            .then(y.fields.len().cmp(&x.fields.len()))
            .then(x.id.cmp(&y.id))
    });
    let count = |s| records.iter().filter(|r| r.state == s).count() as u32;
    TableDiff {
        summary: DiffTable {
            dataset: dataset.to_string(),
            table: name.to_string(),
            presence,
            added: count(RecordState::Added),
            removed: count(RecordState::Removed),
            changed: count(RecordState::Changed),
            same,
            identity,
        },
        records,
    }
}

/// Compare two sources: datasets matched by file name, every table of either side. Sorted
/// by changed + added + removed, largest first. `progress(done, total)` per dataset.
pub fn diff_sources(
    a: &[(String, &Dataset)],
    b: &[(String, &Dataset)],
    progress: &dyn Fn(usize, usize),
) -> Vec<TableDiff> {
    let mut names: Vec<&String> = a
        .iter()
        .map(|x| &x.0)
        .chain(b.iter().map(|x| &x.0))
        .collect();
    names.sort();
    names.dedup();
    fn find<'a>(side: &[(String, &'a Dataset)], n: &str) -> Option<&'a Dataset> {
        side.iter().find(|x| x.0 == n).map(|x| x.1)
    }
    let mut out = Vec::new();
    for (k, n) in names.iter().enumerate() {
        progress(k, names.len());
        let (da, db) = (find(a, n), find(b, n));
        let mut tables: Vec<&str> = da
            .iter()
            .chain(db.iter())
            .flat_map(|d| d.tables.iter().map(|t| t.name.as_str()))
            .collect();
        let mut seen = std::collections::HashSet::new();
        tables.retain(|t| seen.insert(*t));
        for t in tables {
            out.push(diff_table(
                n,
                t,
                da.and_then(|d| d.table(t)),
                db.and_then(|d| d.table(t)),
                None,
            ));
        }
    }
    progress(names.len(), names.len());
    out.sort_by(|x, y| {
        y.summary
            .weight()
            .cmp(&x.summary.weight())
            .then(x.summary.dataset.cmp(&y.summary.dataset))
            .then(x.summary.table.cmp(&y.summary.table))
    });
    out
}
