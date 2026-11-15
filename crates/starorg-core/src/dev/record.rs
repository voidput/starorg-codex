//! Record JSON for the explorer (addendum 7.4): replies of at most 1 MiB, cut by JSON pointer
//! and depth, and the loc keys a record holds.

use crate::ini::StockIni;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Largest record JSON in one reply (7.3).
pub const MAX_REPLY: usize = 1 << 20;
/// Depth a too-large reply is cut at; deeper nodes load on expand.
pub const CUT_DEPTH: usize = 3;
/// Most loc keys resolved for one record.
pub const MAX_LOC: usize = 500;
/// The key of a cut node: `{"_cut": {"kind": "object", "len": 12}}`.
pub const CUT_KEY: &str = "_cut";
/// The key of a reference written by our sink: `{"_ref": guid, "name", "type", "path"}`.
pub const REF_KEY: &str = "_ref";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordHeader {
    pub guid: String,
    pub name: String,
    #[serde(rename = "type")]
    pub struct_type: String,
    pub path: String,
    pub tag: Option<String>,
    pub main: bool,
    /// Size of the whole record as compact JSON.
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocText {
    /// As written in the record, with the `@`.
    pub key: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordView {
    pub header: RecordHeader,
    /// The node at `pointer`, compact JSON, at most [`MAX_REPLY`] bytes.
    pub json: String,
    pub pointer: String,
    /// True when nodes were replaced by `{"_cut": ...}` stubs.
    pub truncated: bool,
    /// `@key` strings in the reply that the channel's stock `global.ini` has.
    pub loc: Vec<LocText>,
    /// What references, GUIDs and `@key` strings in the reply read as, by raw value
    /// (addendum 7.14).
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, super::readable::Readable>,
}

/// Replace containers deeper than `depth` with cut stubs.
pub fn cut(v: &Value, depth: usize) -> (Value, bool) {
    fn go(v: &Value, depth: usize, cut_any: &mut bool) -> Value {
        match v {
            Value::Object(m) if is_ref(v) => Value::Object(m.clone()),
            Value::Object(m) if depth == 0 && !m.is_empty() => {
                *cut_any = true;
                json!({ CUT_KEY: { "kind": "object", "len": m.len() } })
            }
            Value::Array(a) if depth == 0 && !a.is_empty() => {
                *cut_any = true;
                json!({ CUT_KEY: { "kind": "array", "len": a.len() } })
            }
            Value::Object(m) => Value::Object(
                m.iter()
                    .map(|(k, x)| (k.clone(), go(x, depth - 1, cut_any)))
                    .collect::<Map<_, _>>(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(|x| go(x, depth - 1, cut_any)).collect()),
            other => other.clone(),
        }
    }
    let mut any = false;
    let out = go(v, depth, &mut any);
    (out, any)
}

/// A reference object written by our sink.
pub fn is_ref(v: &Value) -> bool {
    v.get(REF_KEY).is_some_and(Value::is_string)
}

/// The JSON text for `pointer` (`""` = the record): whole when it fits in [`MAX_REPLY`] and
/// no depth is asked for, else cut at `depth` (default [`CUT_DEPTH`]), shallower until it fits.
pub fn view_json(
    root: &Value,
    pointer: &str,
    depth: Option<usize>,
) -> Result<(String, bool), String> {
    let node = root
        .pointer(pointer)
        .ok_or_else(|| format!("no node at {pointer:?}"))?;
    if depth.is_none() {
        let s = serde_json::to_string(node).map_err(|e| e.to_string())?;
        if s.len() <= MAX_REPLY {
            return Ok((s, false));
        }
    }
    let mut d = depth.unwrap_or(CUT_DEPTH).max(1);
    loop {
        let (v, any) = cut(node, d);
        let s = serde_json::to_string(&v).map_err(|e| e.to_string())?;
        if s.len() <= MAX_REPLY || d == 1 {
            return Ok((s, any));
        }
        d -= 1;
    }
}

/// `@key` strings in `v`, first-seen order, without duplicates, at most [`MAX_LOC`].
pub fn loc_keys(v: &Value) -> Vec<String> {
    fn go(v: &Value, out: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
        if out.len() >= MAX_LOC {
            return;
        }
        match v {
            Value::String(s) if s.len() > 1 && s.starts_with('@') => {
                if seen.insert(s.clone()) {
                    out.push(s.clone());
                }
            }
            Value::Array(a) => a.iter().for_each(|x| go(x, out, seen)),
            Value::Object(m) => m.values().for_each(|x| go(x, out, seen)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    go(v, &mut out, &mut Default::default());
    out
}

/// English text of the `@key` strings the stock file has.
pub fn resolve_loc(keys: &[String], stock: Option<&StockIni>) -> Vec<LocText> {
    let Some(stock) = stock else { return vec![] };
    keys.iter()
        .filter_map(|k| {
            let bare = k.strip_prefix('@').unwrap_or(k);
            stock.value(bare).map(|t| LocText {
                key: k.clone(),
                text: t.to_string(),
            })
        })
        .collect()
}
