//! Readable labels for raw ids (addendum 7.12): record GUIDs and names, `@loc` keys, tag
//! GUIDs and manufacturer codes become the text a player reads, with the raw id kept beside
//! it. Pure: a [`Lookup`] supplies what is known (the explorer worker from the DataCore index,
//! the app from a build's GameData and stock `global.ini`, the mocks from synthetic records);
//! every screen goes through [`label`] so they agree. Nothing here changes data or exports.

use super::explorer::is_guid;
use crate::model::{GameData, short_name};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Longest raw value looked up (longer strings are prose, not ids).
pub const MAX_RAW: usize = 256;
/// Most labels in one reply (a record or a row).
pub const MAX_LABELS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LabelKind {
    /// A DataCore record (GUID or record name): its display name, else its short name.
    Record,
    /// An `@loc` key (or, in datasets, a bare stock key): the stock text.
    Loc,
    /// A tag GUID: the tag name.
    Tag,
    /// A manufacturer code: `manufacturer_Name<code>` of the stock file.
    Manufacturer,
}

/// What a raw value reads as. The raw value itself stays the caller's (the cell, the key of
/// the label map), so it is never lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readable {
    pub kind: LabelKind,
    /// Display name, stock text, tag name or manufacturer name.
    pub text: String,
    /// Records with a display name: the short record name (`COOL_AEGS_S01_Bracer_SCItem`).
    pub detail: Option<String>,
    /// Our enhanced text of the same key, when it differs from the stock text.
    pub enhanced: Option<String>,
}

/// A record as a [`Lookup`] knows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownRecord {
    /// Lower case; `None` for records known by name only.
    pub guid: Option<String>,
    /// Full record name (`EntityClassDefinition.COOL_AEGS_S01_Bracer_SCItem`).
    pub name: String,
    /// [`LabelKind::Record`] or [`LabelKind::Tag`].
    pub kind: LabelKind,
    /// Display: an `@loc` key, or literal text (a tag name).
    pub display: Option<String>,
}

/// Where labels come from.
pub trait Lookup {
    /// A record by lower-case GUID.
    fn by_guid(&self, guid: &str) -> Option<KnownRecord>;
    /// A record by full or short name, case-insensitive.
    fn by_name(&self, _name: &str) -> Option<KnownRecord> {
        None
    }
    /// Stock text of a key (no `@`), case-insensitive.
    fn stock(&self, key: &str) -> Option<String>;
    /// Our enhanced text of a key, when we have one.
    fn enhanced(&self, _key: &str) -> Option<String> {
        None
    }
    /// Plain strings equal to a stock key read as loc keys (datasets; never in records,
    /// where keys always carry the `@`).
    fn bare_keys(&self) -> bool {
        false
    }
}

/// A lookup that knows nothing: every value stays raw.
pub struct NoLookup;

impl Lookup for NoLookup {
    fn by_guid(&self, _: &str) -> Option<KnownRecord> {
        None
    }
    fn stock(&self, _: &str) -> Option<String> {
        None
    }
}

/// Another lookup without bare-key loc matching (record JSON).
pub struct Strict<'a>(pub &'a dyn Lookup);

impl Lookup for Strict<'_> {
    fn by_guid(&self, g: &str) -> Option<KnownRecord> {
        self.0.by_guid(g)
    }
    fn by_name(&self, n: &str) -> Option<KnownRecord> {
        self.0.by_name(n)
    }
    fn stock(&self, k: &str) -> Option<String> {
        self.0.stock(k)
    }
    fn enhanced(&self, k: &str) -> Option<String> {
        self.0.enhanced(k)
    }
}

/// Text of a key, from a stock file or anything else.
pub type TextFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A [`Lookup`] over known records and text functions: the app's per-build resolver and the
/// mocks'. Lookups are hash lookups; nothing is cached beyond the maps themselves.
#[derive(Default, Clone)]
pub struct MapLookup {
    records: Vec<KnownRecord>,
    by_guid: HashMap<String, u32>,
    by_name: HashMap<String, u32>,
    stock: Option<TextFn>,
    enhanced: Option<TextFn>,
    bare: bool,
}

impl MapLookup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a record; the first record of a GUID or name wins.
    pub fn add(&mut self, r: KnownRecord) {
        let i = self.records.len() as u32;
        let mut used = false;
        if let Some(g) = &r.guid
            && let std::collections::hash_map::Entry::Vacant(v) =
                self.by_guid.entry(g.to_ascii_lowercase())
        {
            v.insert(i);
            used = true;
        }
        for n in [r.name.to_lowercase(), short_name(&r.name).to_lowercase()] {
            if !n.is_empty()
                && let std::collections::hash_map::Entry::Vacant(v) = self.by_name.entry(n)
            {
                v.insert(i);
                used = true;
            }
        }
        if used {
            self.records.push(r);
        }
    }

    pub fn with_stock(mut self, f: TextFn) -> Self {
        self.stock = Some(f);
        self
    }

    pub fn with_enhanced(mut self, f: TextFn) -> Self {
        self.enhanced = Some(f);
        self
    }

    pub fn with_bare_keys(mut self, on: bool) -> Self {
        self.bare = on;
        self
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn records(&self) -> &[KnownRecord] {
        &self.records
    }

    /// Records of a build's GameData: items and ships (display = their name key), blueprints
    /// (the product's name key), reputation standings and contracts (title key).
    pub fn from_gamedata(gd: &GameData) -> Self {
        let mut m = Self::new();
        let mut put = |guid: &str, name: &str, display: Option<&String>| {
            if name.is_empty() {
                return;
            }
            m.add(KnownRecord {
                guid: (!guid.is_empty()).then(|| guid.to_ascii_lowercase()),
                name: name.to_string(),
                kind: LabelKind::Record,
                display: display.cloned(),
            });
        };
        for c in &gd.components {
            put(&c.item.id, &c.item.record_name, c.item.loc.name.as_ref());
        }
        for w in &gd.weapons {
            put(&w.item.id, &w.item.record_name, w.item.loc.name.as_ref());
        }
        for x in &gd.missiles {
            put(&x.item.id, &x.item.record_name, x.item.loc.name.as_ref());
        }
        for x in &gd.ammo {
            put(&x.id, &x.record_name, None);
        }
        for s in &gd.ships {
            put(&s.id, &s.record_name, s.loc.name.as_ref());
        }
        for b in &gd.blueprints {
            put(&b.id, &b.record_name, b.produces_name_key.as_ref());
        }
        for r in &gd.reputation_standings {
            put(&r.id, &r.record_name, r.display_name_key.as_ref());
        }
        for c in &gd.contracts {
            put(&c.id, &c.debug_name, c.title_key.as_ref());
        }
        m
    }
}

impl Lookup for MapLookup {
    fn by_guid(&self, guid: &str) -> Option<KnownRecord> {
        self.by_guid
            .get(guid)
            .and_then(|&i| self.records.get(i as usize))
            .cloned()
    }
    fn by_name(&self, name: &str) -> Option<KnownRecord> {
        self.by_name
            .get(&name.to_lowercase())
            .and_then(|&i| self.records.get(i as usize))
            .cloned()
    }
    fn stock(&self, key: &str) -> Option<String> {
        self.stock.as_ref().and_then(|f| f(key))
    }
    fn enhanced(&self, key: &str) -> Option<String> {
        self.enhanced.as_ref().and_then(|f| f(key))
    }
    fn bare_keys(&self) -> bool {
        self.bare
    }
}

/// Column or JSON keys that hold a manufacturer code.
pub fn is_maker_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("manufacturer") || k == "maker"
}

/// A manufacturer code: 2 to 6 letters or digits.
fn is_code(s: &str) -> bool {
    (2..=6).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Text worth showing: not empty, not a placeholder.
fn shown(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty() && !t.contains("PLACEHOLDER")
}

fn loc(l: &dyn Lookup, key: &str) -> Option<Readable> {
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    let text = l.stock(key).filter(|t| shown(t))?;
    let enhanced = l.enhanced(key).filter(|e| *e != text);
    Some(Readable {
        kind: LabelKind::Loc,
        text,
        detail: None,
        enhanced,
    })
}

/// A known record as read: the display name (with the short name as detail), else the short
/// name.
pub fn record_label(l: &dyn Lookup, r: &KnownRecord) -> Readable {
    let short = short_name(&r.name).to_string();
    let display = r
        .display
        .as_deref()
        .and_then(|d| match d.strip_prefix('@') {
            Some(k) => loc(l, k).map(|x| (x.text, x.enhanced)),
            None if r.kind == LabelKind::Tag => shown(d).then(|| (d.to_string(), None)),
            None if l.bare_keys() => loc(l, d)
                .map(|x| (x.text, x.enhanced))
                .or_else(|| shown(d).then(|| (d.to_string(), None))),
            None => shown(d).then(|| (d.to_string(), None)),
        });
    match display {
        Some((text, enhanced)) => Readable {
            kind: r.kind,
            detail: (r.kind == LabelKind::Record && short != text).then_some(short),
            text,
            enhanced,
        },
        None => Readable {
            kind: r.kind,
            text: short,
            detail: None,
            enhanced: None,
        },
    }
}

/// What `raw` reads as, `key` being the column or JSON key that holds it. `None` = keep raw.
pub fn label(l: &dyn Lookup, raw: &str, key: Option<&str>) -> Option<Readable> {
    let s = raw.trim();
    if s.is_empty() || s.len() > MAX_RAW {
        return None;
    }
    if let Some(bare) = s.strip_prefix('@') {
        return loc(l, bare);
    }
    if is_guid(s) {
        return l
            .by_guid(&s.to_ascii_lowercase())
            .map(|r| record_label(l, &r));
    }
    if s.contains(char::is_whitespace) {
        return None;
    }
    if key.is_some_and(is_maker_key) && is_code(s) {
        let code = s.to_ascii_uppercase();
        if let Some(text) = l
            .stock(&format!("manufacturer_Name{code}"))
            .filter(|t| shown(t))
        {
            return Some(Readable {
                kind: LabelKind::Manufacturer,
                text,
                detail: None,
                enhanced: None,
            });
        }
    }
    if s.len() >= 4
        && (s.contains('_') || s.contains('.'))
        && let Some(r) = l.by_name(s)
    {
        return Some(record_label(l, &r));
    }
    if l.bare_keys() && s.contains('_') {
        return loc(l, s);
    }
    None
}

/// Labels of every string in `v` that reads as something, by raw value (references by their
/// GUID). At most [`MAX_LABELS`].
pub fn labels_in(l: &dyn Lookup, v: &Value) -> BTreeMap<String, Readable> {
    fn go(
        l: &dyn Lookup,
        v: &Value,
        key: Option<&str>,
        out: &mut BTreeMap<String, Readable>,
        seen: &mut HashSet<String>,
    ) {
        if out.len() >= MAX_LABELS {
            return;
        }
        match v {
            Value::String(s) => {
                if seen.insert(s.clone())
                    && let Some(r) = label(l, s, key)
                {
                    out.insert(s.clone(), r);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| go(l, x, key, out, seen)),
            Value::Object(m) if super::record::is_ref(v) => {
                if let Some(g) = m.get(super::record::REF_KEY) {
                    go(l, g, None, out, seen);
                }
            }
            Value::Object(m) => m.iter().for_each(|(k, x)| go(l, x, Some(k), out, seen)),
            _ => {}
        }
    }
    let mut out = BTreeMap::new();
    go(l, v, None, &mut out, &mut HashSet::new());
    out
}

/// Pointers tried, in order, for a row's own display key.
const TITLE_KEYS: [&str; 5] = [
    "/loc/name",
    "/Localization/Name",
    "/displayName",
    "/nameKey",
    "/name",
];

/// The display name of a dataset row: its own name key, else the display name its identity
/// value reads as, else a plain `name` that is not the identity, else the short record name
/// its identity reads as.
pub fn row_title(l: &dyn Lookup, row: &Value, identity: Option<&str>) -> Option<String> {
    for p in TITLE_KEYS {
        if let Some(s) = row.pointer(p).and_then(Value::as_str)
            && let Some(r) = label(l, s, Some("name"))
            && r.kind == LabelKind::Loc
        {
            return Some(r.text);
        }
    }
    let ids = identity
        .into_iter()
        .chain(["recordName", "className", "id", "record", "item"]);
    let ids: Vec<&str> = ids.collect();
    for c in &ids {
        if let Some(s) = row.get(*c).and_then(Value::as_str)
            && let Some(r) = label(l, s, Some(c))
            && r.detail.is_some()
        {
            return Some(r.text);
        }
    }
    if let Some(s) = row.get("name").and_then(Value::as_str)
        && identity != Some("name")
        && shown(s)
        && !s.starts_with('@')
    {
        return Some(s.to_string());
    }
    // A record without a display name: its short name, when the id is not already that.
    for c in &ids {
        if let Some(s) = row.get(*c).and_then(Value::as_str)
            && let Some(r) = label(l, s, Some(c))
            && r.text != s
        {
            return Some(r.text);
        }
    }
    None
}

fn unescape(seg: &str) -> String {
    seg.replace("~1", "/").replace("~0", "~")
}

/// `[column=value]` of a flattened path segment.
fn keyed(seg: &str) -> Option<(&str, &str)> {
    seg.strip_prefix('[')?.strip_suffix(']')?.split_once('=')
}

fn seg_text(l: &dyn Lookup, seg: &str) -> (String, bool) {
    match keyed(seg) {
        Some((c, v)) => {
            let v = unescape(v);
            match label(l, &v, Some(c)) {
                Some(r) => (r.text, true),
                None => (v, false),
            }
        }
        None => (unescape(seg), false),
    }
}

/// A flattened path (`/loadout/[className=X]/size`) with its `[column=value]` segments read:
/// `loadout › Bracer › size`. `None` when nothing in it reads as anything.
pub fn path_label(l: &dyn Lookup, path: &str) -> Option<String> {
    let mut any = false;
    let parts: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .map(|s| {
            let (t, hit) = seg_text(l, s);
            any |= hit;
            t
        })
        .collect();
    any.then(|| parts.join(" › "))
}

/// The group a field change is shown under: the path up to its first `[column=value]`
/// segment (read), else its first segment; `None` for a top-level field.
pub fn group_of(l: &dyn Lookup, path: &str) -> Option<String> {
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segs.len() < 2 {
        return None;
    }
    let end = segs
        .iter()
        .position(|s| keyed(s).is_some())
        .map_or(1, |i| i + 1);
    Some(
        segs[..end]
            .iter()
            .map(|s| seg_text(l, s).0)
            .collect::<Vec<_>>()
            .join(" › "),
    )
}

/// The unit of a numeric field, by its name, where the shapes say it (GameData and the
/// sc-gamedata files use these names). `None` when unknown.
pub fn unit_of(path: &str) -> Option<&'static str> {
    let last = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let unit = match last.as_str() {
        "mass" => "kg",
        "dps" | "weapondps" => "dmg/s",
        "rpm" | "firerate" => "rpm",
        "health" | "armorhealth" | "shieldhealth" => "HP",
        "scm" => "m/s",
        "duration" | "cooldown" => "s",
        k if k.contains("speed") => "m/s",
        k if k.ends_with("range") || k.ends_with("distance") => "m",
        k if k.ends_with("seconds") => "s",
        k if k.contains("scu") => "SCU",
        _ => return None,
    };
    Some(unit)
}

/// The display key a record value names for itself: the first `Localization.Name`, else the
/// first `displayName`/`DisplayName`/`Name`/`name` holding an `@key` (placeholders skipped);
/// for a tag, its `tagName`. Breadth first, a few levels deep.
pub fn display_of(v: &Value) -> Option<(LabelKind, String)> {
    let body = v.get("_RecordValue_").unwrap_or(v);
    if let Some(t) = body.get("tagName").and_then(Value::as_str)
        && body.get("_Type_").and_then(Value::as_str) == Some("Tag")
    {
        return Some((LabelKind::Tag, t.to_string()));
    }
    let key_ok = |s: &str| s.len() > 1 && s.starts_with('@') && !s.starts_with("@LOC_");
    let mut level: Vec<&Value> = vec![body];
    for _ in 0..6 {
        let mut next = Vec::new();
        for n in &level {
            if let Some(s) = n.pointer("/Localization/Name").and_then(Value::as_str)
                && key_ok(s)
            {
                return Some((LabelKind::Record, s.to_string()));
            }
        }
        for n in &level {
            if let Value::Object(m) = n {
                for k in ["displayName", "DisplayName", "Name", "name"] {
                    if let Some(s) = m.get(k).and_then(Value::as_str)
                        && key_ok(s)
                    {
                        return Some((LabelKind::Record, s.to_string()));
                    }
                }
            }
        }
        for n in level {
            match n {
                Value::Object(m) if !super::record::is_ref(n) => {
                    next.extend(m.values().filter(|x| x.is_object() || x.is_array()))
                }
                Value::Array(a) => next.extend(a.iter().filter(|x| x.is_object() || x.is_array())),
                _ => {}
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    None
}
