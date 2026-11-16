//! Persistence (addendum §6.4): `data/fleet.json`, `data/ship-names.toml` and the
//! `[ship_names]` table of a channel's `profile.toml`. Unknown keys survive a write in all
//! three.

use crate::appdirs::AppDirs;
use crate::fsutil;
use crate::ini::fold_key;
use crate::source::Warning;
use crate::state;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const FLEET_JSON: &str = "fleet.json";
pub const SHIP_NAMES_TOML: &str = "ship-names.toml";
pub const FLEET_SCHEMA: u32 = 1;
pub const SHIP_NAMES_SCHEMA: u32 = 1;
/// Largest `fleet.json` / `ship-names.toml` read.
pub const FILE_LIMIT: u64 = 4 << 20;
/// Separators between prefix and name.
pub const SEPARATORS: [&str; 3] = [" ", "-", ". "];
/// Symbols for `style = "symbol"`.
pub const SYMBOLS: [&str; 6] = ["!", "#", "*", "+", "-", "."];
/// Longest rename, in characters.
pub const MAX_NAME: usize = 48;
/// Most blocks `group-letter` can name (A-Z).
pub const MAX_BLOCKS: usize = 26;

pub fn fleet_path(dirs: &AppDirs) -> PathBuf {
    dirs.fleet_json()
}

pub fn ship_names_path(dirs: &AppDirs) -> PathBuf {
    dirs.ship_names_toml()
}

fn one() -> u32 {
    1
}

// ---------------------------------------------------------------------------------------
// fleet.json
// ---------------------------------------------------------------------------------------

/// A hull the user owns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnedHull {
    pub hull: String,
    pub count: u32,
    /// Custom names from the export: rename suggestions, never applied by the import.
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A row that did not match one hull (unmatched, or ambiguous when `candidates` is set).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingRow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub count: u32,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `fleet.json`: what the user owns. Shared by every install and channel; not a build input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetFile {
    #[serde(default = "one")]
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    /// `xplor-json`, `xplor-csv` or `text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default)]
    pub hulls: Vec<OwnedHull>,
    #[serde(default)]
    pub pending: Vec<PendingRow>,
    /// Normalised row name -> hull id, or `null` for "Not in the game".
    #[serde(default)]
    pub matches: BTreeMap<String, Option<String>>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for FleetFile {
    fn default() -> Self {
        Self {
            schema: FLEET_SCHEMA,
            imported_at: None,
            format: None,
            hulls: Vec::new(),
            pending: Vec::new(),
            matches: BTreeMap::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl FleetFile {
    pub fn load(dirs: &AppDirs) -> Result<Self, String> {
        let p = fleet_path(dirs);
        match fsutil::read_limited(&p, FILE_LIMIT) {
            Ok(None) => Ok(Self::default()),
            Ok(Some(b)) => serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", p.display())),
            Err(e) => Err(format!("{}: {e}", p.display())),
        }
    }

    /// Atomic write (temp file + `rename_retry`).
    pub fn save(&self, dirs: &AppDirs) -> Result<(), String> {
        let p = fleet_path(dirs);
        fsutil::write_json(&p, self).map_err(|e| format!("{}: {e}", p.display()))
    }

    pub fn owned(&self, hull: &str) -> Option<&OwnedHull> {
        let f = fold_key(hull);
        self.hulls.iter().find(|h| fold_key(&h.hull) == f)
    }

    pub fn total(&self) -> u32 {
        self.hulls.iter().map(|h| h.count).sum()
    }
}

// ---------------------------------------------------------------------------------------
// ship-names.toml
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    /// Rename only.
    None,
    /// `01 `, `02 `, ...
    #[default]
    Numeric,
    /// `A1 `, `B3 `: block letter + position in the block.
    GroupLetter,
    /// The same symbol for every listed ship.
    Symbol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rest {
    /// Other ships keep their stock name (listed ones float above them).
    #[default]
    Stock,
    /// `numeric` numbering continues through every other flyable hull.
    Numbered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: toml::Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipEntry {
    pub hull: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Rename (long name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Rename of the short name; defaults to `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    #[serde(flatten)]
    pub extra: toml::Table,
}

/// Order, groups, renames and prefix style. In `ship-names.toml`, or a channel's
/// `profile.toml` `[ship_names]` table with `use = "channel"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipNames {
    #[serde(default = "one")]
    pub schema: u32,
    /// Opt-in.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub style: Style,
    #[serde(default = "default_separator")]
    pub separator: String,
    #[serde(default = "default_symbol")]
    pub symbol: String,
    #[serde(default)]
    pub rest: Rest,
    /// Also prefix the `_short` key.
    #[serde(default)]
    pub prefix_short: bool,
    #[serde(default, rename = "group", skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
    /// File order is the order.
    #[serde(default, rename = "ship", skip_serializing_if = "Vec::is_empty")]
    pub ships: Vec<ShipEntry>,
    #[serde(flatten)]
    pub extra: toml::Table,
}

fn default_separator() -> String {
    " ".into()
}

fn default_symbol() -> String {
    "*".into()
}

impl Default for ShipNames {
    fn default() -> Self {
        Self {
            schema: SHIP_NAMES_SCHEMA,
            enabled: false,
            style: Style::default(),
            separator: default_separator(),
            symbol: default_symbol(),
            rest: Rest::default(),
            prefix_short: false,
            groups: Vec::new(),
            ships: Vec::new(),
            extra: toml::Table::new(),
        }
    }
}

/// Why a rename is refused, `None` when fine (the value is checked trimmed).
pub fn name_problem(s: &str) -> Option<&'static str> {
    let t = s.trim();
    let n = t.chars().count();
    if n == 0 {
        Some("empty")
    } else if n > MAX_NAME {
        Some("longer than 48 characters")
    } else if t.chars().any(char::is_control) {
        Some("control character")
    } else if t.contains(['%', '<', '>', '~', '\\']) {
        Some("contains one of % < > ~ \\")
    } else if t.starts_with('@') {
        Some("starts with @")
    } else {
        None
    }
}

/// One block of the order: the ungrouped ships, or one group.
#[derive(Debug, Clone, Copy)]
pub struct Block<'a> {
    pub group: Option<&'a Group>,
    pub ships: &'a [ShipEntry],
}

impl ShipNames {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }

    /// Ships in order (§6.4): the ungrouped ones first, in file order, then each group in
    /// `[[group]]` order. A `group` id that does not exist counts as no group; a hull listed
    /// twice keeps its first entry.
    pub fn ordered(&self) -> Vec<(Option<&Group>, &ShipEntry)> {
        let ids: BTreeSet<&str> = self.groups.iter().map(|g| g.id.as_str()).collect();
        fn group_of<'s>(s: &'s ShipEntry, ids: &BTreeSet<&str>) -> Option<&'s str> {
            s.group.as_deref().filter(|g| ids.contains(g))
        }
        let mut seen = BTreeSet::new();
        let ships: Vec<&ShipEntry> = self
            .ships
            .iter()
            .filter(|s| seen.insert(fold_key(&s.hull)))
            .collect();
        let mut out: Vec<(Option<&Group>, &ShipEntry)> = ships
            .iter()
            .filter(|s| group_of(s, &ids).is_none())
            .map(|s| (None, *s))
            .collect();
        for g in &self.groups {
            out.extend(
                ships
                    .iter()
                    .filter(|s| group_of(s, &ids) == Some(g.id.as_str()))
                    .map(|s| (Some(g), *s)),
            );
        }
        out
    }

    /// Number of non-empty blocks.
    pub fn block_count(&self) -> usize {
        let o = self.ordered();
        let mut n = usize::from(o.iter().any(|(g, _)| g.is_none()));
        for g in &self.groups {
            if o.iter().any(|(x, _)| x.is_some_and(|x| x.id == g.id)) {
                n += 1;
            }
        }
        n
    }

    pub fn is_listed(&self, hull: &str) -> bool {
        let f = fold_key(hull);
        self.ships.iter().any(|s| fold_key(&s.hull) == f)
    }

    /// The settings with bad entries dropped (a warning each), and whether the whole table
    /// is invalid (`ship-names-invalid`: the category then generates nothing).
    pub fn validated(&self) -> (Self, Vec<Warning>, bool) {
        let mut w = Vec::new();
        let mut s = self.clone();
        let warn = |w: &mut Vec<Warning>, m: String| w.push(Warning::new("ship-names", m));
        if !SEPARATORS.contains(&s.separator.as_str()) {
            warn(
                &mut w,
                format!(
                    "separator {:?} is not one of \" \", \"-\", \". \"",
                    s.separator
                ),
            );
            s.separator = default_separator();
        }
        if !SYMBOLS.contains(&s.symbol.as_str()) {
            warn(
                &mut w,
                format!("symbol {:?} is not one of ! # * + - .", s.symbol),
            );
            s.symbol = default_symbol();
        }
        let mut gids = BTreeSet::new();
        s.groups.retain(|g| {
            let ok = !g.id.trim().is_empty() && gids.insert(g.id.clone());
            if !ok {
                w.push(Warning::new(
                    "ship-names",
                    format!("group {:?} dropped (empty or repeated id)", g.id),
                ));
            }
            ok
        });
        for g in &mut s.groups {
            if name_problem(&g.name).is_some() {
                g.name = g.id.clone();
            } else {
                g.name = g.name.trim().to_string();
            }
        }
        let mut hulls = BTreeSet::new();
        s.ships
            .retain(|e| !e.hull.trim().is_empty() && hulls.insert(fold_key(&e.hull)));
        for e in &mut s.ships {
            for (field, v) in [("name", &mut e.name), ("short", &mut e.short)] {
                if let Some(x) = v.as_ref() {
                    match name_problem(x) {
                        Some(why) => {
                            w.push(Warning::new(
                                "ship-names",
                                format!("{}: {field} dropped ({why})", e.hull),
                            ));
                            *v = None;
                        }
                        None => *v = Some(x.trim().to_string()),
                    }
                }
            }
        }
        let invalid = s.style == Style::GroupLetter && s.block_count() > MAX_BLOCKS;
        if invalid {
            w.push(Warning::new(
                "ship-names-invalid",
                format!(
                    "style group-letter names at most {MAX_BLOCKS} blocks, the list has {}; ship names are not written",
                    s.block_count()
                ),
            ));
        }
        (s, w, invalid)
    }

    /// Take the known fields of `new` (from the UI), keeping the unknown keys of this file,
    /// its groups and its ship entries.
    pub fn merge_from(&mut self, new: &ShipNames) {
        let mut out = new.clone();
        out.extra = self.extra.clone();
        for g in &mut out.groups {
            if let Some(old) = self.groups.iter().find(|o| o.id == g.id) {
                g.extra = old.extra.clone();
            }
        }
        for e in &mut out.ships {
            let f = fold_key(&e.hull);
            if let Some(old) = self.ships.iter().find(|o| fold_key(&o.hull) == f) {
                e.extra = old.extra.clone();
            }
        }
        out.schema = self.schema.max(SHIP_NAMES_SCHEMA);
        *self = out;
    }

    /// Append hulls not listed yet (ungrouped, at the end). Returns how many were added.
    pub fn append(&mut self, hulls: &[String]) -> usize {
        let mut n = 0;
        for h in hulls {
            if !self.is_listed(h) {
                self.ships.push(ShipEntry {
                    hull: h.clone(),
                    group: None,
                    name: None,
                    short: None,
                    extra: toml::Table::new(),
                });
                n += 1;
            }
        }
        n
    }

    /// Drop the ships whose hull is not in `keep` ("Remove all ships first" on a fleet
    /// import). Ships that stay keep their group, rename and place; groups stay even when
    /// they end up empty. Returns the dropped entries, in list order.
    pub fn prune_to(&mut self, keep: &[String]) -> Vec<ShipEntry> {
        let keep: Vec<String> = keep.iter().map(|h| fold_key(h)).collect();
        let (stay, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ships)
            .into_iter()
            .partition(|s| keep.contains(&fold_key(&s.hull)));
        self.ships = stay;
        gone
    }
}

/// The shared settings. A missing file is the default (off, empty list).
pub fn load_shared(dirs: &AppDirs) -> Result<ShipNames, String> {
    let p = ship_names_path(dirs);
    match fsutil::read_limited(&p, FILE_LIMIT) {
        Ok(None) => Ok(ShipNames::default()),
        Ok(Some(b)) => {
            let text = String::from_utf8(b).map_err(|_| format!("{}: not UTF-8", p.display()))?;
            ShipNames::parse(&text).map_err(|e| format!("{}: {e}", p.display()))
        }
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// The shared settings before a save replaces them. A file that cannot be read as settings
/// (not UTF-8, not TOML, a bad value) is moved aside to `ship-names.toml.bad-<stamp>` first, so
/// the save does not lose what the user wrote; the result is then the default. Returns the
/// backup path when one was made. A read error (permissions) stays an error.
pub fn load_shared_for_save(dirs: &AppDirs) -> Result<(ShipNames, Option<PathBuf>), String> {
    let p = ship_names_path(dirs);
    let bytes = match fsutil::read_limited(&p, FILE_LIMIT) {
        Ok(None) => return Ok((ShipNames::default(), None)),
        Ok(Some(b)) => b,
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    let parsed = String::from_utf8(bytes)
        .map_err(|_| ())
        .and_then(|t| ShipNames::parse(&t).map_err(|_| ()));
    match parsed {
        Ok(s) => Ok((s, None)),
        Err(()) => {
            let mut name = p.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".bad-{}", fsutil::stamp(fsutil::unix_now())));
            let bak = p.with_file_name(name);
            fsutil::rename_retry(&p, &bak).map_err(|e| format!("{}: {e}", bak.display()))?;
            Ok((ShipNames::default(), Some(bak)))
        }
    }
}

pub fn save_shared(dirs: &AppDirs, s: &ShipNames) -> Result<(), String> {
    let p = ship_names_path(dirs);
    let text = s.to_toml()?;
    fsutil::write_atomic(&p, text.as_bytes()).map_err(|e| format!("{}: {e}", p.display()))
}

// ---------------------------------------------------------------------------------------
// Channel scope and effective settings
// ---------------------------------------------------------------------------------------

/// `profile.toml` `[ship_names] use = ...`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// The shared `ship-names.toml` (default when the table is absent).
    #[default]
    Shared,
    /// The table in this channel's profile.
    Channel,
    /// No ship names for this channel.
    Off,
}

impl Scope {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Channel => "channel",
            Self::Off => "off",
        }
    }
}

pub const PROFILE_TABLE: &str = "ship_names";

fn raw_use(profile: &state::Profile) -> Option<&toml::Value> {
    profile.table.get(PROFILE_TABLE).and_then(|t| t.get("use"))
}

/// The scope; an unknown `use` value counts as shared (see [`scope_warning`]).
pub fn scope_of(profile: &state::Profile) -> Scope {
    match raw_use(profile).and_then(|v| v.as_str()) {
        Some("channel") => Scope::Channel,
        Some("off") => Scope::Off,
        _ => Scope::Shared,
    }
}

/// A warning when `use` is set to something other than `shared`, `channel` or `off`.
pub fn scope_warning(profile: &state::Profile) -> Option<Warning> {
    let v = raw_use(profile)?;
    if matches!(v.as_str(), Some("shared" | "channel" | "off")) {
        return None;
    }
    Some(Warning::new(
        "ship-names",
        format!(
            "profile.toml [ship_names] use = {v} is not \"shared\", \"channel\" or \"off\"; the shared settings are used"
        ),
    ))
}

/// The channel's own table (`use = "channel"`), without `use`.
pub fn channel_settings(profile: &state::Profile) -> Result<ShipNames, String> {
    let mut t = profile
        .table
        .get(PROFILE_TABLE)
        .and_then(|v| v.as_table())
        .cloned()
        .unwrap_or_default();
    t.remove("use");
    toml::Value::Table(t)
        .try_into()
        .map_err(|e: toml::de::Error| format!("profile.toml [ship_names]: {e}"))
}

/// Write the scope (and for `Channel` the settings) into the profile table, keeping unknown
/// keys of an existing channel table.
pub fn set_channel(profile: &mut state::Profile, scope: Scope, settings: Option<&ShipNames>) {
    let old = channel_settings(profile).ok();
    let mut t = match (scope, settings) {
        (Scope::Channel, Some(s)) => {
            let mut merged = old.unwrap_or_default();
            merged.merge_from(s);
            match toml::Value::try_from(&merged) {
                Ok(toml::Value::Table(t)) => t,
                _ => toml::Table::new(),
            }
        }
        // Keep a channel list around while it is not in use? No: "All channels" drops it
        // (the UI asks first), "Off" keeps nothing either.
        _ => toml::Table::new(),
    };
    if scope == Scope::Shared {
        profile.table.remove(PROFILE_TABLE);
        return;
    }
    t.insert("use".into(), toml::Value::String(scope.name().into()));
    profile
        .table
        .insert(PROFILE_TABLE.into(), toml::Value::Table(t));
}

/// A channel's effective ship-name settings, after `use` is resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Effective {
    pub scope: Scope,
    /// Validated settings (the default when the source could not be read).
    pub settings: ShipNames,
    pub warnings: Vec<Warning>,
    /// The table could not be read or is invalid: nothing is generated.
    pub invalid: bool,
}

impl Effective {
    /// What the generator uses: `None` when off, disabled or invalid.
    pub fn active(&self) -> Option<&ShipNames> {
        (self.scope != Scope::Off && self.settings.enabled && !self.invalid)
            .then_some(&self.settings)
    }

    /// `applied.json` `ship_names_hash`: SHA-256 of the canonical JSON of the active
    /// settings' difference from `ShipNames::default()` ([`crate::canon::settings_hash`]),
    /// empty when there are none. A field a later build adds with its default leaves it
    /// unchanged; fields added to `ShipEntry`/`Group` need `skip_serializing_if` at their
    /// default for the same (array elements are compared whole).
    pub fn hash(&self) -> String {
        self.active()
            .map(crate::canon::settings_hash)
            .unwrap_or_default()
    }

    /// The hash 0.1.0 recorded: canonical JSON of all active settings. Status accepts it so
    /// a manifest from 0.1.0 does not go stale on upgrade (it stops matching once the
    /// settings' shape changes; the next apply records [`Self::hash`]).
    pub fn legacy_hash(&self) -> String {
        match self.active() {
            None => String::new(),
            Some(s) => {
                let v = serde_json::to_value(s).unwrap_or_default();
                fsutil::sha256_hex(crate::canon::canonical(&v).as_bytes())
            }
        }
    }
}

pub fn effective(dirs: &AppDirs, profile: &state::Profile) -> Effective {
    let scope = scope_of(profile);
    let raw = match scope {
        Scope::Shared => load_shared(dirs),
        Scope::Channel => channel_settings(profile),
        Scope::Off => Ok(ShipNames::default()),
    };
    let scope_warn = scope_warning(profile);
    let mut eff = match raw {
        Ok(s) => {
            let (settings, warnings, invalid) = s.validated();
            Effective {
                scope,
                settings,
                warnings,
                invalid,
            }
        }
        Err(e) => Effective {
            scope,
            settings: ShipNames::default(),
            warnings: vec![Warning::new(
                "ship-names-invalid",
                format!("{e}; ship names are not written"),
            )],
            invalid: true,
        },
    };
    eff.warnings.splice(0..0, scope_warn);
    eff
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn entry(hull: &str, group: Option<&str>) -> ShipEntry {
        ShipEntry {
            hull: hull.into(),
            group: group.map(str::to_string),
            name: None,
            short: None,
            extra: toml::Table::new(),
        }
    }

    #[test]
    fn ship_names_round_trip_keeps_unknown_keys() {
        let text = r#"
schema = 1
enabled = true
style = "group-letter"
future = "x"

[[group]]
id = "combat"
name = "Combat"
colour = "red"

[[ship]]
hull = "vehicle_NameAEGS_Gladius"
group = "combat"
name = "Needle"
pinned = true

[[ship]]
hull = "vehicle_NameDRAK_Cutlass_Black"
"#;
        let s = ShipNames::parse(text).unwrap();
        assert_eq!(s.style, Style::GroupLetter);
        assert_eq!(s.ships.len(), 2);
        let again = ShipNames::parse(&s.to_toml().unwrap()).unwrap();
        assert_eq!(again, s);
        let out = s.to_toml().unwrap();
        assert!(
            out.contains("future = \"x\"")
                && out.contains("colour = \"red\"")
                && out.contains("pinned = true"),
            "{out}"
        );

        // The UI sends known fields only; unknown keys stay.
        let mut file = s.clone();
        let mut ui = s.clone();
        ui.extra.clear();
        ui.groups[0].extra.clear();
        ui.ships[0].extra.clear();
        ui.ships.reverse();
        file.merge_from(&ui);
        assert_eq!(file.extra.get("future").and_then(|v| v.as_str()), Some("x"));
        assert!(file.ships[1].extra.contains_key("pinned"));
        assert!(file.groups[0].extra.contains_key("colour"));
        assert_eq!(file.ships[0].hull, "vehicle_NameDRAK_Cutlass_Black");
    }

    #[test]
    fn prune_to_keeps_listed_ships_with_their_settings() {
        let mut s = ShipNames::default();
        s.groups.push(Group {
            id: "combat".into(),
            name: "Combat".into(),
            extra: toml::Table::new(),
        });
        s.ships = vec![
            entry("vehicle_NameAEGS_Gladius", Some("combat")),
            entry("vehicle_NameDRAK_Cutlass_Black", None),
            entry("vehicle_NameRSI_Aurora_MR", Some("combat")),
        ];
        s.ships[2].name = Some("Old Rusty".into());
        let gone = s.prune_to(&[
            "VEHICLE_NAMEAEGS_GLADIUS".into(),
            "vehicle_NameRSI_Aurora_MR".into(),
        ]);
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].hull, "vehicle_NameDRAK_Cutlass_Black");
        let hulls: Vec<_> = s.ships.iter().map(|e| e.hull.as_str()).collect();
        assert_eq!(
            hulls,
            ["vehicle_NameAEGS_Gladius", "vehicle_NameRSI_Aurora_MR"]
        );
        assert_eq!(s.ships[1].name.as_deref(), Some("Old Rusty"));
        assert_eq!(s.ships[1].group.as_deref(), Some("combat"));
        assert_eq!(s.groups.len(), 1, "groups stay");

        let gone = s.prune_to(&[]);
        assert_eq!(gone.len(), 2);
        assert!(s.ships.is_empty());
    }

    #[test]
    fn unknown_style_is_a_parse_error() {
        assert!(ShipNames::parse("style = \"roman\"").is_err());
        assert_eq!(ShipNames::parse("").unwrap(), ShipNames::default());
    }

    #[test]
    fn order_ungrouped_first_then_groups() {
        let mut s = ShipNames::default();
        s.groups = vec![
            Group {
                id: "cargo".into(),
                name: "Cargo".into(),
                extra: Default::default(),
            },
            Group {
                id: "combat".into(),
                name: "Combat".into(),
                extra: Default::default(),
            },
        ];
        s.ships = vec![
            entry("a", Some("combat")),
            entry("b", None),
            entry("c", Some("cargo")),
            entry("d", Some("nope")),
            entry("B", Some("combat")),
            entry("e", Some("combat")),
        ];
        let order: Vec<&str> = s.ordered().iter().map(|(_, e)| e.hull.as_str()).collect();
        assert_eq!(order, ["b", "d", "c", "a", "e"]);
        assert_eq!(s.block_count(), 3);
    }

    #[test]
    fn validation_drops_bad_entries() {
        let mut s = ShipNames::default();
        s.separator = "_".into();
        s.symbol = "$".into();
        s.ships = vec![entry("a", None), entry("a", None), entry(" ", None)];
        s.ships[0].name = Some("  Needle ".into());
        s.ships[0].short = Some("100%".into());
        let (v, w, invalid) = s.validated();
        assert!(!invalid);
        assert_eq!(v.separator, " ");
        assert_eq!(v.symbol, "*");
        assert_eq!(v.ships.len(), 1);
        assert_eq!(v.ships[0].name.as_deref(), Some("Needle"));
        assert_eq!(v.ships[0].short, None);
        assert_eq!(w.len(), 3, "{w:?}");
        for bad in ["", "@x", "a<b", "x\u{7}", &"x".repeat(49), "a\\n"] {
            assert!(name_problem(bad).is_some(), "{bad:?}");
        }
        assert!(name_problem(&"é".repeat(48)).is_none());
    }

    #[test]
    fn group_letter_with_too_many_blocks_is_invalid() {
        let mut s = ShipNames::default();
        s.style = Style::GroupLetter;
        for i in 0..27 {
            s.groups.push(Group {
                id: format!("g{i}"),
                name: format!("G{i}"),
                extra: Default::default(),
            });
            s.ships
                .push(entry(&format!("h{i}"), Some(&format!("g{i}"))));
        }
        assert!(s.validated().2);
        s.style = Style::Numeric;
        assert!(!s.validated().2);
    }

    #[test]
    fn scope_and_hash() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        let mut p = state::Profile::preset("recommended").unwrap();
        assert_eq!(scope_of(&p), Scope::Shared);
        let e = effective(&dirs, &p);
        assert_eq!(e.hash(), "", "default is off");

        let mut shared = ShipNames::default();
        shared.enabled = true;
        shared.ships.push(entry("vehicle_NameA", None));
        save_shared(&dirs, &shared).unwrap();
        let h1 = effective(&dirs, &p).hash();
        assert_eq!(h1.len(), 64);
        // 0.1.0's hash (all settings) differs from the diff-from-default one; both are kept
        // so status accepts either.
        let legacy = effective(&dirs, &p).legacy_hash();
        assert_eq!(legacy.len(), 64);
        assert_ne!(legacy, h1);
        shared.ships.insert(0, entry("vehicle_NameB", None));
        save_shared(&dirs, &shared).unwrap();
        let h2 = effective(&dirs, &p).hash();
        assert_ne!(h1, h2, "reorder changes the hash");

        // Channel scope: its own table, the shared file is ignored.
        let mut own = ShipNames::default();
        own.enabled = true;
        own.style = Style::Symbol;
        set_channel(&mut p, Scope::Channel, Some(&own));
        assert_eq!(scope_of(&p), Scope::Channel);
        let e = effective(&dirs, &p);
        assert_eq!(e.settings.style, Style::Symbol);
        assert!(e.settings.ships.is_empty());
        let text = toml::to_string(&p.table).unwrap();
        assert!(
            text.contains("[ship_names]") && text.contains("use = \"channel\""),
            "{text}"
        );

        set_channel(&mut p, Scope::Off, None);
        assert_eq!(effective(&dirs, &p).hash(), "");
        set_channel(&mut p, Scope::Shared, None);
        assert!(!p.table.contains_key(PROFILE_TABLE));
        assert_eq!(effective(&dirs, &p).hash(), h2);

        // A broken shared file: invalid, nothing generated, a warning.
        std::fs::write(ship_names_path(&dirs), "style = \"roman\"\n").unwrap();
        let e = effective(&dirs, &p);
        assert!(e.invalid && e.active().is_none());
        assert_eq!(e.warnings[0].kind, "ship-names-invalid");
    }

    #[test]
    fn unknown_scope_value_warns_and_uses_shared() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        let mut p = state::Profile::preset("recommended").unwrap();
        let mut tbl = toml::Table::new();
        tbl.insert("use".into(), "chanel".into());
        p.table.insert(PROFILE_TABLE.into(), tbl.into());
        assert_eq!(scope_of(&p), Scope::Shared);
        let e = effective(&dirs, &p);
        assert_eq!(e.scope, Scope::Shared);
        assert!(
            e.warnings
                .iter()
                .any(|w| w.kind == "ship-names" && w.detail.contains("chanel")),
            "{:?}",
            e.warnings
        );
        for ok in [Scope::Shared, Scope::Channel, Scope::Off] {
            set_channel(&mut p, ok, Some(&ShipNames::default()));
            assert!(scope_warning(&p).is_none(), "{ok:?}");
        }
    }

    #[test]
    fn save_over_a_broken_shared_file_keeps_a_copy() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        assert_eq!(
            load_shared_for_save(&dirs).unwrap(),
            (ShipNames::default(), None)
        );
        let mut s = ShipNames::default();
        s.ships.push(entry("vehicle_NameA", None));
        save_shared(&dirs, &s).unwrap();
        assert_eq!(load_shared_for_save(&dirs).unwrap(), (s, None));
        let broken = "style = \"roman\"\n# my notes\n";
        std::fs::write(ship_names_path(&dirs), broken).unwrap();
        let (got, bak) = load_shared_for_save(&dirs).unwrap();
        assert_eq!(got, ShipNames::default());
        let bak = bak.expect("backup made");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), broken);
        assert!(!ship_names_path(&dirs).exists());
        assert!(
            bak.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ship-names.toml.bad-")
        );
    }

    #[test]
    fn fleet_round_trip_keeps_unknown_keys() {
        let t = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(t.path());
        assert_eq!(FleetFile::load(&dirs).unwrap(), FleetFile::default());
        std::fs::create_dir_all(&dirs.data).unwrap();
        std::fs::write(
            fleet_path(&dirs),
            r#"{"schema":1,"later":{"a":1},"hulls":[{"hull":"vehicle_NameA","count":2,"names":["Rusty"],"paint":"x"}],
               "pending":[{"name":"Kraken","count":1,"note":true}],"matches":{"kraken":null,"old":"vehicle_NameA"}}"#,
        )
        .unwrap();
        let f = FleetFile::load(&dirs).unwrap();
        assert_eq!(f.total(), 2);
        assert_eq!(f.matches.get("kraken"), Some(&None));
        f.save(&dirs).unwrap();
        let text = std::fs::read_to_string(fleet_path(&dirs)).unwrap();
        for k in ["\"later\"", "\"paint\"", "\"note\""] {
            assert!(text.contains(k), "{k} lost: {text}");
        }
        assert_eq!(FleetFile::load(&dirs).unwrap(), f);
    }
}
