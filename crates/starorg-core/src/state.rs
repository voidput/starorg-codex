//! Per-channel state in the data directory (spec 4.2, 4.4, 4.7):
//! `channel.toml` (consent, language decision), `profile.toml`, `overrides.ini`.

use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Overrides larger than this are rejected (spec 11).
pub const OVERRIDES_LIMIT: u64 = 8 << 20;

pub const CHANNEL_TOML: &str = "channel.toml";
pub const PROFILE_TOML: &str = "profile.toml";
pub const OVERRIDES_INI: &str = "overrides.ini";
pub const APPLIED_JSON: &str = "applied.json";
pub const INTENT_JSON: &str = "intent.json";

/// `channel.toml`. A channel is **set up** when `consent = true` (spec 4.1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelState {
    #[serde(default)]
    pub consent: bool,
    /// When `setup` ran (RFC 3339 UTC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_time: Option<String>,
    /// The user agreed to replace another `g_language` value with `english` (spec 7.4).
    #[serde(default)]
    pub switch_language: bool,
    /// Whose owned blueprints drive `[Owned]` markers: `auto` (default: the character of the
    /// newest session in this channel's logs), `any` (union), or a geid (addendum §3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_character: Option<String>,
    #[serde(flatten)]
    pub extra: toml::Table,
}

fn read_toml<T: serde::de::DeserializeOwned + Default>(path: &Path) -> std::io::Result<T> {
    match fsutil::read_limited(path, 1 << 20)? {
        None => Ok(T::default()),
        Some(b) => {
            let s = String::from_utf8(b).map_err(std::io::Error::other)?;
            toml::from_str(&s)
                .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))
        }
    }
}

impl ChannelState {
    pub fn load(channel_dir: &Path) -> std::io::Result<Self> {
        read_toml(&channel_dir.join(CHANNEL_TOML))
    }

    pub fn save(&self, channel_dir: &Path) -> std::io::Result<()> {
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fsutil::write_atomic(&channel_dir.join(CHANNEL_TOML), s.as_bytes())
    }
}

/// Presets shipped (spec 4.7).
pub const PRESETS: &[&str] = &["recommended", "minimal", "everything"];

/// `schema` of the `profile.toml` this build writes.
///
/// - 1: builds up to the `[Owned]` colour change wrote every `[enhance]` key out, so a
///   setting saved at its default stayed pinned when a later build changed that default.
/// - 2: the first sparse build. Its saves hold only the settings that differ from the
///   preset, but its migration of schema-1 files dropped only the colour keys, so such a
///   file still pins the other defaults.
/// - 3: `[enhance]` holds only the settings that differ from the preset, so new defaults
///   reach everyone who did not change that setting. Older files are read (and, by the
///   app, rewritten once) as [`Profile::migrate`] says.
pub const PROFILE_SCHEMA: i64 = 3;

/// The backup [`migrate_profile_file`] keeps of a schema-1 file it changes.
pub const PROFILE_SCHEMA1_BAK: &str = "profile.toml.bak-schema1";

/// The backup [`migrate_profile_file`] keeps of a file of `schema` it changes
/// (`profile.toml.bak-schema1` for schema 1).
pub fn profile_backup_name(schema: i64) -> String {
    format!("profile.toml.bak-schema{schema}")
}

/// A schema-1 `[enhance]` value: the default it had when it was written out.
#[derive(Clone, Copy)]
enum Was {
    Bool(bool),
    Int(i64),
}

/// The colour/marker settings (added after 0.2.0, plus the two EM settings 0.2.0 already
/// had) with their schema-1 defaults. A schema-1 file holding one of these at that value
/// most likely never chose it (the whole table was written on every save), so the
/// migration drops it and the current default applies. `owned_em` is the one whose
/// default changed (0 to 4); dropping the others leaves the effective value as it was.
const SCHEMA1_DEFAULTS: &[(&[&str], Was)] = &[
    (&["mission", "header_em"], Was::Int(3)),
    (&["mission", "label_em"], Was::Int(4)),
    (&["mission", "title_bp_em"], Was::Int(4)),
    (&["mission", "title_rep_em"], Was::Int(4)),
    (&["mission", "title_owned"], Was::Bool(false)),
    (&["mission", "owned_em"], Was::Int(0)),
    (&["mission", "owned_line"], Was::Bool(false)),
    (&["mission", "missing_marker"], Was::Bool(false)),
    (&["mission", "missing_em"], Was::Int(0)),
    (&["mission", "variant_em"], Was::Int(4)),
    (&["mission", "reward_em"], Was::Int(0)),
    (&["colour_test"], Was::Bool(false)),
];

/// The table at `path` under `t`, if every step is a table.
fn table_at<'a>(t: &'a mut toml::Table, path: &[&str]) -> Option<&'a mut toml::Table> {
    match path.split_first() {
        None => Some(t),
        Some((p, rest)) => match t.get_mut(*p) {
            Some(toml::Value::Table(sub)) => table_at(sub, rest),
            _ => None,
        },
    }
}

impl Was {
    fn is(self, v: &toml::Value) -> bool {
        match self {
            Was::Bool(b) => v.as_bool() == Some(b),
            Was::Int(n) => v.as_integer() == Some(n),
        }
    }
}

/// What [`Profile::migrate`] dropped from a schema-1 `[enhance]` table, as
/// `enhance.mission.owned_em`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Migration {
    /// The file's `schema` before.
    pub from: i64,
    /// Saved at an old default this build changed: these now follow the current default,
    /// so their effective value changed.
    pub followed: Vec<String>,
    /// Saved at the current default: nothing changes now, a later default change reaches
    /// them.
    pub unpinned: Vec<String>,
}

impl Migration {
    pub fn is_empty(&self) -> bool {
        self.followed.is_empty() && self.unpinned.is_empty()
    }
}

/// `profile.toml`, kept as a TOML table so unknown keys survive a write by an older version.
/// M1 reads only `preset` and `patches.disabled`; the category settings arrive with the
/// generators in M2.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Profile {
    pub table: toml::Table,
}

impl Profile {
    pub fn preset(name: &str) -> Option<Self> {
        if !PRESETS.contains(&name) {
            return None;
        }
        let mut table = toml::Table::new();
        table.insert("schema".into(), toml::Value::Integer(PROFILE_SCHEMA));
        table.insert("preset".into(), toml::Value::String(name.into()));
        let mut patches = toml::Table::new();
        patches.insert("disabled".into(), toml::Value::Array(Vec::new()));
        table.insert("patches".into(), toml::Value::Table(patches));
        Some(Self { table })
    }

    pub fn path(channel_dir: &Path) -> PathBuf {
        channel_dir.join(PROFILE_TOML)
    }

    pub fn load_file(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            table: read_toml(path)?,
        })
    }

    pub fn save(&self, channel_dir: &Path) -> std::io::Result<()> {
        let s = toml::to_string_pretty(&self.table).map_err(std::io::Error::other)?;
        fsutil::write_atomic(&Self::path(channel_dir), s.as_bytes())
    }

    /// The file's `schema` (1 when missing: files before 0.2 had it, but be lenient).
    pub fn schema(&self) -> i64 {
        self.table
            .get("schema")
            .and_then(toml::Value::as_integer)
            .unwrap_or(1)
    }

    /// Bring an older table to [`PROFILE_SCHEMA`] (schema 1 wrote every `[enhance]` key
    /// out, so a saved value cannot tell a choice from a default; a schema-2 file may be
    /// such a file after the first, colour-keys-only migration):
    ///
    /// 1. schema 1 only: each [`SCHEMA1_DEFAULTS`] key saved at its schema-1 default is
    ///    dropped, so it follows the current default (`owned_em` 0 becomes 4);
    /// 2. every other `[enhance]` key equal to the current default of the file's preset is
    ///    dropped too ([`crate::enhance::profile::drop_defaults`]): no effective change now,
    ///    but a later default change reaches it.
    ///
    /// Tables that empties go, `schema` is set, everything else stays as saved. `None` when
    /// already current (nothing changed). Idempotent: a migrated table is current, so a
    /// later deliberate `owned_em = 0` is never dropped (nor is a schema-2 one: step 1 is
    /// for schema 1 only). The cost: a schema-1 value the user did pick but that equals a
    /// default is dropped with the rest (the app keeps a backup).
    pub fn migrate(&mut self) -> Option<Migration> {
        let from = self.schema();
        if from >= PROFILE_SCHEMA {
            return None;
        }
        let mut m = Migration {
            from,
            ..Migration::default()
        };
        let base = crate::enhance::profile::Profile::preset(
            self.table
                .get("preset")
                .and_then(toml::Value::as_str)
                .unwrap_or("recommended"),
        )
        .ok();
        let def = base.as_ref().and_then(|b| toml::Value::try_from(b).ok());
        if let Some(toml::Value::Table(enh)) = self.table.get_mut("enhance") {
            for (path, was) in SCHEMA1_DEFAULTS.iter().filter(|_| from < 2) {
                let (key, parents) = path.split_last().expect("non-empty path");
                if let Some(t) = table_at(enh, parents)
                    && t.get(*key).is_some_and(|v| was.is(v))
                {
                    let v = t.remove(*key).expect("checked");
                    let name = format!("enhance.{}", path.join("."));
                    let mut now = def.as_ref();
                    for p in *path {
                        now = now.and_then(|d| d.get(*p));
                    }
                    if now == Some(&v) {
                        m.unpinned.push(name);
                    } else {
                        m.followed.push(name);
                    }
                }
            }
            if let Some(b) = &base {
                m.unpinned
                    .extend(crate::enhance::profile::drop_defaults(enh, b).unwrap_or_default());
            }
            if enh
                .get("mission")
                .and_then(toml::Value::as_table)
                .is_some_and(toml::Table::is_empty)
            {
                enh.remove("mission");
            }
            if enh.is_empty() {
                self.table.remove("enhance");
            }
        }
        self.table
            .insert("schema".into(), toml::Value::Integer(PROFILE_SCHEMA));
        Some(m)
    }

    /// The generator settings: `preset` plus the optional `[enhance]` table (spec 4.7),
    /// read as [`Self::migrate`] would leave them, so a schema-1 file means the same in
    /// every reader whether or not it has been rewritten yet. Other tables (`patches`,
    /// unknown keys) are not the generators' business.
    pub fn enhance(&self) -> Result<crate::enhance::profile::Profile, String> {
        self.enhance_checked().map(|(p, _)| p)
    }

    /// [`Self::enhance`] plus the `[enhance]` keys this build does not know (ignored).
    pub fn enhance_checked(
        &self,
    ) -> Result<(crate::enhance::profile::Profile, Vec<String>), String> {
        let mut cur = Profile {
            table: toml::Table::new(),
        };
        for k in ["schema", "preset", "enhance"] {
            if let Some(v) = self.table.get(k) {
                cur.table.insert(k.into(), v.clone());
            }
        }
        cur.migrate();
        let mut t = toml::Table::new();
        for k in ["preset", "enhance"] {
            if let Some(v) = cur.table.get(k) {
                t.insert(k.into(), v.clone());
            }
        }
        let text = toml::to_string(&t).map_err(|e| e.to_string())?;
        crate::enhance::profile::Profile::from_toml_checked(&text).map_err(|e| e.to_string())
    }

    /// Ids of built-in patches the user switched off.
    pub fn disabled_patches(&self) -> Vec<String> {
        self.table
            .get("patches")
            .and_then(|p| p.get("disabled"))
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `applied.json` `profile_hash`: the generator settings ([`Self::enhance`]) hashed by
    /// [`crate::enhance::profile::Profile::hash`], so only settings off the defaults count
    /// and a build adding a setting does not change it. Patch switches and ship names have
    /// their own hashes; other keys do not change the output. An invalid profile hashes as
    /// `invalid:` plus [`Self::legacy_hash`], so editing it still shows.
    pub fn hash(&self) -> String {
        match self.enhance() {
            Ok(p) => p.hash(),
            Err(_) => format!("invalid:{}", self.legacy_hash()),
        }
    }

    /// The hash 0.1.0 recorded: of the whole table's TOML text. Status accepts it too, so a
    /// manifest from 0.1.0 does not go stale on upgrade; the next apply records [`Self::hash`].
    pub fn legacy_hash(&self) -> String {
        let s = toml::to_string(&self.table).unwrap_or_default();
        fsutil::sha256_hex(s.as_bytes())
    }
}

/// Rewrite a channel's older `profile.toml` as [`Profile::migrate`] says, once. When a
/// setting is dropped the old file is first copied to [`profile_backup_name`] of its schema
/// (an existing backup is kept): restoring it (an old schema again) re-runs the migration, so to
/// keep a value the migration dropped, set it again in the app. `None` when there is no file
/// or it is already current; otherwise what was dropped (empty when only `schema` changed).
pub fn migrate_profile_file(channel_dir: &Path) -> std::io::Result<Option<Migration>> {
    let path = Profile::path(channel_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let mut p = Profile::load_file(&path)?;
    let Some(m) = p.migrate() else {
        return Ok(None);
    };
    let bak = channel_dir.join(profile_backup_name(m.from));
    if !m.is_empty() && !bak.exists() {
        std::fs::copy(&path, &bak)?;
    }
    p.save(channel_dir)?;
    Ok(Some(m))
}

/// One line of `overrides.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub line: usize,
    pub key: String,
    pub value: String,
}

/// Parse `overrides.ini` (spec 4.4): UTF-8, `;` comments, blank lines, split on the first
/// `=`, value taken raw (no trimming, no quotes). A BOM and CRLF endings are tolerated on
/// read. Lines without `=` are reported back as warnings.
pub fn parse_overrides(text: &str) -> (Vec<Override>, Vec<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    let mut bad = Vec::new();
    for (i, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        match line.split_once('=') {
            Some((k, v)) if !k.is_empty() => out.push(Override {
                line: i + 1,
                key: k.to_string(),
                value: v.to_string(),
            }),
            _ => bad.push(format!("overrides.ini line {}: no key=value", i + 1)),
        }
    }
    (out, bad)
}

/// Read a channel's overrides. Missing file = none. Returns the raw bytes too (for hashing).
pub fn load_overrides(
    channel_dir: &Path,
) -> std::io::Result<(Vec<u8>, Vec<Override>, Vec<String>)> {
    let Some(bytes) = fsutil::read_limited(&channel_dir.join(OVERRIDES_INI), OVERRIDES_LIMIT)?
    else {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    };
    let text = String::from_utf8(bytes.clone())
        .map_err(|_| std::io::Error::other("overrides.ini is not valid UTF-8"))?;
    let (o, bad) = parse_overrides(&text);
    Ok((bytes, o, bad))
}

/// Write `overrides.ini` atomically, keeping the previous version as `overrides.ini.bak`.
pub fn save_overrides(channel_dir: &Path, text: &str) -> std::io::Result<()> {
    let path = channel_dir.join(OVERRIDES_INI);
    if path.exists() {
        std::fs::copy(&path, channel_dir.join("overrides.ini.bak"))?;
    }
    fsutil::write_atomic(&path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_parse_raw_values() {
        let (o, bad) = parse_overrides(
            "; comment\n\nvehicle_NameA=*05-Avenger Titan\nitem_x=  spaced = kept \nnokey\n=x\n",
        );
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].key, "vehicle_NameA");
        assert_eq!(o[0].value, "*05-Avenger Titan");
        assert_eq!(o[1].value, "  spaced = kept ");
        assert_eq!(bad.len(), 2);
    }

    #[test]
    fn profile_keeps_unknown_keys_and_reads_disabled_patches() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join(PROFILE_TOML),
            "preset = \"minimal\"\nfuture = 3\n[patches]\ndisabled = [\"x\"]\n",
        )
        .unwrap();
        let p = Profile::load_file(&Profile::path(t.path())).unwrap();
        assert_eq!(p.disabled_patches(), vec!["x".to_string()]);
        p.save(t.path()).unwrap();
        let s = std::fs::read_to_string(t.path().join(PROFILE_TOML)).unwrap();
        assert!(s.contains("future = 3"));
        assert!(Profile::preset("nope").is_none());
        assert_ne!(
            Profile::preset("minimal").unwrap().hash(),
            Profile::preset("everything").unwrap().hash()
        );
    }

    #[test]
    fn profile_hash_follows_generator_settings_only() {
        let parse = |s: &str| Profile {
            table: toml::from_str(s).unwrap(),
        };
        let base = parse("schema = 1\npreset = \"recommended\"\n[patches]\ndisabled = []\n");
        // Keys that do not feed the generators (or have their own hash) leave it alone.
        let other = parse(
            "schema = 1\npreset = \"recommended\"\nfuture = 3\n[patches]\ndisabled = [\"x\"]\n[ship_names]\nuse = \"off\"\n",
        );
        assert_eq!(base.hash(), other.hash());
        assert_ne!(base.legacy_hash(), other.legacy_hash());
        // Spelling out a default is no change; a real setting is.
        let same = parse("preset = \"recommended\"\n[enhance]\nmissions = true\n");
        assert_eq!(base.hash(), same.hash());
        let changed = parse("preset = \"recommended\"\n[enhance]\nmissions = false\n");
        assert_ne!(base.hash(), changed.hash());
        // A key from a newer build is ignored: no change to what is written.
        assert_eq!(base.hash(), parse("[enhance]\nno_such = 1\n").hash());
        // Invalid: still changes with the text.
        let bad1 = parse("[enhance.mission]\nheader_em = 7\n");
        let bad2 = parse("[enhance.mission]\nheader_em = 8\n");
        assert!(bad1.hash().starts_with("invalid:"));
        assert_ne!(bad1.hash(), bad2.hash());
    }

    /// A schema-1 file as the app wrote it before schema 2: every key, `owned_em` at the
    /// old default 0, plus two real choices.
    const SCHEMA1: &str = "preset = \"recommended\"\nschema = 1\n[enhance]\nbranding = true\ncolour_test = false\nships = true\n[enhance.mission]\nheader_em = 3\nlabel_em = 4\nmax_list = 25\nmissing_em = 0\nmissing_marker = false\nowned_em = 0\nowned_line = false\nreward_em = 0\ntitle_bp_em = 4\ntitle_owned = true\ntitle_rep_em = 2\nvariant_em = 4\n[patches]\ndisabled = [\"p1\"]\n";

    fn parse(s: &str) -> Profile {
        Profile {
            table: toml::from_str(s).unwrap(),
        }
    }

    fn keys(v: &[&str]) -> Vec<String> {
        v.iter().map(|k| format!("enhance.{k}")).collect()
    }

    #[test]
    fn schema1_migration_drops_old_and_current_defaults() {
        let mut p = parse(SCHEMA1);
        let before = p.enhance().unwrap();
        let m = p.migrate().unwrap();
        // Only owned_em changes its effective value (0 was the old default, 4 is now).
        assert_eq!(m.followed, keys(&["mission.owned_em"]));
        assert_eq!(
            m.unpinned,
            keys(&[
                "mission.header_em",
                "mission.label_em",
                "mission.title_bp_em",
                "mission.owned_line",
                "mission.missing_marker",
                "mission.missing_em",
                "mission.variant_em",
                "mission.reward_em",
                "colour_test",
                "mission.max_list",
                "ships",
            ])
        );
        assert_eq!(p.schema(), PROFILE_SCHEMA);
        // Truly sparse: only the real choices are left, exactly as saved.
        let expect: toml::Table =
            toml::from_str("branding = true\n[mission]\ntitle_owned = true\ntitle_rep_em = 2\n")
                .unwrap();
        assert_eq!(p.table["enhance"].as_table().unwrap(), &expect);
        assert_eq!(p.disabled_patches(), vec!["p1".to_string()]);
        // Readers already saw the migrated settings: owned_em follows the new default.
        let after = p.enhance().unwrap();
        assert_eq!(after, before);
        assert_eq!(after.mission.owned_em, 4);
        assert!(after.mission.title_owned && after.branding);
        assert_eq!(after.mission.title_rep_em, 2);
        // Idempotent: a second run changes nothing.
        let once = p.clone();
        assert!(p.migrate().is_none());
        assert_eq!(p, once);
    }

    #[test]
    fn schema1_migration_drops_defaults_of_the_files_preset() {
        // Every key a schema-1 save wrote, at the preset's values: all of it goes.
        for preset in PRESETS {
            let full = crate::enhance::profile::Profile::preset(preset).unwrap();
            let mut t = toml::Table::new();
            t.insert("schema".into(), toml::Value::Integer(1));
            t.insert("preset".into(), toml::Value::String((*preset).into()));
            t.insert("enhance".into(), toml::Value::try_from(&full).unwrap());
            let mut p = Profile { table: t };
            let hash = p.hash();
            let m = p.migrate().unwrap();
            assert!(m.followed.is_empty(), "{preset}: {m:?}");
            assert!(m.unpinned.len() > 40, "{preset}: {}", m.unpinned.len());
            assert!(!p.table.contains_key("enhance"), "{preset}: {:?}", p.table);
            assert_eq!(p.hash(), hash);
            assert_eq!(p.enhance().unwrap(), full);
        }
        // Minimal's `ships = false` is its default; `ships = true` there is a choice.
        let mut q =
            parse("schema = 1\npreset = \"minimal\"\n[enhance]\nships = false\nmissions = true\n");
        assert_eq!(q.migrate().unwrap().unpinned, keys(&["ships"]));
        assert_eq!(
            q.table["enhance"]
                .as_table()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["missions"]
        );
        // Unknown and invalid keys stay as saved.
        let mut r = parse("schema = 1\n[enhance]\nno_such = 1\nships = \"yes\"\n");
        assert!(r.migrate().unwrap().is_empty());
        assert_eq!(r.table["enhance"].as_table().unwrap().len(), 2);
    }

    #[test]
    fn later_schemas_keep_a_deliberate_old_default() {
        // Saved by this build: owned_em = 0 is a choice (sparse files only hold choices).
        let mut p = parse("schema = 3\n[enhance.mission]\nowned_em = 0\nmax_list = 25\n");
        assert!(p.migrate().is_none());
        assert_eq!(p.enhance().unwrap().mission.owned_em, 0);
        assert!(p.table["enhance"]["mission"].get("max_list").is_some());
        // Schema 2 (the first sparse build, whose migration left other defaults pinned):
        // only keys at the current default go; owned_em = 0 there is a choice too.
        let mut s2 = parse("schema = 2\n[enhance.mission]\nowned_em = 0\nmax_list = 25\n");
        let m = s2.migrate().unwrap();
        assert_eq!((m.from, m.followed.len()), (2, 0));
        assert_eq!(m.unpinned, keys(&["mission.max_list"]));
        assert_eq!(s2.enhance().unwrap().mission.owned_em, 0);
        assert_eq!(s2.schema(), PROFILE_SCHEMA);
        // A schema-1 file with a non-default owned_em keeps it.
        let mut q = parse("schema = 1\n[enhance.mission]\nowned_em = 2\n");
        assert!(q.migrate().unwrap().is_empty());
        assert_eq!(q.enhance().unwrap().mission.owned_em, 2);
        // Emptied tables go; no schema key counts as 1.
        let mut r = parse("[enhance.mission]\nowned_em = 0\n");
        assert_eq!(r.migrate().unwrap().followed, keys(&["mission.owned_em"]));
        assert!(!r.table.contains_key("enhance"));
        assert_eq!(r.enhance().unwrap(), Default::default());
    }

    #[test]
    fn migration_leaves_the_hash_of_unchanged_settings_alone() {
        let mut p = parse(SCHEMA1);
        let read = p.hash();
        p.migrate();
        assert_eq!(p.hash(), read);
        // The same choices written sparse by hand hash the same: only branding,
        // title_owned and title_rep_em differ from the defaults.
        let sparse = parse(
            "schema = 2\n[enhance]\nbranding = true\n[enhance.mission]\ntitle_owned = true\ntitle_rep_em = 2\n",
        );
        assert_eq!(sparse.hash(), read);
        // Before the in-memory migration this file would have hashed with owned_em = 0.
        let mut pinned = parse(SCHEMA1);
        pinned
            .table
            .insert("schema".into(), toml::Value::Integer(PROFILE_SCHEMA));
        assert_ne!(pinned.hash(), read);
    }

    #[test]
    fn profile_file_migrates_once_with_a_backup() {
        let t = tempfile::tempdir().unwrap();
        let path = Profile::path(t.path());
        let bak = t.path().join(PROFILE_SCHEMA1_BAK);
        // No file: nothing to do.
        assert!(migrate_profile_file(t.path()).unwrap().is_none());
        std::fs::write(&path, SCHEMA1).unwrap();
        let hash = Profile::load_file(&path).unwrap().hash();
        let m = migrate_profile_file(t.path()).unwrap().unwrap();
        assert_eq!(m.followed, keys(&["mission.owned_em"]));
        assert!(m.unpinned.contains(&"enhance.mission.max_list".to_string()));
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), SCHEMA1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("schema = 3") && !text.contains("owned_em"));
        assert!(!text.contains("max_list") && !text.contains("ships"));
        let now = Profile::load_file(&path).unwrap();
        assert_eq!(now.hash(), hash);
        assert_eq!(now.enhance().unwrap().mission.owned_em, 4);
        // Second run: nothing, and the backup still holds the original.
        assert!(migrate_profile_file(t.path()).unwrap().is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), SCHEMA1);
        // A schema-1 preset-only file is marked current without a backup.
        let u = tempfile::tempdir().unwrap();
        std::fs::write(
            Profile::path(u.path()),
            "preset = \"minimal\"\nschema = 1\n",
        )
        .unwrap();
        assert_eq!(
            migrate_profile_file(u.path()).unwrap(),
            Some(Migration {
                from: 1,
                ..Migration::default()
            })
        );
        assert!(!u.path().join(PROFILE_SCHEMA1_BAK).exists());
        assert_eq!(
            Profile::load_file(&Profile::path(u.path()))
                .unwrap()
                .schema(),
            PROFILE_SCHEMA
        );
        // A schema-2 file left full by the first sparse build: backed up under its schema.
        let w = tempfile::tempdir().unwrap();
        let full = "preset = \"recommended\"\nschema = 2\n[enhance]\nships = true\n[enhance.mission]\nowned_em = 4\n[enhance.tags.components.styles]\n";
        std::fs::write(Profile::path(w.path()), full).unwrap();
        let m = migrate_profile_file(w.path()).unwrap().unwrap();
        assert_eq!(m.unpinned, keys(&["mission.owned_em", "ships"]));
        assert_eq!(
            std::fs::read_to_string(w.path().join(profile_backup_name(2))).unwrap(),
            full
        );
        let t2: toml::Table =
            toml::from_str(&std::fs::read_to_string(Profile::path(w.path())).unwrap()).unwrap();
        assert!(!t2.contains_key("enhance"), "{t2:?}");
        // New profiles start current.
        assert_eq!(
            Profile::preset("recommended").unwrap().schema(),
            PROFILE_SCHEMA
        );
    }

    #[test]
    fn overrides_bak_is_kept() {
        let t = tempfile::tempdir().unwrap();
        save_overrides(t.path(), "a=1\n").unwrap();
        save_overrides(t.path(), "a=2\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(t.path().join("overrides.ini.bak")).unwrap(),
            "a=1\n"
        );
    }
}
