//! Patch files and the patch engine (spec 4.5, [ADR](../../../docs/decisions/own-patch-format.md)).
//!
//! A patch describes one CIG data bug and how we correct for it: data entries change a field
//! of one `GameData` record before generators run; loc entries change an output string after
//! generators run. The engine never fails a build because of a patch; every entry gets an
//! outcome.
//!
//! - **No ordering, no cycles.** Every loc entry reads its sources from one snapshot taken
//!   after generators and before any loc entry writes.
//! - **Guards never contain CIG's text**: a guard on prose is `expect_sha256` of the stock
//!   value.

use crate::appdirs::AppDirs;
use crate::build::{BuildInfo, GameVersion};
use crate::fsutil;
use crate::ini::{StockIni, fold_key};
use crate::model::GameData;
use crate::validate::Op;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Patch files larger than this are rejected (spec 11).
pub const PATCH_LIMIT: u64 = 256 * 1024;

/// Built-in patches, embedded at build time: `(file name, TOML text)`. The v1 patches
/// (`hockrow-p2m4-desc`, `hockrow-recoveritem-desc`) need record names confirmed by the M2
/// data mapping and are added then.
pub const BUILTIN: &[(&str, &str)] = &[];

/// Fields a data entry may change (spec 4.5). Items and ships: `name_key`, `short_name_key`,
/// `desc_key`; contracts: `title_key`, `description_key`.
pub const DATA_FIELDS: &[&str] = &[
    "name_key",
    "short_name_key",
    "desc_key",
    "title_key",
    "description_key",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Builds {
    #[serde(default)]
    pub game_min: String,
    #[serde(default)]
    pub game_max: String,
    #[serde(default)]
    pub change_min: u64,
    #[serde(default)]
    pub change_max: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataEntry {
    pub id: String,
    #[serde(default)]
    pub record: Option<String>,
    #[serde(default)]
    pub guid: Option<String>,
    #[serde(default)]
    pub contract: Option<String>,
    pub field: String,
    pub expect: String,
    pub set: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocOp {
    Set,
    Append,
    Prepend,
    Replace,
    AppendFrom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FromPart {
    #[default]
    Generated,
    Stock,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocEntry {
    #[serde(default)]
    pub id: Option<String>,
    pub key: String,
    pub op: LocOp,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub find: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub from_part: FromPart,
    #[serde(default)]
    pub separator: String,
    #[serde(default)]
    pub requires: Option<String>,
    #[serde(default)]
    pub expect_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchFile {
    pub schema: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub builds: Builds,
    #[serde(default)]
    pub data: Vec<DataEntry>,
    #[serde(default)]
    pub loc: Vec<LocEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PatchOrigin {
    Builtin,
    User { path: PathBuf },
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedPatch {
    pub file: PatchFile,
    pub origin: PatchOrigin,
    pub sha256: String,
    pub enabled: bool,
}

/// A patch file that could not be used. It is skipped with a warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PatchFileError {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PatchSet {
    pub patches: Vec<LoadedPatch>,
    pub errors: Vec<PatchFileError>,
}

/// Parse and check one patch file.
pub fn parse_patch(name: &str, text: &str) -> Result<PatchFile, String> {
    let p: PatchFile = toml::from_str(text).map_err(|e| e.to_string())?;
    if p.schema != 1 {
        return Err(format!("unsupported schema {}", p.schema));
    }
    if p.id.is_empty()
        || !p
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("id must be non-empty and use only letters, digits, - and _".into());
    }
    if let Some(stem) = name.strip_suffix(".patch.toml")
        && stem != p.id
    {
        return Err(format!("file name {name} does not match id {}", p.id));
    }
    for v in [&p.builds.game_min, &p.builds.game_max] {
        if !v.is_empty() && GameVersion::from_branch(v).is_none() {
            return Err(format!("builds: {v:?} is not a game version"));
        }
    }
    let mut data_ids = Vec::new();
    for d in &p.data {
        if !DATA_FIELDS.contains(&d.field.as_str()) {
            return Err(format!(
                "data {}: field {} is not one of {DATA_FIELDS:?}",
                d.id, d.field
            ));
        }
        if d.record.is_none() == d.guid.is_none() {
            return Err(format!("data {}: give exactly one of record or guid", d.id));
        }
        data_ids.push(d.id.as_str());
    }
    for (i, l) in p.loc.iter().enumerate() {
        let name = l.id.clone().unwrap_or_else(|| format!("loc[{i}]"));
        let need = |what: &str, v: &Option<String>| {
            v.as_ref()
                .map(|_| ())
                .ok_or_else(|| format!("{name}: op {:?} needs {what}", l.op))
        };
        match l.op {
            LocOp::Set | LocOp::Append | LocOp::Prepend => need("value", &l.value)?,
            LocOp::Replace => {
                need("find", &l.find)?;
                need("value", &l.value)?;
                if l.find.as_deref() == Some("") {
                    return Err(format!("{name}: find must not be empty"));
                }
            }
            LocOp::AppendFrom => {
                need("from", &l.from)?;
                if l.requires.is_none() && l.expect_sha256.is_none() {
                    return Err(format!(
                        "{name}: append_from must carry requires or expect_sha256"
                    ));
                }
            }
        }
        if let Some(r) = &l.requires
            && !data_ids.contains(&r.as_str())
        {
            return Err(format!(
                "{name}: requires {r}, which is not a data entry in this file"
            ));
        }
    }
    Ok(p)
}

/// Load built-in and user patches; mark the ones the profile disables.
pub fn load_patches(dirs: &AppDirs, disabled: &[String]) -> PatchSet {
    let mut set = PatchSet::default();
    let add = |name: &str, text: &str, origin: PatchOrigin, set: &mut PatchSet| match parse_patch(
        name, text,
    ) {
        Ok(file) => {
            if set.patches.iter().any(|p| p.file.id == file.id) {
                set.errors.push(PatchFileError {
                    name: name.into(),
                    error: format!("duplicate patch id {}", file.id),
                });
                return;
            }
            let enabled = !disabled.contains(&file.id);
            set.patches.push(LoadedPatch {
                sha256: fsutil::sha256_hex(text.as_bytes()),
                file,
                origin,
                enabled,
            });
        }
        Err(error) => set.errors.push(PatchFileError {
            name: name.into(),
            error,
        }),
    };
    for (name, text) in BUILTIN {
        add(name, text, PatchOrigin::Builtin, &mut set);
    }
    let dir = dirs.user_patches();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().ends_with(".patch.toml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    for path in files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match fsutil::read_limited(&path, PATCH_LIMIT) {
            Ok(Some(b)) => match String::from_utf8(b) {
                Ok(text) => add(
                    &name,
                    &text,
                    PatchOrigin::User { path: path.clone() },
                    &mut set,
                ),
                Err(_) => set.errors.push(PatchFileError {
                    name,
                    error: "not valid UTF-8".into(),
                }),
            },
            Ok(None) => {}
            Err(e) => set.errors.push(PatchFileError {
                name,
                error: e.to_string(),
            }),
        }
    }
    set
}

impl PatchSet {
    /// Hash of ids, content and enabled state; part of the applied manifest.
    pub fn hash(&self) -> String {
        let mut parts: Vec<String> = self
            .patches
            .iter()
            .map(|p| format!("{}:{}:{}", p.file.id, p.sha256, p.enabled))
            .collect();
        parts.extend(
            self.errors
                .iter()
                .map(|e| format!("error:{}:{}", e.name, e.error)),
        );
        parts.sort();
        fsutil::sha256_hex(parts.join("\n").as_bytes())
    }
}

/// Outcome of one entry (spec 4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryState {
    Applies,
    NotNeeded,
    TargetChanged,
    NothingToAdd,
    Missing,
    OutOfRange,
    Conflict,
    /// Switched off in the profile.
    Disabled,
}

impl EntryState {
    /// States that make an apply "applied with warnings".
    pub fn warns(self) -> bool {
        matches!(
            self,
            EntryState::TargetChanged | EntryState::Missing | EntryState::Conflict
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryOutcome {
    pub patch: String,
    /// `data:<id>` or `loc:<id or index>`.
    pub entry: String,
    pub state: EntryState,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// Per-patch summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PatchOutcome {
    pub id: String,
    /// `applies` if any entry applies; `retired` when all are `not-needed`; otherwise the
    /// first warning state, or the common state.
    pub state: String,
}

/// What the loc engine reads: one snapshot after generators, before any loc entry writes.
pub trait Snapshot {
    fn stock(&self) -> &StockIni;
    /// What our generators added to the line (empty when nothing).
    fn generated_part(&self, line: usize) -> String;
    /// Value after generators (stock when nothing was generated).
    fn final_value(&self, line: usize) -> String;
}

/// One loc edit the engine wants to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocEdit {
    pub line: usize,
    pub patch: String,
    pub value: String,
    pub op: Op,
    pub added: Vec<String>,
}

fn in_range(b: &Builds, build: Option<&BuildInfo>) -> (bool, Option<String>) {
    let Some(build) = build else {
        return (
            true,
            Some("build unknown: build ranges are not enforced".into()),
        );
    };
    let mut note = None;
    match build.game_version {
        Some(v) => {
            if let Some(min) = GameVersion::from_branch(&b.game_min)
                && v < min
            {
                return (false, None);
            }
            if let Some(max) = GameVersion::from_branch(&b.game_max)
                && v > max
            {
                return (false, None);
            }
        }
        None if !b.game_min.is_empty() || !b.game_max.is_empty() => {
            note = Some("game version unknown: build ranges are not enforced".into());
        }
        None => {}
    }
    if let Some(c) = build.change {
        if b.change_min != 0 && c < b.change_min {
            return (false, note);
        }
        if b.change_max != 0 && c > b.change_max {
            return (false, note);
        }
    }
    (true, note)
}

/// Look up a data entry's field in `GameData`. M1's `GameData` holds no records yet (M2a), so
/// every data entry is `missing` until the data mapping lands.
fn data_field(_gd: &GameData, _entry: &DataEntry) -> Option<String> {
    None
}

/// Run every enabled patch. Data entries are evaluated against `GameData` (as an overlay; the
/// cache is never changed); loc entries produce [`LocEdit`]s from the snapshot.
pub fn run(
    set: &PatchSet,
    game_data: &GameData,
    snap: &dyn Snapshot,
    build: Option<&BuildInfo>,
) -> (Vec<LocEdit>, Vec<EntryOutcome>, Vec<String>) {
    let stock = snap.stock();
    let mut edits = Vec::new();
    let mut outcomes = Vec::new();
    let mut notes = Vec::new();

    // Conflicts: two enabled, in-range loc entries writing the same key.
    let mut writers: HashMap<String, usize> = HashMap::new();
    for p in set.patches.iter().filter(|p| p.enabled) {
        if !in_range(&p.file.builds, build).0 {
            continue;
        }
        for l in &p.file.loc {
            *writers.entry(fold_key(&l.key)).or_default() += 1;
        }
    }

    for p in &set.patches {
        let pid = p.file.id.clone();
        let out = |entry: String, state: EntryState, detail: String| EntryOutcome {
            patch: pid.clone(),
            entry,
            state,
            detail,
        };
        let loc_name = |i: usize, l: &LocEntry| {
            format!("loc:{}", l.id.clone().unwrap_or_else(|| i.to_string()))
        };
        if !p.enabled {
            for d in &p.file.data {
                outcomes.push(out(
                    format!("data:{}", d.id),
                    EntryState::Disabled,
                    String::new(),
                ));
            }
            for (i, l) in p.file.loc.iter().enumerate() {
                outcomes.push(out(loc_name(i, l), EntryState::Disabled, String::new()));
            }
            continue;
        }
        let (ok, note) = in_range(&p.file.builds, build);
        if let Some(n) = note {
            notes.push(format!("{pid}: {n}"));
        }
        if !ok {
            for d in &p.file.data {
                outcomes.push(out(
                    format!("data:{}", d.id),
                    EntryState::OutOfRange,
                    String::new(),
                ));
            }
            for (i, l) in p.file.loc.iter().enumerate() {
                outcomes.push(out(loc_name(i, l), EntryState::OutOfRange, String::new()));
            }
            continue;
        }

        let mut data_states: BTreeMap<&str, EntryState> = BTreeMap::new();
        for d in &p.file.data {
            let (state, detail) = match data_field(game_data, d) {
                None => (
                    EntryState::Missing,
                    "record or field not found in GameData".to_string(),
                ),
                Some(cur) if fold_key(&cur) == fold_key(&d.set) => {
                    (EntryState::NotNeeded, String::new())
                }
                Some(cur) if fold_key(&cur) == fold_key(&d.expect) => {
                    (EntryState::Applies, String::new())
                }
                Some(cur) => (EntryState::TargetChanged, format!("field is {cur}")),
            };
            data_states.insert(d.id.as_str(), state);
            outcomes.push(out(format!("data:{}", d.id), state, detail));
        }

        for (i, l) in p.file.loc.iter().enumerate() {
            let name = loc_name(i, l);
            let (state, detail, edit) = eval_loc(l, stock, snap, &data_states, &writers);
            if let Some((line, value, op, added)) = edit {
                edits.push(LocEdit {
                    line,
                    patch: pid.clone(),
                    value,
                    op,
                    added,
                });
            }
            outcomes.push(out(name, state, detail));
        }
    }
    (edits, outcomes, notes)
}

type Planned = Option<(usize, String, Op, Vec<String>)>;

fn eval_loc(
    l: &LocEntry,
    stock: &StockIni,
    snap: &dyn Snapshot,
    data_states: &BTreeMap<&str, EntryState>,
    writers: &HashMap<String, usize>,
) -> (EntryState, String, Planned) {
    if writers.get(&fold_key(&l.key)).copied().unwrap_or(0) > 1 {
        return (
            EntryState::Conflict,
            format!("another enabled entry also writes {}", l.key),
            None,
        );
    }
    let line = match stock.find(&l.key) {
        Ok(line) => line,
        Err(e) => return (EntryState::Missing, e.to_string(), None),
    };
    let stock_value = stock.value_at(line).unwrap_or_default();
    if let Some(r) = &l.requires {
        match data_states.get(r.as_str()) {
            Some(EntryState::NotNeeded) => {
                return (EntryState::NotNeeded, format!("{r} is not needed"), None);
            }
            Some(EntryState::Applies) => {}
            Some(other) => return (*other, format!("required entry {r} is {other:?}"), None),
            None => {
                return (
                    EntryState::Missing,
                    format!("required entry {r} not found"),
                    None,
                );
            }
        }
    }
    let value = l.value.clone().unwrap_or_default();
    let not_needed = match l.op {
        LocOp::Set => stock_value == value,
        LocOp::Replace => {
            let find = l.find.as_deref().unwrap_or_default();
            !stock_value.contains(find) && stock_value.contains(value.as_str())
        }
        LocOp::Append => stock_value.ends_with(value.as_str()),
        LocOp::Prepend => stock_value.starts_with(value.as_str()),
        LocOp::AppendFrom => false,
    };
    if not_needed {
        return (EntryState::NotNeeded, String::new(), None);
    }
    if let Some(want) = &l.expect_sha256
        && !want.is_empty()
        && !want.eq_ignore_ascii_case(&fsutil::sha256_hex(stock_value.as_bytes()))
    {
        return (
            EntryState::TargetChanged,
            "stock value no longer matches expect_sha256".into(),
            None,
        );
    }
    let base = snap.final_value(line);
    let planned = match l.op {
        LocOp::Set => (value.clone(), Op::Set, Vec::new()),
        LocOp::Append => (format!("{base}{value}"), Op::Append, vec![value]),
        LocOp::Prepend => (format!("{value}{base}"), Op::Prepend, vec![value]),
        LocOp::Replace => {
            let find = l.find.as_deref().unwrap_or_default();
            if base.matches(find).count() != 1 {
                return (
                    EntryState::TargetChanged,
                    "find text does not occur exactly once".into(),
                    None,
                );
            }
            (base.replacen(find, &value, 1), Op::Replace, Vec::new())
        }
        LocOp::AppendFrom => {
            let from = l.from.as_deref().unwrap_or_default();
            let from_line = match stock.find(from) {
                Ok(f) => f,
                Err(e) => return (EntryState::Missing, e.to_string(), None),
            };
            let part = match l.from_part {
                FromPart::Generated => snap.generated_part(from_line),
                FromPart::Stock => stock.value_at(from_line).unwrap_or_default().to_string(),
                FromPart::Final => snap.final_value(from_line),
            };
            if part.is_empty() {
                return (EntryState::NothingToAdd, String::new(), None);
            }
            let added = format!("{}{part}", l.separator);
            (
                format!("{base}{added}"),
                Op::AppendFrom {
                    borrows_stock: l.from_part != FromPart::Generated,
                },
                vec![added],
            )
        }
    };
    (
        EntryState::Applies,
        String::new(),
        Some((line, planned.0, planned.1, planned.2)),
    )
}

/// Summarise entry outcomes per patch.
pub fn summarise(set: &PatchSet, outcomes: &[EntryOutcome]) -> Vec<PatchOutcome> {
    set.patches
        .iter()
        .map(|p| {
            let states: Vec<EntryState> = outcomes
                .iter()
                .filter(|o| o.patch == p.file.id)
                .map(|o| o.state)
                .collect();
            let state = if states.contains(&EntryState::Applies) {
                "applies".to_string()
            } else if !states.is_empty() && states.iter().all(|s| *s == EntryState::NotNeeded) {
                "retired".to_string()
            } else if let Some(w) = states.iter().find(|s| s.warns()) {
                state_name(*w)
            } else {
                states
                    .first()
                    .map(|s| state_name(*s))
                    .unwrap_or_else(|| "empty".into())
            };
            PatchOutcome {
                id: p.file.id.clone(),
                state,
            }
        })
        .collect()
}

fn state_name(s: EntryState) -> String {
    serde_json::to_value(s)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Location of a user patch by id, for display.
pub fn user_patch_path(dirs: &AppDirs, id: &str) -> PathBuf {
    dirs.user_patches().join(format!("{id}.patch.toml"))
}

/// Does `path` look like a patch file name?
pub fn is_patch_file(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".patch.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Snap {
        stock: StockIni,
        generated: BTreeMap<usize, String>,
    }
    impl Snapshot for Snap {
        fn stock(&self) -> &StockIni {
            &self.stock
        }
        fn generated_part(&self, line: usize) -> String {
            self.generated.get(&line).cloned().unwrap_or_default()
        }
        fn final_value(&self, line: usize) -> String {
            format!(
                "{}{}",
                self.stock.value_at(line).unwrap_or_default(),
                self.generated_part(line)
            )
        }
    }

    fn snap() -> Snap {
        let stock = StockIni::parse(
            b"\xEF\xBB\xBFp2m1_desc=Filler one\r\np2m4_desc=Filler four\r\nname_a=Old Name\r\nfmt=%i dmg\r\n"
                .to_vec(),
        );
        let mut generated = BTreeMap::new();
        generated.insert(1, "\\n\\nGEN BODY".to_string());
        Snap { stock, generated }
    }

    fn set_of(texts: &[&str]) -> PatchSet {
        let mut set = PatchSet::default();
        for t in texts {
            let file = parse_patch("x", t).unwrap();
            set.patches.push(LoadedPatch {
                sha256: fsutil::sha256_hex(t.as_bytes()),
                file,
                origin: PatchOrigin::Builtin,
                enabled: true,
            });
        }
        set
    }

    const HDR: &str = "schema = 1\ntitle = \"t\"\n";

    #[test]
    fn rejects_bad_files() {
        assert!(
            parse_patch("a.patch.toml", &format!("{HDR}id = \"b\"\n")).is_err(),
            "name mismatch"
        );
        assert!(parse_patch("x", &format!("{HDR}id = \"a\"\nunknown = 1\n")).is_err());
        assert!(
            parse_patch(
                "x",
                &format!(
                    "{HDR}id = \"a\"\n[[loc]]\nkey = \"k\"\nop = \"append_from\"\nfrom = \"j\"\n"
                )
            )
            .is_err(),
            "append_from needs a guard"
        );
        assert!(
            parse_patch(
                "x",
                &format!("{HDR}id = \"a\"\n[[data]]\nid = \"d\"\nrecord = \"r\"\nfield = \"speed\"\nexpect = \"\"\nset = \"\"\n")
            )
            .is_err(),
            "fields are limited to the documented set"
        );
    }

    #[test]
    fn loc_ops_and_states() {
        let s = snap();
        let sha = fsutil::sha256_hex(b"Filler one");
        let set = set_of(&[
            &format!(
                "{HDR}id = \"a\"\n[[loc]]\nkey = \"P2M1_DESC\"\nop = \"append_from\"\nfrom = \"p2m4_desc\"\nseparator = \"\\\\n\\\\nALSO P2M4\"\nexpect_sha256 = \"{sha}\"\n"
            ),
            &format!(
                "{HDR}id = \"b\"\n[[loc]]\nkey = \"name_a\"\nop = \"replace\"\nfind = \"Old\"\nvalue = \"New\"\n"
            ),
            &format!("{HDR}id = \"c\"\n[[loc]]\nkey = \"name_x\"\nop = \"set\"\nvalue = \"v\"\n"),
            &format!(
                "{HDR}id = \"d\"\nbuilds = {{ game_min = \"5.0\" }}\n[[loc]]\nkey = \"fmt\"\nop = \"set\"\nvalue = \"v\"\n"
            ),
        ]);
        let build = BuildInfo::from_json(
            br#"{"Data":{"Branch":"sc-alpha-4.10.0","RequestedP4ChangeNum":"1"}}"#,
        )
        .unwrap();
        let (edits, outcomes, _) = run(&set, &GameData::default(), &s, Some(&build));
        let st: Vec<_> = outcomes
            .iter()
            .map(|o| (o.patch.as_str(), o.state))
            .collect();
        assert_eq!(
            st,
            vec![
                ("a", EntryState::Applies),
                ("b", EntryState::Applies),
                ("c", EntryState::Missing),
                ("d", EntryState::OutOfRange)
            ]
        );
        assert_eq!(edits[0].value, "Filler one\\n\\nALSO P2M4\\n\\nGEN BODY");
        assert_eq!(
            edits[0].op,
            Op::AppendFrom {
                borrows_stock: false
            }
        );
        assert_eq!(edits[1].value, "New Name");
    }

    #[test]
    fn not_needed_target_changed_conflict_nothing_to_add() {
        let s = snap();
        let set = set_of(&[
            &format!(
                "{HDR}id = \"a\"\n[[loc]]\nkey = \"name_a\"\nop = \"replace\"\nfind = \"Bad\"\nvalue = \"Old\"\n"
            ),
            &format!(
                "{HDR}id = \"b\"\n[[loc]]\nkey = \"name_a\"\nop = \"append\"\nvalue = \" Name\"\n"
            ),
            &format!(
                "{HDR}id = \"c\"\n[[loc]]\nkey = \"p2m4_desc\"\nop = \"set\"\nvalue = \"x\"\nexpect_sha256 = \"00\"\n"
            ),
            &format!("{HDR}id = \"d\"\n[[loc]]\nkey = \"fmt\"\nop = \"set\"\nvalue = \"%i dmg\"\n"),
            &format!(
                "{HDR}id = \"e\"\n[[loc]]\nkey = \"p2m1_desc\"\nop = \"append_from\"\nfrom = \"p2m1_desc\"\nexpect_sha256 = \"\"\n"
            ),
        ]);
        let (edits, outcomes, _) = run(&set, &GameData::default(), &s, None);
        let st: Vec<_> = outcomes.iter().map(|o| o.state).collect();
        assert_eq!(
            st,
            vec![
                EntryState::Conflict,
                EntryState::Conflict,
                EntryState::TargetChanged,
                EntryState::NotNeeded,
                EntryState::NothingToAdd
            ]
        );
        assert!(edits.is_empty());
        let sum = summarise(&set, &outcomes);
        assert_eq!(sum[3].state, "retired");
    }

    #[test]
    fn requires_follows_the_data_entry() {
        let s = snap();
        let set = set_of(&[&format!(
            "{HDR}id = \"a\"\n[[data]]\nid = \"own\"\nrecord = \"r\"\nfield = \"description_key\"\nexpect = \"p2m1_desc\"\nset = \"p2m4_desc\"\n[[loc]]\nkey = \"p2m1_desc\"\nop = \"append_from\"\nfrom = \"p2m4_desc\"\nrequires = \"own\"\n"
        )]);
        let (edits, outcomes, _) = run(&set, &GameData::default(), &s, None);
        // No records in M1's GameData: the data entry is missing, and so is the loc entry.
        assert_eq!(outcomes[0].state, EntryState::Missing);
        assert_eq!(outcomes[1].state, EntryState::Missing);
        assert!(edits.is_empty());
    }

    #[test]
    fn disabled_and_hash() {
        let mut set = set_of(&[&format!("{HDR}id = \"a\"\n")]);
        let h1 = set.hash();
        set.patches[0].enabled = false;
        assert_ne!(h1, set.hash());
    }
}
