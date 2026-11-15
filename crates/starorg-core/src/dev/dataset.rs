//! Generic dataset viewer (addendum 7.5): shape inference over a JSON file, typed columns,
//! search, filters, sort and paging. Parsed as data only; nothing is executed or rendered as
//! HTML.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Largest dataset file read (7.10).
pub const MAX_FILE: u64 = 64 * 1024 * 1024;
/// Largest total of a folder's `*.json` files.
pub const MAX_FOLDER: u64 = 4 * 1024 * 1024 * 1024;
/// Text cells longer than this are cut in a page (full value in the detail).
pub const MAX_CELL: usize = 200;
/// Distinct values listed for a text column with at most this many.
pub const MAX_DISTINCT: usize = 50;
/// Columns shown by default.
pub const DEFAULT_COLUMNS: usize = 8;
/// Columns that go first when present.
pub const LEADING: [&str; 3] = ["name", "recordName", "id"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColType {
    Text,
    Number,
    Bool,
    Object,
    List,
    Mixed,
    /// Only nulls or missing.
    Empty,
}

impl ColType {
    pub fn scalar(self) -> bool {
        matches!(self, ColType::Text | ColType::Number | ColType::Bool)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueCount {
    pub value: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub kind: ColType,
    /// Rows with a non-null value.
    pub filled: u32,
    /// Text columns with at most [`MAX_DISTINCT`] values: each with its count.
    pub distinct: Option<Vec<ValueCount>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TableKind {
    /// A top-level array of objects (or the only object table of a file).
    Main,
    /// `byManufacturer`, `oreLocations`: key + value.
    Index,
    /// Anything else: one row holding the value, shown as a JSON tree.
    Tree,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSummary {
    pub name: String,
    pub kind: TableKind,
    pub rows: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub file: String,
    pub bytes: u64,
    /// `_source`, `_extracted`, `_build`, ...
    pub meta: Vec<Fact>,
    /// `summary` entries.
    pub facts: Vec<Fact>,
    pub tables: Vec<TableSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
    pub kind: TableKind,
    pub rows: u32,
    pub columns: Vec<Column>,
    /// The column that names a record (diff identity), when one is unique.
    pub identity: Option<String>,
    pub default_columns: Vec<String>,
}

/// One table: rows are JSON objects.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub name: String,
    pub kind: TableKind,
    pub rows: Vec<Value>,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    pub info: DatasetInfo,
    pub tables: Vec<Table>,
}

fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Text of a cell in a page: scalars (cut at [`MAX_CELL`] characters), `{…}`, `[n]`.
pub fn cell_text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Null => None,
        Value::Object(m) if m.is_empty() => Some("{}".into()),
        Value::Object(_) => Some("{…}".into()),
        Value::Array(a) => Some(format!("[{}]", a.len())),
        other => {
            let s = scalar_text(other)?;
            Some(if s.chars().count() > MAX_CELL {
                let mut t: String = s.chars().take(MAX_CELL).collect();
                t.push('…');
                t
            } else {
                s
            })
        }
    }
}

fn kind_of(v: &Value) -> Option<ColType> {
    match v {
        Value::Null => None,
        Value::String(_) => Some(ColType::Text),
        Value::Number(_) => Some(ColType::Number),
        Value::Bool(_) => Some(ColType::Bool),
        Value::Object(_) => Some(ColType::Object),
        Value::Array(_) => Some(ColType::List),
    }
}

/// Union of the row keys in first-seen order, each typed.
pub fn columns(rows: &[Value]) -> Vec<Column> {
    struct Acc {
        kind: Option<ColType>,
        filled: u32,
        distinct: BTreeMap<String, u32>,
        too_many: bool,
        min: Option<f64>,
        max: Option<f64>,
    }
    let mut order: Vec<String> = Vec::new();
    let mut acc: std::collections::HashMap<String, Acc> = Default::default();
    for r in rows {
        let Some(m) = r.as_object() else { continue };
        for (k, v) in m {
            let a = acc.entry(k.clone()).or_insert_with(|| {
                order.push(k.clone());
                Acc {
                    kind: None,
                    filled: 0,
                    distinct: BTreeMap::new(),
                    too_many: false,
                    min: None,
                    max: None,
                }
            });
            let Some(kind) = kind_of(v) else { continue };
            a.filled += 1;
            a.kind = match a.kind {
                None => Some(kind),
                Some(k) if k == kind => Some(k),
                Some(_) => Some(ColType::Mixed),
            };
            if let Value::String(s) = v
                && !a.too_many
            {
                *a.distinct.entry(s.clone()).or_default() += 1;
                if a.distinct.len() > MAX_DISTINCT {
                    a.too_many = true;
                    a.distinct.clear();
                }
            }
            if let Some(n) = v.as_f64() {
                a.min = Some(a.min.map_or(n, |m| m.min(n)));
                a.max = Some(a.max.map_or(n, |m| m.max(n)));
            }
        }
    }
    order
        .into_iter()
        .map(|name| {
            let a = acc.remove(&name).expect("seen");
            let kind = a.kind.unwrap_or(ColType::Empty);
            let mut distinct: Vec<ValueCount> = a
                .distinct
                .into_iter()
                .map(|(value, count)| ValueCount { value, count })
                .collect();
            distinct.sort_by(|x, y| y.count.cmp(&x.count).then(x.value.cmp(&y.value)));
            Column {
                name,
                kind,
                filled: a.filled,
                distinct: (kind == ColType::Text && !a.too_many && !distinct.is_empty())
                    .then_some(distinct),
                min: (kind == ColType::Number).then_some(a.min).flatten(),
                max: (kind == ColType::Number).then_some(a.max).flatten(),
            }
        })
        .collect()
}

/// Default visible columns: `name`, `recordName`, `id` first, then the first scalar columns,
/// [`DEFAULT_COLUMNS`] in all.
pub fn default_columns(cols: &[Column]) -> Vec<String> {
    let mut out: Vec<String> = LEADING
        .iter()
        .filter(|l| cols.iter().any(|c| c.name == **l))
        .map(|s| s.to_string())
        .collect();
    for c in cols {
        if out.len() >= DEFAULT_COLUMNS {
            break;
        }
        if c.kind.scalar() && !out.contains(&c.name) {
            out.push(c.name.clone());
        }
    }
    if out.is_empty() {
        out.extend(cols.iter().take(DEFAULT_COLUMNS).map(|c| c.name.clone()));
    }
    out
}

fn is_object_array(v: &Value) -> bool {
    matches!(v, Value::Array(a) if !a.is_empty() && a.iter().all(Value::is_object))
}

fn kv_rows(m: &Map<String, Value>) -> Vec<Value> {
    m.iter()
        .map(|(k, v)| {
            let mut r = Map::new();
            r.insert("key".into(), Value::String(k.clone()));
            r.insert("value".into(), v.clone());
            Value::Object(r)
        })
        .collect()
}

fn table(name: &str, kind: TableKind, rows: Vec<Value>) -> Table {
    let columns = columns(&rows);
    Table {
        name: name.to_string(),
        kind,
        rows,
        columns,
    }
}

fn fact_text(v: &Value) -> String {
    scalar_text(v).unwrap_or_else(|| serde_json::to_string(v).unwrap_or_default())
}

/// Shape inference (7.5), checked against the shapes of every `sc-gamedata` file:
///
/// - `_`-keys are metadata, `summary` is header facts;
/// - a top-level array of objects is a table (a top-level array file is one table);
/// - arrays of arrays with a sibling `columns` list of the same width use those names;
/// - an object of scalars is a key/value table: the main table when the file has no array
///   tables (`strings`), else an index;
/// - an object of objects or arrays is an index table;
/// - anything else is one row shown as a JSON tree.
pub fn infer(file: &str, bytes: u64, root: Value) -> Dataset {
    let mut meta = Vec::new();
    let mut facts = Vec::new();
    let mut tables = Vec::new();
    match root {
        Value::Array(a) => {
            let rows = if a.iter().all(Value::is_object) {
                a
            } else {
                a.into_iter()
                    .map(|v| {
                        let mut r = Map::new();
                        r.insert("value".into(), v);
                        Value::Object(r)
                    })
                    .collect()
            };
            tables.push(table(file_stem(file), TableKind::Main, rows));
        }
        Value::Object(m) => {
            let names: Option<Vec<String>> = m.get("columns").and_then(|c| {
                c.as_array()?
                    .iter()
                    .map(|x| x.as_str().map(str::to_string))
                    .collect()
            });
            let has_array_tables = m.iter().any(|(k, v)| {
                !k.starts_with('_') && (is_object_array(v) || arrays_with_columns(v, &names))
            });
            let mut deferred = Vec::new();
            for (k, v) in m {
                if k.starts_with('_') {
                    meta.push(Fact {
                        key: k,
                        value: fact_text(&v),
                    });
                    continue;
                }
                if k == "summary"
                    && let Value::Object(s) = &v
                {
                    facts.extend(s.iter().map(|(k, v)| Fact {
                        key: k.clone(),
                        value: fact_text(v),
                    }));
                    continue;
                }
                if is_object_array(&v) {
                    let Value::Array(rows) = v else {
                        unreachable!()
                    };
                    tables.push(table(&k, TableKind::Main, rows));
                } else if arrays_with_columns(&v, &names) {
                    let names = names.clone().unwrap_or_default();
                    let Value::Array(rows) = v else {
                        unreachable!()
                    };
                    let rows = rows
                        .into_iter()
                        .map(|r| {
                            let Value::Array(cells) = r else {
                                return Value::Object(Map::new());
                            };
                            Value::Object(names.iter().cloned().zip(cells).collect())
                        })
                        .collect();
                    tables.push(table(&k, TableKind::Main, rows));
                } else if let Value::Object(o) = &v
                    && !o.is_empty()
                {
                    let all_scalar = o.values().all(|x| !x.is_object() && !x.is_array());
                    let kind = if all_scalar && !has_array_tables {
                        TableKind::Main
                    } else {
                        TableKind::Index
                    };
                    deferred.push(table(&k, kind, kv_rows(o)));
                } else if k != "columns" {
                    let mut r = Map::new();
                    r.insert("key".into(), Value::String(k.clone()));
                    r.insert("value".into(), v);
                    deferred.push(table(&k, TableKind::Tree, vec![Value::Object(r)]));
                }
            }
            // Main tables first, then indexes, then JSON-only values (file order within each).
            tables.extend(
                deferred
                    .iter()
                    .filter(|t| t.kind == TableKind::Main)
                    .cloned(),
            );
            tables.extend(
                deferred
                    .iter()
                    .filter(|t| t.kind == TableKind::Index)
                    .cloned(),
            );
            tables.extend(deferred.into_iter().filter(|t| t.kind == TableKind::Tree));
        }
        other => {
            let mut r = Map::new();
            r.insert("value".into(), other);
            tables.push(table(
                file_stem(file),
                TableKind::Tree,
                vec![Value::Object(r)],
            ));
        }
    }
    let info = DatasetInfo {
        file: file.to_string(),
        bytes,
        meta,
        facts,
        tables: tables
            .iter()
            .map(|t| TableSummary {
                name: t.name.clone(),
                kind: t.kind,
                rows: t.rows.len() as u32,
            })
            .collect(),
    };
    Dataset { info, tables }
}

fn arrays_with_columns(v: &Value, names: &Option<Vec<String>>) -> bool {
    let Some(names) = names else { return false };
    matches!(v, Value::Array(a) if !a.is_empty()
        && a.iter().all(|r| r.as_array().is_some_and(|c| c.len() == names.len())))
}

pub fn file_stem(file: &str) -> &str {
    let f = file.rsplit(['/', '\\']).next().unwrap_or(file);
    f.strip_suffix(".json").unwrap_or(f)
}

impl Dataset {
    /// Parse JSON bytes as data (serde_json's nesting limit of 128 applies).
    pub fn parse(file: &str, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_FILE {
            return Err(format!(
                "{file} is {} bytes, more than the {MAX_FILE} allowed",
                bytes.len()
            ));
        }
        let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("{file}: {e}"))?;
        Ok(infer(file, bytes.len() as u64, v))
    }

    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }
}

impl Table {
    pub fn info(&self) -> TableInfo {
        TableInfo {
            name: self.name.clone(),
            kind: self.kind,
            rows: self.rows.len() as u32,
            columns: self.columns.clone(),
            identity: identity_column(&self.rows, None),
            default_columns: default_columns(&self.columns),
        }
    }

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

// ---------------------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------------------

/// One column filter; the parts that are set must all hold.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowFilter {
    pub column: String,
    /// Value chips: the cell's text is one of these.
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// "Has a value" (true) or "is empty" (false).
    #[serde(default)]
    pub has: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowSort {
    pub column: String,
    #[serde(default)]
    pub desc: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowsQuery {
    /// Columns returned (and searched); empty = the table's default columns.
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub filters: Vec<RowFilter>,
    #[serde(default)]
    pub sort: Option<RowSort>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Position in the table.
    pub id: u32,
    pub cells: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowsPage {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    pub offset: u32,
    /// Rows matching the search and filters.
    pub total: u32,
    /// Rows in the table.
    pub all: u32,
}

fn filter_ok(row: &Value, f: &RowFilter) -> bool {
    let v = row.get(&f.column).filter(|v| !v.is_null());
    if let Some(h) = f.has
        && v.is_some() != h
    {
        return false;
    }
    if !f.values.is_empty() {
        let Some(t) = v.and_then(scalar_text) else {
            return false;
        };
        if !f.values.contains(&t) {
            return false;
        }
    }
    if f.min.is_some() || f.max.is_some() {
        let Some(n) = v.and_then(Value::as_f64) else {
            return false;
        };
        if f.min.is_some_and(|m| n < m) || f.max.is_some_and(|m| n > m) {
            return false;
        }
    }
    true
}

fn cmp_cells(a: Option<&Value>, b: Option<&Value>) -> Ordering {
    let a = a.filter(|v| !v.is_null());
    let b = b.filter(|v| !v.is_null());
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(p), Some(q)) => p.partial_cmp(&q).unwrap_or(Ordering::Equal),
            _ => {
                let p = scalar_text(x).unwrap_or_default().to_lowercase();
                let q = scalar_text(y).unwrap_or_default().to_lowercase();
                p.cmp(&q)
            }
        },
    }
}

/// Search (substring over the scalar cells of the shown columns), filters, sort, page.
pub fn query_rows(t: &Table, q: &RowsQuery, max_page: usize) -> RowsPage {
    let columns = if q.columns.is_empty() {
        default_columns(&t.columns)
    } else {
        q.columns.clone()
    };
    let needle = q.search.trim().to_lowercase();
    let mut hits: Vec<usize> = (0..t.rows.len())
        .filter(|&i| {
            let r = &t.rows[i];
            q.filters.iter().all(|f| filter_ok(r, f))
                && (needle.is_empty()
                    || columns.iter().any(|c| {
                        r.get(c)
                            .and_then(scalar_text)
                            .is_some_and(|s| s.to_lowercase().contains(&needle))
                    }))
        })
        .collect();
    if let Some(s) = &q.sort {
        hits.sort_by(|&a, &b| {
            let o = cmp_cells(t.rows[a].get(&s.column), t.rows[b].get(&s.column));
            let o = if s.desc { o.reverse() } else { o };
            o.then(a.cmp(&b))
        });
    }
    let total = hits.len();
    let limit = (q.limit as usize).clamp(1, max_page);
    let rows = hits
        .into_iter()
        .skip(q.offset as usize)
        .take(limit)
        .map(|i| Row {
            id: i as u32,
            cells: columns
                .iter()
                .map(|c| cell_text(t.rows[i].get(c)))
                .collect(),
        })
        .collect();
    RowsPage {
        columns,
        rows,
        offset: q.offset,
        total: total as u32,
        all: t.rows.len() as u32,
    }
}

// ---------------------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------------------

/// Columns tried, in order, as a record's identity (7.6).
pub const IDENTITY: [&str; 6] = ["id", "recordName", "className", "name", "key", "code"];

/// The identity text of a row in `column`.
pub fn identity_of(row: &Value, column: &str) -> Option<String> {
    row.get(column)
        .and_then(scalar_text)
        .filter(|s| !s.is_empty())
}

fn unique_in(rows: &[Value], column: &str) -> bool {
    let mut seen = std::collections::HashSet::with_capacity(rows.len());
    !rows.is_empty()
        && rows
            .iter()
            .all(|r| identity_of(r, column).is_some_and(|k| seen.insert(k)))
}

/// The first of [`IDENTITY`] present and unique in every given table.
pub fn identity_column(rows: &[Value], other: Option<&[Value]>) -> Option<String> {
    IDENTITY
        .iter()
        .find(|c| unique_in(rows, c) && other.is_none_or(|o| o.is_empty() || unique_in(o, c)))
        .map(|c| c.to_string())
}
