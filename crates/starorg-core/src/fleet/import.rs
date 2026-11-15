//! Fleet import (addendum §6.3): HangarXPLOR JSON (RSI+ "Export fleet", FleetYards,
//! myfleet.gg / FleetView, Starjump / hangar.link), HangarXPLOR CSV and plain text, one
//! importer with the format auto-detected, rows matched to the hulls of a build.
//!
//! Only the ship name, custom name, maker code and ship code of a row are read; pledge ids,
//! names, costs, dates, LTI, warbond, insurance and how a ship was acquired are never kept.

use super::catalog::{Catalog, Hull};
use super::store::{FleetFile, OwnedHull, PendingRow};
use super::{has_mk, maker_code, normalize, squash, strip_maker, strip_mk, word_set};
use crate::ini::fold_key;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Largest file read (5 MB).
pub const IMPORT_LIMIT: u64 = 5 << 20;
/// Most rows read.
pub const MAX_ROWS: usize = 5_000;
/// Hand-kept store names -> hull ids (§6.3 tier 2).
pub const ALIASES_TOML: &str = include_str!("../../data/fleet-aliases.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    XplorJson,
    XplorCsv,
    Text,
}

impl Format {
    pub fn name(&self) -> &'static str {
        match self {
            Self::XplorJson => "xplor-json",
            Self::XplorCsv => "xplor-csv",
            Self::Text => "text",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "xplor-json" => Some(Self::XplorJson),
            "xplor-csv" => Some(Self::XplorCsv),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

/// One ship of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Ship name as the file has it.
    pub name: String,
    /// Custom name (differs from the ship name after normalisation).
    pub custom: Option<String>,
    /// Maker code or name as the file has it.
    pub maker: Option<String>,
    pub code: Option<String>,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub format: Format,
    pub rows: Vec<Row>,
    /// Rows of the file (ships and others).
    pub total: usize,
    /// Rows that are not ships (`entity_type` other than `ship`).
    pub skipped: usize,
}

const NOT_FLEET: &str = "not a fleet export";

/// Detect the format: `[` (or `{`) first = HangarXPLOR JSON; a first line with the columns
/// `Manufacturer` and `Ship` = HangarXPLOR CSV; anything else = plain text.
pub fn detect(text: &str) -> Format {
    let t = text.trim_start();
    // `{` too, so a JSON object is refused as "not a fleet export" instead of read as text.
    if t.starts_with('[') || t.starts_with('{') {
        return Format::XplorJson;
    }
    let first = t.lines().next().unwrap_or_default();
    let cols: Vec<String> = csv_record(first)
        .into_iter()
        .map(|c| c.trim().to_ascii_lowercase())
        .collect();
    if cols.iter().any(|c| c == "manufacturer") && cols.iter().any(|c| c == "ship") {
        Format::XplorCsv
    } else {
        Format::Text
    }
}

/// Read a fleet file. `format = None` detects it.
pub fn parse(bytes: &[u8], format: Option<Format>) -> Result<Parsed, String> {
    if bytes.len() as u64 > IMPORT_LIMIT {
        return Err("the file is larger than 5 MB".into());
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| "the file is not UTF-8 text".to_string())?;
    let format = format.unwrap_or_else(|| detect(text));
    let p = match format {
        Format::XplorJson => parse_json(text)?,
        Format::XplorCsv => parse_csv(text)?,
        Format::Text => parse_text(text)?,
    };
    if p.rows.is_empty() {
        return Err(match format {
            Format::Text => "the list names no ship".into(),
            _ => NOT_FLEET.into(),
        });
    }
    Ok(p)
}

fn too_many() -> String {
    format!("more than {MAX_ROWS} rows")
}

fn custom_of(ship_name: &str, name: &str) -> Option<String> {
    let c = ship_name.trim();
    (!c.is_empty() && normalize(c) != normalize(name)).then(|| c.to_string())
}

fn parse_json(text: &str) -> Result<Parsed, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("{NOT_FLEET}: {e}"))?;
    let arr = v.as_array().ok_or_else(|| NOT_FLEET.to_string())?;
    if arr.len() > MAX_ROWS {
        return Err(too_many());
    }
    let mut rows = Vec::new();
    let mut skipped = 0;
    for item in arr {
        let Some(obj) = item.as_object() else {
            skipped += 1;
            continue;
        };
        // Field names ignoring case and `_` (`shipCode` = `ship_code`).
        let field = |want: &str| -> Option<String> {
            obj.iter()
                .find(|(k, _)| k.replace('_', "").eq_ignore_ascii_case(want))
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        if field("entitytype").is_some_and(|t| !t.eq_ignore_ascii_case("ship")) {
            skipped += 1;
            continue;
        }
        let ship_name = field("shipname");
        let Some(name) = field("name").or_else(|| ship_name.clone()) else {
            skipped += 1;
            continue;
        };
        rows.push(Row {
            custom: ship_name.as_deref().and_then(|s| custom_of(s, &name)),
            name,
            maker: field("manufacturercode").or_else(|| field("manufacturername")),
            code: field("shipcode"),
            count: 1,
        });
    }
    Ok(Parsed {
        format: Format::XplorJson,
        total: arr.len(),
        skipped,
        rows,
    })
}

/// One CSV record: quoted fields with `""` escapes, unquoted fields as they are.
fn csv_record(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if cur.trim().is_empty() => {
                cur.clear();
                quoted = true;
            }
            (',', false) => out.push(std::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// CSV records, honouring line breaks inside quotes; CRLF or LF.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let mut recs = Vec::new();
    let mut buf = String::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(line);
        if buf.matches('"').count().is_multiple_of(2) {
            if !buf.trim().is_empty() {
                recs.push(csv_record(&buf));
            }
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        recs.push(csv_record(&buf));
    }
    recs
}

fn parse_csv(text: &str) -> Result<Parsed, String> {
    let recs = csv_records(text);
    let Some((head, body)) = recs.split_first() else {
        return Err(NOT_FLEET.into());
    };
    let col = |n: &str| head.iter().position(|c| c.trim().eq_ignore_ascii_case(n));
    let (Some(mc), Some(sc)) = (col("manufacturer"), col("ship")) else {
        return Err(format!("{NOT_FLEET}: no Manufacturer and Ship columns"));
    };
    if body.len() > MAX_ROWS {
        return Err(too_many());
    }
    let mut rows = Vec::new();
    let mut skipped = 0;
    for r in body {
        let get = |i: usize| {
            r.get(i)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        match get(sc) {
            Some(name) => rows.push(Row {
                name,
                custom: None,
                maker: get(mc),
                code: None,
                count: 1,
            }),
            None => skipped += 1,
        }
    }
    Ok(Parsed {
        format: Format::XplorCsv,
        total: body.len(),
        skipped,
        rows,
    })
}

/// `2x Gladius`, `2 x Gladius`, `Gladius x2`, `Gladius ×2` -> (2, "Gladius").
fn text_count(line: &str) -> (u32, &str) {
    let num = |s: &str| s.parse::<u32>().ok().filter(|n| (1..=999).contains(n));
    let lead = line
        .split_once(['x', 'X', '×'])
        .and_then(|(n, rest)| Some((num(n.trim())?, rest)))
        .filter(|(_, rest)| rest.starts_with(' '));
    if let Some((n, rest)) = lead {
        return (n, rest.trim());
    }
    if let Some((rest, n)) = line.rsplit_once(' ') {
        let n = n.trim_start_matches(['x', 'X', '×']);
        let marked = line.len() - rest.len() - 1 > n.len();
        if marked && let Some(n) = num(n) {
            return (n, rest.trim());
        }
    }
    (1, line)
}

fn parse_text(text: &str) -> Result<Parsed, String> {
    let mut rows = Vec::new();
    let mut total = 0;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        total += 1;
        if total > MAX_ROWS {
            return Err(too_many());
        }
        let (count, name) = text_count(l);
        if !name.is_empty() {
            rows.push(Row {
                name: name.to_string(),
                custom: None,
                maker: None,
                code: None,
                count,
            });
        }
    }
    Ok(Parsed {
        format: Format::Text,
        total,
        skipped: 0,
        rows,
    })
}

// ---------------------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------------------

/// Which tier matched (§6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Saved,
    Alias,
    Exact,
    Mk,
    WordOrder,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    Hull(String, Tier),
    /// A saved "Not in the game".
    NotInGame,
    Unmatched,
    Ambiguous(Vec<String>),
}

/// The alias table: normalised store name -> hull id.
pub fn aliases() -> BTreeMap<String, String> {
    let t: toml::Table = toml::from_str(ALIASES_TOML).unwrap_or_default();
    t.get("aliases")
        .and_then(|a| a.as_table())
        .map(|a| {
            a.iter()
                .filter_map(|(k, v)| Some((normalize(k), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

pub struct Matcher<'a> {
    catalog: &'a Catalog,
    aliases: BTreeMap<String, String>,
    saved: &'a BTreeMap<String, Option<String>>,
    /// Per flyable hull: its normalised name forms.
    forms: Vec<(&'a Hull, Vec<String>)>,
}

fn hull_suffix(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    squash(lower.strip_prefix("vehicle_name").unwrap_or(&lower))
}

impl<'a> Matcher<'a> {
    pub fn new(
        catalog: &'a Catalog,
        aliases: BTreeMap<String, String>,
        saved: &'a BTreeMap<String, Option<String>>,
    ) -> Self {
        let forms = catalog
            .flyable()
            .map(|h| {
                let long = normalize(&h.name);
                let mut f = vec![long.clone()];
                f.extend(strip_maker(&long));
                f.extend(h.short.as_deref().map(normalize).filter(|s| !s.is_empty()));
                f.dedup();
                (h, f)
            })
            .collect();
        Self {
            catalog,
            aliases,
            saved,
            forms,
        }
    }

    fn known(&self, id: &str) -> Option<String> {
        self.catalog
            .get(id)
            .filter(|h| h.flyable)
            .map(|h| h.hull.clone())
    }

    fn pick(&self, row: &Row, hits: Vec<&'a Hull>, tier: Tier) -> Option<Match> {
        let mut hits = hits;
        hits.sort_by(|a, b| a.hull.cmp(&b.hull));
        hits.dedup_by(|a, b| a.hull == b.hull);
        match hits.len() {
            0 => return None,
            1 => return Some(Match::Hull(hits[0].hull.clone(), tier)),
            _ => {}
        }
        // Tie-breaks: maker, then ship code, then the namesake among same display names.
        if let Some(m) = row.maker.as_deref().and_then(maker_code) {
            let same: Vec<&Hull> = hits
                .iter()
                .copied()
                .filter(|h| h.maker.as_deref() == Some(m))
                .collect();
            if !same.is_empty() {
                hits = same;
            }
        }
        if hits.len() > 1
            && let Some(code) = row.code.as_deref().map(squash)
        {
            let by: Vec<&Hull> = hits
                .iter()
                .copied()
                .filter(|h| hull_suffix(&h.hull) == code)
                .collect();
            if !by.is_empty() {
                hits = by;
            }
        }
        if hits.len() > 1 {
            let first = normalize(&hits[0].name);
            if hits.iter().all(|h| normalize(&h.name) == first) {
                let ns: Vec<&Hull> = hits.iter().copied().filter(|h| h.namesake).collect();
                if ns.len() == 1 {
                    hits = ns;
                }
            }
        }
        Some(if hits.len() == 1 {
            Match::Hull(hits[0].hull.clone(), tier)
        } else {
            Match::Ambiguous(hits.iter().map(|h| h.hull.clone()).collect())
        })
    }

    /// Match one row (§6.3 tiers, first tier with a result wins).
    pub fn find(&self, row: &Row) -> Match {
        let key = normalize(&row.name);
        if let Some(saved) = self.saved.get(&key) {
            match saved {
                None => return Match::NotInGame,
                Some(id) => {
                    if let Some(h) = self.known(id) {
                        return Match::Hull(h, Tier::Saved);
                    }
                }
            }
        }
        if let Some(h) = self.aliases.get(&key).and_then(|id| self.known(id)) {
            return Match::Hull(h, Tier::Alias);
        }
        let mut row_forms = vec![key.clone()];
        row_forms.extend(strip_maker(&key));
        type Form<'f> = &'f dyn Fn(&str) -> String;
        let tiers: [(Tier, Form<'_>); 2] = [
            (Tier::Exact, &|s: &str| s.to_string()),
            (Tier::Mk, &strip_mk),
        ];
        for (tier, f) in tiers {
            let rf: Vec<String> = row_forms.iter().map(|s| f(s)).collect();
            let hits: Vec<&Hull> = self
                .forms
                .iter()
                .filter(|(_, hf)| hf.iter().any(|x| rf.contains(&f(x))))
                .map(|(h, _)| *h)
                .collect();
            if tier == Tier::Mk && !has_mk(&key) && hits.len() > 1 {
                // No Mk in the row and both Mk I and Mk II fit: never guessed.
                let mks: Vec<&Hull> = hits
                    .iter()
                    .copied()
                    .filter(|h| has_mk(&normalize(&h.name)))
                    .collect();
                if mks.len() > 1 {
                    return Match::Ambiguous({
                        let mut v: Vec<String> = mks.iter().map(|h| h.hull.clone()).collect();
                        v.sort();
                        v.dedup();
                        v
                    });
                }
            }
            if let Some(m) = self.pick(row, hits, tier) {
                return m;
            }
        }
        let rw: Vec<Vec<String>> = row_forms.iter().map(|s| word_set(s)).collect();
        let hits: Vec<&Hull> = self
            .forms
            .iter()
            .filter(|(_, hf)| hf.iter().any(|x| rw.contains(&word_set(x))))
            .map(|(h, _)| *h)
            .collect();
        if let Some(m) = self.pick(row, hits, Tier::WordOrder) {
            return m;
        }
        if let Some(code) = row.code.as_deref().map(squash).filter(|c| !c.is_empty()) {
            let hits: Vec<&Hull> = self
                .forms
                .iter()
                .map(|(h, _)| *h)
                .filter(|h| hull_suffix(&h.hull) == code)
                .collect();
            if let Some(m) = self.pick(row, hits, Tier::Code) {
                return m;
            }
        }
        Match::Unmatched
    }
}

// ---------------------------------------------------------------------------------------
// Report and fleet update
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportedHull {
    pub hull: String,
    pub name: String,
    pub count: u32,
    /// Custom names: rename suggestions.
    pub names: Vec<String>,
    /// Not in the fleet before this import.
    pub new: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingReport {
    pub name: String,
    pub count: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub format: Format,
    pub total: usize,
    pub skipped: usize,
    pub hulls: Vec<ImportedHull>,
    pub unmatched: Vec<PendingReport>,
    pub ambiguous: Vec<PendingReport>,
    /// Rows the user marked "Not in the game" earlier.
    pub not_in_game: usize,
    /// How many rows each tier matched.
    pub tiers: BTreeMap<Tier, usize>,
    /// The channel's game data has not been read: nothing could match.
    pub empty_catalog: bool,
    pub merge: bool,
}

/// The result of matching a file: the report plus the fleet it would leave.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub report: ImportReport,
    pub fleet: FleetFile,
    /// Hulls new to the fleet, in file order (appended to the order list).
    pub added: Vec<String>,
}

/// Match `parsed` against `catalog` and compute the fleet after the import. `merge`: add to
/// the fleet (count = the larger) instead of replacing it. Hand matches survive either way.
pub fn run(parsed: &Parsed, catalog: &Catalog, fleet: &FleetFile, merge: bool) -> Outcome {
    let matcher = Matcher::new(catalog, aliases(), &fleet.matches);
    let mut hulls: Vec<OwnedHull> = Vec::new();
    let mut pending: Vec<PendingRow> = Vec::new();
    let mut tiers: BTreeMap<Tier, usize> = BTreeMap::new();
    let mut not_in_game = 0;
    let push_name = |names: &mut Vec<String>, n: &Option<String>| {
        if let Some(n) = n
            && !names.iter().any(|x| normalize(x) == normalize(n))
        {
            names.push(n.clone());
        }
    };
    for row in &parsed.rows {
        match matcher.find(row) {
            Match::Hull(h, tier) => {
                *tiers.entry(tier).or_default() += 1;
                let f = fold_key(&h);
                let e = match hulls.iter().position(|x| fold_key(&x.hull) == f) {
                    Some(i) => &mut hulls[i],
                    None => {
                        hulls.push(OwnedHull {
                            hull: h,
                            count: 0,
                            names: Vec::new(),
                            extra: Default::default(),
                        });
                        hulls.last_mut().expect("just pushed")
                    }
                };
                e.count += row.count;
                push_name(&mut e.names, &row.custom);
            }
            Match::NotInGame => not_in_game += 1,
            m => {
                let candidates = match m {
                    Match::Ambiguous(c) => c,
                    _ => Vec::new(),
                };
                let key = normalize(&row.name);
                let e = match pending.iter().position(|p| normalize(&p.name) == key) {
                    Some(i) => &mut pending[i],
                    None => {
                        pending.push(PendingRow {
                            name: row.name.clone(),
                            maker: row.maker.clone(),
                            code: row.code.clone(),
                            count: 0,
                            names: Vec::new(),
                            candidates,
                            extra: Default::default(),
                        });
                        pending.last_mut().expect("just pushed")
                    }
                };
                e.count += row.count;
                push_name(&mut e.names, &row.custom);
            }
        }
    }
    let mut out = fleet.clone();
    let before: Vec<String> = fleet.hulls.iter().map(|h| fold_key(&h.hull)).collect();
    let added: Vec<String> = hulls
        .iter()
        .filter(|h| !before.contains(&fold_key(&h.hull)))
        .map(|h| h.hull.clone())
        .collect();
    let report_hulls = hulls
        .iter()
        .map(|h| ImportedHull {
            name: catalog
                .get(&h.hull)
                .map(|c| c.name.trim().to_string())
                .unwrap_or_default(),
            hull: h.hull.clone(),
            count: h.count,
            names: h.names.clone(),
            new: added.contains(&h.hull),
        })
        .collect();
    let pr = |p: &PendingRow| PendingReport {
        name: p.name.clone(),
        count: p.count,
        candidates: p.candidates.clone(),
    };
    let unmatched = pending
        .iter()
        .filter(|p| p.candidates.is_empty())
        .map(pr)
        .collect();
    let ambiguous = pending
        .iter()
        .filter(|p| !p.candidates.is_empty())
        .map(pr)
        .collect();
    if merge {
        for h in hulls {
            let f = fold_key(&h.hull);
            match out.hulls.iter_mut().find(|x| fold_key(&x.hull) == f) {
                Some(x) => {
                    x.count = x.count.max(h.count);
                    for n in &h.names {
                        push_name(&mut x.names, &Some(n.clone()));
                    }
                }
                None => out.hulls.push(h),
            }
        }
        for p in pending {
            let k = normalize(&p.name);
            match out.pending.iter_mut().find(|x| normalize(&x.name) == k) {
                Some(x) => {
                    x.count = x.count.max(p.count);
                    x.candidates = p.candidates;
                }
                None => out.pending.push(p),
            }
        }
    } else {
        // Keep unknown keys of hulls that stay.
        for h in &mut hulls {
            if let Some(old) = fleet.owned(&h.hull) {
                h.extra = old.extra.clone();
            }
        }
        out.hulls = hulls;
        out.pending = pending;
    }
    out.format = Some(parsed.format.name().to_string());
    Outcome {
        report: ImportReport {
            format: parsed.format,
            total: parsed.total,
            skipped: parsed.skipped,
            hulls: report_hulls,
            unmatched,
            ambiguous,
            not_in_game,
            tiers,
            empty_catalog: catalog.flyable().next().is_none(),
            merge,
        },
        fleet: out,
        added,
    }
}

/// Hand match (`fleet match`, the review list): a pending row name -> a hull, or `None` for
/// "Not in the game". The pending row becomes an owned hull (count and custom names kept).
/// Returns the hull when one was added to the fleet.
pub fn hand_match(fleet: &mut FleetFile, name: &str, hull: Option<&Hull>) -> Option<String> {
    let key = normalize(name);
    fleet
        .matches
        .insert(key.clone(), hull.map(|h| h.hull.clone()));
    let i = fleet
        .pending
        .iter()
        .position(|p| normalize(&p.name) == key)?;
    let p = fleet.pending.remove(i);
    let h = hull?;
    let f = fold_key(&h.hull);
    let mut names = p.names.clone();
    // CSV rows carry the custom name instead of the hull name: that becomes the suggestion.
    if p.maker.is_some() && p.code.is_none() && normalize(&p.name) != normalize(&h.name) {
        let stripped = strip_maker(&normalize(&h.name)).unwrap_or_default();
        if normalize(&p.name) != stripped && !names.iter().any(|n| normalize(n) == key) {
            names.push(p.name.clone());
        }
    }
    match fleet.hulls.iter_mut().find(|x| fold_key(&x.hull) == f) {
        Some(x) => {
            x.count += p.count;
            for n in names {
                if !x.names.iter().any(|m| normalize(m) == normalize(&n)) {
                    x.names.push(n);
                }
            }
            None
        }
        None => {
            fleet.hulls.push(OwnedHull {
                hull: h.hull.clone(),
                count: p.count.max(1),
                names,
                extra: Default::default(),
            });
            Some(h.hull.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hull(id: &str, name: &str, short: Option<&str>, maker: &str) -> Hull {
        Hull {
            hull: id.into(),
            name: name.into(),
            short: short.map(str::to_string),
            short_key: short.map(|_| format!("{id}_short")),
            maker: Some(maker.into()),
            flyable: true,
            ground: false,
            npc_shared: 0,
            namesake: true,
        }
    }

    fn catalog() -> Catalog {
        let mut hijacked = hull(
            "vehicle_NameORIG_890Jump_Hijacked",
            "Origin 890 Jump",
            None,
            "orig",
        );
        hijacked.namesake = false;
        let mut ai = hull(
            "vehicle_NameAEGS_Gladius_AI",
            "Aegis Gladius AI",
            None,
            "aegs",
        );
        ai.flyable = false;
        Catalog::new(vec![
            hull(
                "vehicle_NameAEGS_Gladius",
                "Aegis Gladius",
                Some("Gladius"),
                "aegs",
            ),
            hull(
                "vehicle_NameAEGS_Avenger_Titan",
                "Aegis Avenger Titan",
                Some("Avenger Titan"),
                "aegs",
            ),
            hull(
                "vehicle_NameANVL_Hornet_F7A",
                "Anvil F7A Hornet Mk I",
                Some("F7A Mk I"),
                "anvl",
            ),
            hull(
                "vehicle_NameANVL_Hornet_F7A_Mk2",
                "Anvil F7A Hornet Mk II",
                Some("F7A Mk II"),
                "anvl",
            ),
            hull(
                "vehicle_NameRSI_Constellation_Andromeda",
                "RSI Constellation Mk IV Andromeda",
                None,
                "rsi",
            ),
            hull(
                "vehicle_NameCRUS_Starlifter_C2",
                "Crusader C2 Hercules Starlifter",
                None,
                "crus",
            ),
            hull("vehicle_NameORIG_890Jump", "Origin 890 Jump", None, "orig"),
            hijacked,
            hull(
                "vehicle_NameMISC_Prospector",
                "MISC Prospector",
                None,
                "misc",
            ),
            hull(
                "vehicle_NameDRAK_Prospector",
                "Drake Prospector",
                None,
                "drak",
            ),
            hull("vehicle_NameRSI_Ursa_Rover", "RSI Ursa", None, "rsi"),
            hull(
                "vehicle_NameRSI_Ursa_Rover_Emerald",
                "RSI Ursa Fortuna",
                None,
                "rsi",
            ),
            hull(
                "vehicle_NameDRAK_Cutlass_Black",
                "Drake Cutlass Black",
                None,
                "drak",
            ),
            ai,
        ])
    }

    fn row(name: &str) -> Row {
        Row {
            name: name.into(),
            custom: None,
            maker: None,
            code: None,
            count: 1,
        }
    }

    fn find(name: &str) -> Match {
        let c = catalog();
        let saved = BTreeMap::new();
        Matcher::new(&c, aliases(), &saved).find(&row(name))
    }

    fn hit(id: &str, t: Tier) -> Match {
        Match::Hull(id.into(), t)
    }

    #[test]
    fn detects_each_format() {
        assert_eq!(detect("  [ {} ]"), Format::XplorJson);
        assert_eq!(
            detect("Manufacturer,Ship,Lti,Warbond,ID,Pledge,Cost,Date\n"),
            Format::XplorCsv
        );
        assert_eq!(detect("\"Manufacturer\",\"Ship\"\n"), Format::XplorCsv);
        assert_eq!(detect("Aegis Gladius\n"), Format::Text);
    }

    #[test]
    fn json_rows_skip_non_ships_and_drop_pledge_data() {
        let text = br#"[
          {"ship_code":"AEGS_Gladius","ship_name":"Needle","manufacturer_code":"AEGS","manufacturer_name":"Aegis Dynamics","name":"Gladius","lti":true,"warbond":false,"entity_type":"ship","pledge_id":"90000001","pledge_name":"Standalone Ship - Gladius","pledge_date":"January 01, 2950","pledge_cost":"$90.00 USD"},
          {"shipCode":"AEGS_Gladius","shipName":"Gladius","manufacturerCode":"AEGS","name":"Gladius","entityType":"ship"},
          {"name":"Paint","entity_type":"skin"},
          "junk"
        ]"#;
        let p = parse(text, None).unwrap();
        assert_eq!(p.format, Format::XplorJson);
        assert_eq!((p.total, p.skipped, p.rows.len()), (4, 2, 2));
        assert_eq!(p.rows[0].custom.as_deref(), Some("Needle"));
        assert_eq!(p.rows[1].custom, None);
        assert_eq!(p.rows[1].code.as_deref(), Some("AEGS_Gladius"));
        assert!(
            parse(br#"{"ships":[]}"#, None)
                .unwrap_err()
                .contains("not a fleet export")
        );
        assert!(
            parse(b"[]", None)
                .unwrap_err()
                .contains("not a fleet export")
        );
        assert!(parse(br#"[{"entity_type":"skin","name":"x"}]"#, None).is_err());
    }

    #[test]
    fn csv_with_quotes_bom_and_crlf() {
        let text = "\u{feff}Manufacturer,Ship,Lti,Warbond,ID,Pledge,Cost,Date\r\n\"Aegis Dynamics\",\"Gladius\",true,false,90000001,\"Pack, \"\"Big\"\"\",\"$90.00 USD\",\"January 01, 2950\"\r\nDrake Interplanetary,My \"\"Bucket\"\",false,false,90000002,x,y,z\r\n,,,\r\n";
        let p = parse(text.as_bytes(), None).unwrap();
        assert_eq!(p.format, Format::XplorCsv);
        assert_eq!((p.total, p.skipped), (3, 1));
        assert_eq!(p.rows[0].name, "Gladius");
        assert_eq!(p.rows[0].maker.as_deref(), Some("Aegis Dynamics"));
        assert_eq!(
            p.rows[1].name, "My \"\"Bucket\"\"",
            "unquoted fields as they are"
        );
        let rec = csv_record("\"a, b\",\"c \"\"d\"\"\",e");
        assert_eq!(rec, ["a, b", "c \"d\"", "e"]);
    }

    #[test]
    fn text_lines_with_counts_and_comments() {
        let p = parse(
            b"# my ships\n\n2x Gladius\nAegis Avenger Titan x2\n3 x Cutlass Black\nF7A Hornet Mk II\n890 Jump x\n",
            None,
        )
        .unwrap();
        assert_eq!(p.format, Format::Text);
        let got: Vec<(u32, &str)> = p.rows.iter().map(|r| (r.count, r.name.as_str())).collect();
        assert_eq!(
            got,
            [
                (2, "Gladius"),
                (2, "Aegis Avenger Titan"),
                (3, "Cutlass Black"),
                (1, "F7A Hornet Mk II"),
                (1, "890 Jump x")
            ]
        );
        assert!(parse(b"# nothing\n\n", None).is_err());
    }

    #[test]
    fn limits_and_encoding() {
        assert!(parse(&vec![b' '; (IMPORT_LIMIT + 1) as usize], None).is_err());
        assert!(parse(b"\xff\xfe", None).unwrap_err().contains("UTF-8"));
        let many = "Gladius\n".repeat(MAX_ROWS + 1);
        assert!(parse(many.as_bytes(), None).unwrap_err().contains("rows"));
        let ok = "Gladius\n".repeat(MAX_ROWS);
        assert_eq!(parse(ok.as_bytes(), None).unwrap().rows.len(), MAX_ROWS);
    }

    #[test]
    fn matching_tiers() {
        assert_eq!(
            find("Gladius"),
            hit("vehicle_NameAEGS_Gladius", Tier::Exact)
        );
        assert_eq!(
            find("Aegis Dynamics Gladius"),
            hit("vehicle_NameAEGS_Gladius", Tier::Exact)
        );
        assert_eq!(
            find("aegis  gladius!"),
            hit("vehicle_NameAEGS_Gladius", Tier::Exact)
        );
        assert_eq!(
            find("Constellation Andromeda"),
            hit("vehicle_NameRSI_Constellation_Andromeda", Tier::Mk)
        );
        assert_eq!(
            find("Hercules Starlifter C2"),
            hit("vehicle_NameCRUS_Starlifter_C2", Tier::WordOrder)
        );
        assert_eq!(
            find("F7A Hornet Mk II"),
            hit("vehicle_NameANVL_Hornet_F7A_Mk2", Tier::Exact)
        );
        assert_eq!(find("Kraken"), Match::Unmatched);
        // AI hulls are never offered.
        assert_eq!(find("Gladius AI"), Match::Unmatched);
        // Alias table: a store name that differs from the game name.
        assert_eq!(
            find("Ursa Rover Fortuna"),
            hit("vehicle_NameRSI_Ursa_Rover_Emerald", Tier::Alias)
        );
    }

    #[test]
    fn mk_without_number_is_ambiguous() {
        assert_eq!(
            find("F7A Hornet"),
            Match::Ambiguous(vec![
                "vehicle_NameANVL_Hornet_F7A".into(),
                "vehicle_NameANVL_Hornet_F7A_Mk2".into()
            ])
        );
    }

    #[test]
    fn tie_breaks() {
        let c = catalog();
        let saved = BTreeMap::new();
        let m = Matcher::new(&c, aliases(), &saved);
        // Two makers build a "Prospector": ambiguous without a maker, settled with one.
        assert!(matches!(m.find(&row("Prospector")), Match::Ambiguous(v) if v.len() == 2));
        let mut r = row("Prospector");
        r.maker = Some("MISC".into());
        assert_eq!(m.find(&r), hit("vehicle_NameMISC_Prospector", Tier::Exact));
        // Then the ship code.
        let mut r = row("Prospector");
        r.code = Some("DRAK_Prospector".into());
        assert_eq!(m.find(&r), hit("vehicle_NameDRAK_Prospector", Tier::Exact));
        // Same display name: the namesake of a player hull wins.
        assert_eq!(
            m.find(&row("890 Jump")),
            hit("vehicle_NameORIG_890Jump", Tier::Exact)
        );
        // Code tier on its own.
        let mut r = row("Avenger Titan Renegade Edition 2950");
        r.code = Some("AEGS_Avenger_Titan".into());
        assert_eq!(
            m.find(&r),
            hit("vehicle_NameAEGS_Avenger_Titan", Tier::Code)
        );
    }

    #[test]
    fn saved_matches_and_not_in_game() {
        let c = catalog();
        let mut saved = BTreeMap::new();
        saved.insert(
            "my bucket".to_string(),
            Some("vehicle_NameDRAK_Cutlass_Black".to_string()),
        );
        saved.insert("kraken".to_string(), None);
        saved.insert("gladius".to_string(), Some("vehicle_NameGone".to_string()));
        let m = Matcher::new(&c, aliases(), &saved);
        assert_eq!(
            m.find(&row("My Bucket")),
            hit("vehicle_NameDRAK_Cutlass_Black", Tier::Saved)
        );
        assert_eq!(m.find(&row("Kraken")), Match::NotInGame);
        // A saved target missing from the build: the other tiers still run.
        assert_eq!(
            m.find(&row("Gladius")),
            hit("vehicle_NameAEGS_Gladius", Tier::Exact)
        );
        // An alias whose target is not in the build is ignored.
        let mut al = BTreeMap::new();
        al.insert("gladius".to_string(), "vehicle_NameMissing".to_string());
        let empty = BTreeMap::new();
        let m = Matcher::new(&c, al, &empty);
        assert_eq!(
            m.find(&row("Gladius")),
            hit("vehicle_NameAEGS_Gladius", Tier::Exact)
        );
    }

    #[test]
    fn duplicates_collapse_and_custom_names_become_suggestions() {
        let p = parse(
            br#"[
            {"name":"Gladius","ship_name":"Needle","manufacturer_code":"AEGS","entity_type":"ship"},
            {"name":"Gladius","ship_name":"Gladius","manufacturer_code":"AEGS","entity_type":"ship"},
            {"name":"Gladius","ship_name":"needle","manufacturer_code":"AEGS","entity_type":"ship"},
            {"name":"Kraken","ship_name":"Big Boy","manufacturer_code":"DRAK","ship_code":"DRAK_Kraken","entity_type":"ship"},
            {"name":"F7A Hornet","manufacturer_code":"ANVL","entity_type":"ship"}
        ]"#,
            None,
        )
        .unwrap();
        let o = run(&p, &catalog(), &FleetFile::default(), false);
        assert_eq!(o.report.hulls.len(), 1);
        let g = &o.report.hulls[0];
        assert_eq!(
            (g.count, g.names.clone(), g.new),
            (3, vec!["Needle".to_string()], true)
        );
        assert_eq!(o.report.unmatched[0].name, "Kraken");
        assert_eq!(o.report.ambiguous[0].candidates.len(), 2);
        assert_eq!(o.fleet.pending.len(), 2);
        assert_eq!(o.fleet.pending[0].code.as_deref(), Some("DRAK_Kraken"));
        assert_eq!(o.added, ["vehicle_NameAEGS_Gladius"]);
        assert_eq!(o.fleet.format.as_deref(), Some("xplor-json"));
        assert!(!o.report.empty_catalog);
        assert!(
            run(&p, &Catalog::default(), &FleetFile::default(), false)
                .report
                .empty_catalog
        );
    }

    #[test]
    fn replace_merge_and_hand_matches() {
        let c = catalog();
        let mut fleet = FleetFile::default();
        let first = parse(b"2x Gladius\nKraken\nMy Bucket\n", None).unwrap();
        fleet = run(&first, &c, &fleet, false).fleet;
        assert_eq!(fleet.pending.len(), 2);
        let bucket = c.get("vehicle_NameDRAK_Cutlass_Black").unwrap();
        assert_eq!(
            hand_match(&mut fleet, "My Bucket", Some(bucket)).as_deref(),
            Some("vehicle_NameDRAK_Cutlass_Black")
        );
        assert_eq!(hand_match(&mut fleet, "kraken", None), None);
        assert!(fleet.pending.is_empty());
        assert_eq!(fleet.hulls.len(), 2);

        // Replace: the new file is the whole hangar; hand matches survive.
        let second = parse(b"Avenger Titan\nmy bucket\nKraken\n", None).unwrap();
        let o = run(&second, &c, &fleet, false);
        let ids: Vec<&str> = o.fleet.hulls.iter().map(|h| h.hull.as_str()).collect();
        assert_eq!(
            ids,
            [
                "vehicle_NameAEGS_Avenger_Titan",
                "vehicle_NameDRAK_Cutlass_Black"
            ]
        );
        assert_eq!(o.report.tiers.get(&Tier::Saved), Some(&1));
        assert_eq!(o.report.not_in_game, 1);
        assert!(o.fleet.pending.is_empty());

        // Merge: added, count = the larger.
        let third = parse(b"5x Gladius\nAvenger Titan\n", None).unwrap();
        let o = run(&third, &c, &fleet, true);
        let g = o.fleet.owned("vehicle_NameAEGS_Gladius").unwrap();
        assert_eq!(g.count, 5);
        assert_eq!(o.fleet.hulls.len(), 3);
        assert_eq!(o.added, ["vehicle_NameAEGS_Avenger_Titan"]);
    }

    #[test]
    fn csv_custom_name_becomes_a_suggestion_once_matched() {
        let c = catalog();
        let p = parse(
            b"Manufacturer,Ship\nDrake Interplanetary,Old Faithful\n",
            None,
        )
        .unwrap();
        let mut fleet = run(&p, &c, &FleetFile::default(), false).fleet;
        assert_eq!(
            fleet.pending[0].maker.as_deref(),
            Some("Drake Interplanetary")
        );
        hand_match(
            &mut fleet,
            "Old Faithful",
            c.get("vehicle_NameDRAK_Cutlass_Black"),
        );
        assert_eq!(fleet.hulls[0].names, ["Old Faithful"]);
    }

    #[test]
    fn alias_table_parses() {
        let a = aliases();
        assert!(!a.is_empty());
        for (k, v) in &a {
            assert_eq!(*k, normalize(k), "alias keys are normalised");
            assert!(v.to_ascii_lowercase().starts_with("vehicle_name"), "{v}");
        }
    }
}
