//! Who uses a loc key, in words: the Strings "Owner" column (addendum, Strings owners).
//!
//! For every stock key: the records that show it and which of their fields it is, so the grid
//! can say `Bracer · description` instead of only `item_DescCOOL_AEGS_S01_Bracer`.
//!
//! Rules, in order:
//!
//! 1. [`GameData`] records whose loc keys include the key: components, ship and FPS weapons,
//!    missiles, ships and ground vehicles, contracts and reputation standings. The owner name
//!    is the record's display text in the stock file (its record name when that is a
//!    placeholder); owners with the same name, kind and manufacturer collapse into one.
//! 2. Key shapes without a record: `item_Name*`/`vehicle_Name*` name themselves, their
//!    `Desc`/`ShortName` siblings take that name (the namesake stem,
//!    [`namesake::stem`]); `manufacturer_Name<code>`/`manufacturer_Desc<code>` belong to the
//!    manufacturer; `X_desc` (also `X_desc_N`) belongs to the text of `X`, `X_name` or
//!    `X_title` (`X_title_N`), and that key names itself.
//! 3. Nothing: the grid shows [`pretty_key`] (`item desc · COOL_AEGS_S01_Bracer`).
//!
//! Built from the cached GameData and the stock file only (no Developer mode, no worker).
//! Pure and deterministic; the result is compact (two `u32` per key plus the distinct
//! owners) and lives next to the build's [`super::GroupIndex`].

use crate::ini::{StockIni, fold_key, strip_flag_suffix};
use crate::model::{GameData, ItemIdentity, WeaponClass, is_placeholder_name, loc_key, namesake};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

/// Which field of its owner a key is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Field {
    Name,
    ShortName,
    Description,
    Title,
    Career,
    Role,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "name",
            Field::ShortName => "short name",
            Field::Description => "description",
            Field::Title => "title",
            Field::Career => "career",
            Field::Role => "role",
        }
    }
}

/// What kind of thing owns a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OwnerKind {
    Ship,
    GroundVehicle,
    Component,
    ShipWeapon,
    FpsWeapon,
    Missile,
    Mission,
    Reputation,
    Manufacturer,
    /// Rule 2 without a manufacturer: the owner is the text of a sibling key (or the key's
    /// own text), not a GameData record.
    Named,
}

impl OwnerKind {
    pub fn label(self) -> &'static str {
        match self {
            OwnerKind::Ship => "Ship",
            OwnerKind::GroundVehicle => "Ground vehicle",
            OwnerKind::Component => "Ship component",
            OwnerKind::ShipWeapon => "Ship weapon",
            OwnerKind::FpsWeapon => "FPS weapon",
            OwnerKind::Missile => "Missile",
            OwnerKind::Mission => "Mission",
            OwnerKind::Reputation => "Reputation",
            OwnerKind::Manufacturer => "Manufacturer",
            OwnerKind::Named => "Named by key",
        }
    }
}

/// One distinct owner.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Record {
    name: Box<str>,
    kind: OwnerKind,
    /// Manufacturer display name.
    detail: Option<Box<str>>,
}

/// One owner of one key, borrowed from the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner<'a> {
    pub name: &'a str,
    pub field: Field,
    pub kind: OwnerKind,
    pub detail: Option<&'a str>,
}

/// The result of [`OwnerIndex::build`].
#[derive(Debug, Clone, Default)]
pub struct OwnerIndex {
    records: Vec<Record>,
    /// `offsets[row]..offsets[row + 1]` is the row's slice of `uses`; `rows + 1` entries.
    offsets: Vec<u32>,
    /// `(record, field)`, the primary owner first.
    uses: Vec<(u32, Field)>,
}

impl OwnerIndex {
    /// Number of stock rows covered.
    pub fn rows(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Distinct owners known.
    pub fn records(&self) -> usize {
        self.records.len()
    }

    fn slice(&self, row: usize) -> &[(u32, Field)] {
        match (self.offsets.get(row), self.offsets.get(row + 1)) {
            (Some(&a), Some(&b)) => &self.uses[a as usize..b as usize],
            _ => &[],
        }
    }

    fn owner(&self, (rec, field): (u32, Field)) -> Owner<'_> {
        let r = &self.records[rec as usize];
        Owner {
            name: &r.name,
            field,
            kind: r.kind,
            detail: r.detail.as_deref(),
        }
    }

    /// Every owner of the key of stock row `row` (in [`StockIni::entries`] order), the
    /// primary one first.
    pub fn owners(&self, row: usize) -> impl Iterator<Item = Owner<'_>> {
        self.slice(row).iter().map(|u| self.owner(*u))
    }

    pub fn primary(&self, row: usize) -> Option<Owner<'_>> {
        self.slice(row).first().map(|u| self.owner(*u))
    }

    pub fn count(&self, row: usize) -> usize {
        self.slice(row).len()
    }

    /// Rough heap bytes held, for the memory report.
    pub fn heap_bytes(&self) -> usize {
        let recs: usize = self
            .records
            .iter()
            .map(|r| r.name.len() + r.detail.as_ref().map_or(0, |d| d.len()))
            .sum();
        recs + self.records.capacity() * std::mem::size_of::<Record>()
            + self.offsets.capacity() * 4
            + self.uses.capacity() * std::mem::size_of::<(u32, Field)>()
    }

    /// Index every stock key (rules 1-3 of the module docs).
    pub fn build(stock: &StockIni, gd: &GameData) -> Self {
        let mut b = Builder {
            stock,
            records: Vec::new(),
            by_record: HashMap::new(),
            links: HashMap::new(),
        };
        b.game_data(gd);
        let siblings = Siblings::new(stock);
        let mut offsets = Vec::with_capacity(stock.key_count() + 1);
        let mut uses: Vec<(u32, Field)> = Vec::new();
        offsets.push(0);
        for (_, key, value) in stock.entries() {
            let folded = fold_key(key);
            if let Some(list) = b.links.get(&folded) {
                uses.extend(list.iter().map(|(r, f, _)| (*r, *f)));
            } else if let Some((rec, field)) = siblings.owner(&folded, value) {
                let id = b.intern(rec);
                uses.push((id, field));
            }
            offsets.push(uses.len() as u32);
        }
        let mut records = b.records;
        records.shrink_to_fit();
        uses.shrink_to_fit();
        Self {
            records,
            offsets,
            uses,
        }
    }
}

/// A one-line text short enough to be a name: trimmed (also of literal `\n` at either end),
/// game variables (`~mission(TargetName)`) made readable (`[target name]`), placeholders
/// (`<-=MISSING=->`, `PH - ...`) refused.
fn as_name(v: &str) -> Option<Cow<'_, str>> {
    let mut v = v.trim();
    loop {
        let t = v
            .strip_prefix("\\n")
            .or_else(|| v.strip_suffix("\\n"))
            .map(str::trim);
        match t {
            Some(t) => v = t,
            None => break,
        }
    }
    let lower = v.to_ascii_lowercase();
    if v.is_empty()
        || v.contains("\\n")
        || is_placeholder_name(Some(v))
        || lower.contains("=missing=")
        || lower == "ph"
        || lower.starts_with("ph ")
        || lower.starts_with("ph-")
        || lower.starts_with("[ph]")
    {
        return None;
    }
    let v = if v.contains('~') {
        Cow::Owned(humanise_vars(v))
    } else {
        Cow::Borrowed(v)
    };
    (!v.trim().is_empty() && v.len() <= 120).then_some(v)
}

/// `Bounty: ~mission(TargetName)` -> `Bounty: [target name]`.
fn humanise_vars(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut rest = v;
    while let Some(i) = rest.find('~') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let open = after.find('(');
        let close = after.find(')');
        match (open, close) {
            (Some(o), Some(c))
                if o < c
                    && !after[..o].trim().is_empty()
                    && after[..o]
                        .trim_end()
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_') =>
            {
                out.push('[');
                out.push_str(&words(&after[o + 1..c]));
                out.push(']');
                rest = &after[c + 1..];
            }
            _ => {
                out.push('~');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

struct Builder<'s> {
    stock: &'s StockIni,
    records: Vec<Record>,
    by_record: HashMap<Record, u32>,
    /// Folded key -> `(record, field, namesake)`, rule 1.
    links: HashMap<String, Vec<(u32, Field, bool)>>,
}

impl Builder<'_> {
    fn intern(&mut self, r: Record) -> u32 {
        if let Some(&i) = self.by_record.get(&r) {
            return i;
        }
        let i = self.records.len() as u32;
        self.records.push(r.clone());
        self.by_record.insert(r, i);
        i
    }

    fn text(&self, key: Option<&str>) -> Option<String> {
        let k = key.and_then(loc_key)?;
        as_name(self.stock.value(k)?).map(Cow::into_owned)
    }

    fn maker(&self, code: Option<&str>) -> Option<Box<str>> {
        let code = code.filter(|c| !c.is_empty())?.to_ascii_uppercase();
        if code == "NONE" {
            return None;
        }
        let v = as_name(self.stock.value(&format!("manufacturer_Name{code}"))?)?;
        (!v.eq_ignore_ascii_case("unknown")).then(|| v.into())
    }

    /// Link `key` to a record; `short` is the record's short name (namesake order).
    fn link(&mut self, key: Option<&str>, rec: u32, field: Field, short: &str) {
        // `LOC_PLACEHOLDER` and friends are engine fillers, owned by nobody.
        let Some(k) = key
            .and_then(loc_key)
            .filter(|k| !k.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("loc_")))
        else {
            return;
        };
        let namesake = namesake::named_after(k, short);
        let list = self.links.entry(fold_key(k)).or_default();
        if !list.iter().any(|(r, f, _)| *r == rec && *f == field) {
            list.push((rec, field, namesake));
        }
    }

    fn item(&mut self, it: &ItemIdentity, kind: OwnerKind) {
        if it.is_template {
            return;
        }
        let short = it.short_name().to_string();
        let name = self
            .text(it.loc.name.as_deref())
            .unwrap_or_else(|| short.clone());
        let detail = self.maker(it.manufacturer.as_deref());
        let rec = self.intern(Record {
            name: name.into(),
            kind,
            detail,
        });
        self.link(it.loc.name.as_deref(), rec, Field::Name, &short);
        self.link(it.loc.short_name.as_deref(), rec, Field::ShortName, &short);
        self.link(it.loc.desc.as_deref(), rec, Field::Description, &short);
    }

    fn game_data(&mut self, gd: &GameData) {
        // One record, one owner: a ship weapon or missile is also listed among the components
        // (its category is `weapons`/`missiles`); the specific list wins.
        let mut order: Vec<&str> = Vec::new();
        let mut items: HashMap<&str, (&ItemIdentity, OwnerKind)> = HashMap::new();
        let weapons = gd.weapons.iter().map(|w| {
            let kind = match w.class {
                WeaponClass::Ship => OwnerKind::ShipWeapon,
                WeaponClass::Fps => OwnerKind::FpsWeapon,
            };
            (&w.item, kind)
        });
        let all = gd
            .components
            .iter()
            .map(|c| (&c.item, OwnerKind::Component))
            .chain(weapons)
            .chain(gd.missiles.iter().map(|m| (&m.item, OwnerKind::Missile)));
        for (it, kind) in all {
            match items.get_mut(it.record_name.as_str()) {
                Some(slot) => *slot = (it, kind),
                None => {
                    order.push(&it.record_name);
                    items.insert(&it.record_name, (it, kind));
                }
            }
        }
        for name in order {
            let (it, kind) = items[name];
            self.item(it, kind);
        }
        for s in &gd.ships {
            if s.is_template {
                continue;
            }
            let short = crate::model::short_name(&s.record_name).to_string();
            let kind = if s.path.contains("groundvehicle") {
                OwnerKind::GroundVehicle
            } else {
                OwnerKind::Ship
            };
            let name = self
                .text(s.loc.name.as_deref())
                .unwrap_or_else(|| short.clone());
            let detail = self.maker(s.manufacturer.as_deref());
            let rec = self.intern(Record {
                name: name.into(),
                kind,
                detail,
            });
            self.link(s.loc.name.as_deref(), rec, Field::Name, &short);
            self.link(s.loc.desc.as_deref(), rec, Field::Description, &short);
            self.link(s.loc.career.as_deref(), rec, Field::Career, &short);
            self.link(s.loc.role.as_deref(), rec, Field::Role, &short);
        }
        for c in &gd.contracts {
            let name = self
                .text(c.title_key.as_deref())
                .unwrap_or_else(|| c.debug_name.clone());
            if name.is_empty() {
                continue;
            }
            let rec = self.intern(Record {
                name: name.into(),
                kind: OwnerKind::Mission,
                detail: None,
            });
            self.link(c.title_key.as_deref(), rec, Field::Title, "");
            self.link(c.description_key.as_deref(), rec, Field::Description, "");
        }
        for r in &gd.reputation_standings {
            let name = self
                .text(r.display_name_key.as_deref())
                .unwrap_or_else(|| r.name.clone());
            let rec = self.intern(Record {
                name: name.into(),
                kind: OwnerKind::Reputation,
                detail: None,
            });
            self.link(r.display_name_key.as_deref(), rec, Field::Name, &r.name);
        }
        // Primary first: the namesake record, then a specific kind over "component", one
        // with a maker over one without, then kind, name and field.
        let records = &self.records;
        let rank = |r: &Record| {
            (
                r.kind == OwnerKind::Component,
                r.kind,
                r.detail.is_none(),
                r.name.to_lowercase(),
            )
        };
        for list in self.links.values_mut() {
            list.sort_by_cached_key(|(rec, field, namesake)| {
                (!namesake, rank(&records[*rec as usize]), *field)
            });
            // One entry per owner: the same record under another field, and records that
            // read the same (same name, compatible kind, same or missing maker), collapse
            // into the first.
            let mut kept: Vec<(u32, Field, bool)> = Vec::with_capacity(list.len());
            for e in list.drain(..) {
                let r = &records[e.0 as usize];
                let dup = kept.iter().any(|k| {
                    let o = &records[k.0 as usize];
                    k.0 == e.0
                        || (o.name.eq_ignore_ascii_case(&r.name)
                            && kinds_match(o.kind, r.kind)
                            && (o.detail == r.detail || o.detail.is_none() || r.detail.is_none()))
                });
                if !dup {
                    kept.push(e);
                }
            }
            *list = kept;
        }
    }
}

/// Two kinds that may name the same thing: equal, or a component and a weapon or missile.
fn kinds_match(a: OwnerKind, b: OwnerKind) -> bool {
    let item = |k| {
        matches!(
            k,
            OwnerKind::Component
                | OwnerKind::ShipWeapon
                | OwnerKind::FpsWeapon
                | OwnerKind::Missile
        )
    };
    a == b || (item(a) && item(b) && (a == OwnerKind::Component || b == OwnerKind::Component))
}

/// Rule 2: owners read from the key's shape and its sibling keys.
struct Siblings<'s> {
    stock: &'s StockIni,
    /// Namesake stem -> text of its `item_Name*`/`vehicle_Name*` key.
    names: HashMap<String, Cow<'s, str>>,
}

impl<'s> Siblings<'s> {
    fn new(stock: &'s StockIni) -> Self {
        let mut names = HashMap::new();
        for (_, key, value) in stock.entries() {
            let lower = key.to_ascii_lowercase();
            if !(lower.starts_with("item_name") || lower.starts_with("vehicle_name"))
                || lower.starts_with("item_nameshort")
            {
                continue;
            }
            if let (Some(stem), Some(v)) = (namesake::stem(strip_flag_suffix(key)), as_name(value))
            {
                names.entry(stem).or_insert(v);
            }
        }
        Self { stock, names }
    }

    fn named(&self, name: &str, field: Field) -> (Record, Field) {
        (
            Record {
                name: name.into(),
                kind: OwnerKind::Named,
                detail: None,
            },
            field,
        )
    }

    /// A variant without a name of its own (`_grey_red01`, `_store01`): the name of its base
    /// item with the suffix, `Gemini S71 Rifle (grey_red01)`. Up to three trailing tokens are
    /// dropped; the base keeps at least two.
    fn variant(&self, rest: &str, field: Field) -> Option<(Record, Field)> {
        let tokens: Vec<&str> = rest
            .trim_start_matches('_')
            .split('_')
            .filter(|t| !t.is_empty())
            .collect();
        (1..=3usize)
            .filter(|k| tokens.len() >= k + 2)
            .find_map(|k| {
                let (base, suffix) = tokens.split_at(tokens.len() - k);
                let name = self.names.get(&namesake::squash_name(&base.join("_")))?;
                Some(self.named(&format!("{name} ({})", suffix.join("_")), field))
            })
    }

    fn value(&self, folded: &str) -> Option<Cow<'s, str>> {
        self.stock.value(folded).and_then(as_name)
    }

    fn owner(&self, folded: &str, value: &str) -> Option<(Record, Field)> {
        // `item_*` / `vehicle_*` namesake keys.
        for (prefix, field) in [
            ("item_shortname", Field::ShortName),
            ("item_nameshort", Field::ShortName),
            ("item_name", Field::Name),
            ("vehicle_name", Field::Name),
            ("item_desc", Field::Description),
            ("vehicle_desc", Field::Description),
        ] {
            if !folded.starts_with(prefix) {
                continue;
            }
            // `item_NameX_short` / `vehicle_NameX_short`: the short name of X.
            let field = if field == Field::Name && folded.ends_with("_short") {
                Field::ShortName
            } else {
                field
            };
            let sibling = namesake::stem(folded).and_then(|s| self.names.get(&s));
            return match field {
                Field::Name => as_name(value).map(|v| self.named(&v, field)),
                Field::ShortName => sibling
                    .map(|v| v.as_ref())
                    .or(as_name(value).as_deref())
                    .map(|v| self.named(v, field)),
                _ => sibling
                    .map(|v| self.named(v, field))
                    .or_else(|| self.variant(&folded[prefix.len()..], field)),
            };
        }
        if let Some(code) = folded.strip_prefix("manufacturer_") {
            let (field, code) = match code.strip_prefix("name") {
                Some(c) => (Field::Name, c),
                None => (Field::Description, code.strip_prefix("desc")?),
            };
            let name = self.value(&format!("manufacturer_name{code}"))?;
            return Some((
                Record {
                    name: name.into(),
                    kind: OwnerKind::Manufacturer,
                    detail: None,
                },
                field,
            ));
        }
        // `X_desc` / `X_desc_N` and the key they describe.
        let desc = folded
            .strip_suffix("_desc")
            .map(|base| {
                [
                    base.to_string(),
                    format!("{base}_name"),
                    format!("{base}_title"),
                ]
            })
            .or_else(|| {
                let (a, b) = folded.rsplit_once("_desc_")?;
                Some([
                    format!("{a}_title_{b}"),
                    format!("{a}_name_{b}"),
                    format!("{a}_{b}"),
                ])
            });
        if let Some(cands) = desc {
            return cands
                .iter()
                .find_map(|k| self.value(k))
                .map(|v| self.named(&v, Field::Description));
        }
        let described = |k: String| self.stock.value(&k).is_some();
        let own = as_name(value)?;
        if let Some((a, b)) = folded.rsplit_once("_title_")
            && described(format!("{a}_desc_{b}"))
        {
            return Some(self.named(&own, Field::Title));
        }
        if let Some(base) = folded.strip_suffix("_title")
            && described(format!("{base}_desc"))
        {
            return Some(self.named(&own, Field::Title));
        }
        if let Some(base) = folded.strip_suffix("_name")
            && described(format!("{base}_desc"))
        {
            return Some(self.named(&own, Field::Name));
        }
        if described(format!("{folded}_desc")) {
            return Some(self.named(&own, Field::Name));
        }
        None
    }
}

/// Field words a key prefix may carry (`item_Desc...`, `item_name_...`).
const FIELD_WORDS: &[&str] = &[
    "name",
    "desc",
    "description",
    "shortname",
    "nameshort",
    "short",
    "title",
];

/// A key readable without an owner: `item_DescCOOL_AEGS_S01_Bracer` ->
/// `item desc · COOL_AEGS_S01_Bracer`, `hud_law_TrespassArea` -> `hud · law trespass area`.
/// The prefix and its field word are lower case; a rest that holds a record-style code
/// (an all-caps token) stays as written, any other rest becomes lower-case words.
pub fn pretty_key(key: &str) -> String {
    let key = strip_flag_suffix(key.strip_prefix('@').unwrap_or(key));
    let Some((head, rest)) = key.split_once('_') else {
        return words(key);
    };
    let mut prefix = head.to_ascii_lowercase();
    let mut rest = rest;
    // `DescCOOL...` / `Name_...` / `desc_...`: a field word right after the prefix.
    let lead: usize = rest
        .char_indices()
        .skip(1)
        .find(|(_, c)| !c.is_ascii_lowercase())
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    let word = &rest[..lead];
    if !word.is_empty() && FIELD_WORDS.contains(&word.to_ascii_lowercase().as_str()) {
        let after = rest[lead..].chars().next();
        if after.is_none_or(|c| c == '_' || c.is_ascii_uppercase() || c.is_ascii_digit()) {
            prefix.push(' ');
            prefix.push_str(&word.to_ascii_lowercase());
            rest = rest[lead..].trim_start_matches('_');
        }
    }
    if rest.is_empty() {
        return prefix;
    }
    let coded = rest.split('_').any(|t| {
        t.len() >= 2
            && t.chars().any(|c| c.is_ascii_uppercase())
            && !t.chars().any(|c| c.is_ascii_lowercase())
    });
    let rest = if coded { rest.to_string() } else { words(rest) };
    format!("{prefix} · {rest}")
}

/// `law_TrespassAreaRevoked` -> `law trespass area revoked`; acronyms (`CI`) stay.
fn words(s: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for token in s.split('_').filter(|t| !t.is_empty()) {
        let chars: Vec<char> = token.chars().collect();
        let mut cur = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1).copied();
            let boundary = c.is_ascii_uppercase()
                && prev.is_some_and(|p| {
                    p.is_ascii_lowercase()
                        || p.is_ascii_digit()
                        || (p.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()))
                });
            if boundary && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out.into_iter()
        .map(|w| {
            let caps = w.chars().filter(|c| c.is_ascii_uppercase()).count();
            if caps >= 2 && !w.chars().any(|c| c.is_ascii_lowercase()) {
                w
            } else {
                w.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "owners_tests.rs"]
mod tests;
