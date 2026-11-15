//! Owned blueprints per character (addendum §3) and the Smart Citizen import (addendum §4).
//!
//! Everything lives in the channel's data folder, `channels/<install-id>/<CHANNEL>/blueprints/`:
//!
//! | File | Content |
//! |---|---|
//! | `characters.json` | Characters seen in this channel's logs, keyed by `geid` |
//! | `<geid>.json` | That character's owned set |
//! | `unattributed.json` | Blueprints seen before any login line in their log file |
//! | `scan.json` | Per log file: size, mtime, head hash, offset, character at the offset |
//!
//! `geid`, `accountId` and handles are personal data: they stay in these files and in the
//! user's own terminal, are never uploaded, and are redacted in `doctor`.

pub mod catalog;
pub mod import;
pub mod logscan;

use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA: u32 = 1;
pub const DIR: &str = "blueprints";
pub const CHARACTERS_JSON: &str = "characters.json";
pub const UNATTRIBUTED_JSON: &str = "unattributed.json";
pub const SCAN_JSON: &str = "scan.json";

/// Source kinds stored per entry.
pub const SOURCE_LOG: &str = "log";
pub const SOURCE_SMART_CITIZEN: &str = "import:smart-citizen";

/// Drop every leading `[...]` group (Smart Citizen's Tag Builder prefixes such as
/// `[CIV-S3-A]` or `[H]`, repeatable), then trim.
pub fn strip_tags(name: &str) -> &str {
    let mut s = name.trim_start();
    while s.starts_with('[') {
        match s.find(']') {
            Some(end) => s = s[end + 1..].trim_start(),
            None => break,
        }
    }
    s.trim()
}

/// Matching key: tags stripped, quotes and apostrophes unified, whitespace collapsed,
/// lower case.
pub fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut space = false;
    for ch in strip_tags(name).chars() {
        let ch = match ch {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '`' | '\u{00B4}'
            | '\u{02BC}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
            c => c,
        };
        if ch.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// Where an entry came from. One per kind; the earliest time wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub kind: String,
    /// Log line time, or the export's `exportedAt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Log file (relative to the channel folder) for `log`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedEntry {
    /// Display name with tags stripped.
    pub name: String,
    /// Matched `GameData` blueprint ids (empty for log entries until matched; several when
    /// the name was ambiguous).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ambiguous: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    #[serde(default)]
    pub sources: Vec<Source>,
}

impl OwnedEntry {
    pub fn add_source(&mut self, src: Source) {
        match self.sources.iter_mut().find(|s| s.kind == src.kind) {
            Some(s) => {
                if let Some(t) = &src.time
                    && s.time.as_ref().is_none_or(|old| t < old)
                {
                    s.time = src.time;
                    s.file = src.file;
                }
            }
            None => self.sources.push(src),
        }
    }
}

/// One character's (or the unattributed) owned set, keyed by [`normalize`]d name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedSet {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geid: Option<String>,
    #[serde(default)]
    pub entries: BTreeMap<String, OwnedEntry>,
}

impl OwnedSet {
    pub fn new(geid: Option<&str>) -> Self {
        Self {
            schema: SCHEMA,
            geid: geid.map(str::to_string),
            entries: BTreeMap::new(),
        }
    }

    /// Union in one name; returns true when the name was new.
    pub fn add(&mut self, raw_name: &str, src: Source) -> bool {
        let key = normalize(raw_name);
        if key.is_empty() {
            return false;
        }
        let mut new = false;
        let e = self.entries.entry(key).or_insert_with(|| {
            new = true;
            OwnedEntry {
                name: strip_tags(raw_name).to_string(),
                ids: Vec::new(),
                ambiguous: false,
                favorite: false,
                sources: Vec::new(),
            }
        });
        e.add_source(src);
        new
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// Latest name seen.
    pub name: String,
    /// Time of the newest `STATE_CURRENT` line (log time, RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Characters {
    #[serde(default)]
    pub characters: BTreeMap<String, Character>,
}

impl Characters {
    /// Record a login line; keeps the latest name.
    pub fn seen(&mut self, geid: &str, account_id: Option<&str>, name: &str, time: Option<&str>) {
        let c = self
            .characters
            .entry(geid.to_string())
            .or_insert_with(|| Character {
                account_id: None,
                name: name.to_string(),
                last_seen: None,
            });
        let newer = match (time, &c.last_seen) {
            (Some(t), Some(old)) => t >= old.as_str(),
            (Some(_), None) => true,
            (None, _) => c.last_seen.is_none(),
        };
        if newer {
            c.name = name.to_string();
            if let Some(t) = time {
                c.last_seen = Some(t.to_string());
            }
        }
        if let Some(a) = account_id {
            c.account_id = Some(a.to_string());
        }
    }

    /// The character of the newest session.
    pub fn newest(&self) -> Option<&str> {
        self.characters
            .iter()
            .max_by(|a, b| a.1.last_seen.cmp(&b.1.last_seen))
            .map(|(g, _)| g.as_str())
    }

    /// Resolve `<name|geid>`. A name must match exactly one geid (case-insensitive).
    pub fn resolve(&self, who: &str) -> Result<String, ResolveError> {
        if self.characters.contains_key(who) {
            return Ok(who.to_string());
        }
        let hits: Vec<&String> = self
            .characters
            .iter()
            .filter(|(_, c)| c.name.eq_ignore_ascii_case(who))
            .map(|(g, _)| g)
            .collect();
        match hits.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(ResolveError::Unknown(who.to_string())),
            _ => Err(ResolveError::Ambiguous(who.to_string(), hits.len())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("no character {0} in this channel's logs (run `blueprints scan` or play once)")]
    Unknown(String),
    #[error("the name {0} matches {1} characters; give the geid instead")]
    Ambiguous(String, usize),
}

/// The `[Owned]` source for an apply (addendum §3 "active character").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// The character of the newest session in this channel's logs (default).
    Auto,
    /// Union over every known character.
    Any,
    /// One character, by geid.
    Character(String),
}

impl Choice {
    /// From `channel.toml`'s `active_character` (or `--character`). Names are resolved.
    pub fn parse(value: Option<&str>, chars: &Characters) -> Result<Self, ResolveError> {
        match value.map(str::trim) {
            None | Some("") | Some("auto") => Ok(Self::Auto),
            Some("any") => Ok(Self::Any),
            Some(who) => chars.resolve(who).map(Self::Character),
        }
    }
}

/// What an apply uses for `[Owned]` markers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Selection {
    /// `auto`, `any` or `character`.
    pub mode: String,
    /// geid(s) whose sets were used.
    pub characters: Vec<String>,
    /// Normalised names.
    #[serde(skip)]
    pub names: BTreeSet<String>,
    /// Matched `GameData` blueprint ids (lower case), from entries an import matched.
    #[serde(skip)]
    pub ids: BTreeSet<String>,
    pub count: usize,
    /// Empty when the set is empty, so channels without blueprint data never look stale.
    pub hash: String,
}

/// The `blueprints/` folder of a channel data folder.
pub fn dir(channel_data: &Path) -> PathBuf {
    channel_data.join(DIR)
}

fn set_path(channel_data: &Path, geid: &str) -> Result<PathBuf, String> {
    if geid.is_empty() || !geid.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(format!("bad character id {geid:?}"));
    }
    Ok(dir(channel_data).join(format!("{geid}.json")))
}

pub fn load_characters(channel_data: &Path) -> Result<Characters, String> {
    fsutil::read_json(&dir(channel_data).join(CHARACTERS_JSON))
        .map(Option::unwrap_or_default)
        .map_err(|e| e.to_string())
}

pub fn save_characters(channel_data: &Path, c: &Characters) -> Result<(), String> {
    fsutil::write_json(&dir(channel_data).join(CHARACTERS_JSON), c).map_err(|e| e.to_string())
}

pub fn load_set(channel_data: &Path, geid: &str) -> Result<OwnedSet, String> {
    let p = set_path(channel_data, geid)?;
    fsutil::read_json(&p)
        .map(|o| o.unwrap_or_else(|| OwnedSet::new(Some(geid))))
        .map_err(|e| format!("{}: {e}", p.display()))
}

pub fn save_set(channel_data: &Path, set: &OwnedSet) -> Result<(), String> {
    let p = match &set.geid {
        Some(g) => set_path(channel_data, g)?,
        None => dir(channel_data).join(UNATTRIBUTED_JSON),
    };
    fsutil::write_json(&p, set).map_err(|e| format!("{}: {e}", p.display()))
}

pub fn load_unattributed(channel_data: &Path) -> Result<OwnedSet, String> {
    fsutil::read_json(&dir(channel_data).join(UNATTRIBUTED_JSON))
        .map(|o| o.unwrap_or_else(|| OwnedSet::new(None)))
        .map_err(|e| e.to_string())
}

/// The owned names an apply uses, for `choice` (`None` = the channel setting).
pub fn select(channel_data: &Path, choice_value: Option<&str>) -> Result<Selection, String> {
    let chars = load_characters(channel_data)?;
    let choice = Choice::parse(choice_value, &chars).map_err(|e| e.to_string())?;
    let (mode, geids): (&str, Vec<String>) = match &choice {
        Choice::Auto => (
            "auto",
            chars.newest().map(str::to_string).into_iter().collect(),
        ),
        Choice::Any => ("any", chars.characters.keys().cloned().collect()),
        Choice::Character(g) => ("character", vec![g.clone()]),
    };
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for g in &geids {
        for (k, e) in load_set(channel_data, g)?.entries {
            // An ambiguous match lists every candidate; only a unique id marks by id.
            if !e.ambiguous {
                ids.extend(e.ids.iter().map(|i| i.to_ascii_lowercase()));
            }
            names.insert(k);
        }
    }
    let hash = if names.is_empty() {
        String::new()
    } else {
        let mut joined: Vec<&str> = names.iter().map(String::as_str).collect();
        if !ids.is_empty() {
            joined.push("\0ids");
            joined.extend(ids.iter().map(String::as_str));
        }
        fsutil::sha256_hex(joined.join("\n").as_bytes())
    };
    Ok(Selection {
        mode: mode.into(),
        characters: geids,
        count: names.len(),
        names,
        ids,
        hash,
    })
}

/// The selected owned set feeds the mission generator's `[Owned]` markers (spec 6.5): a
/// blueprint is owned when its id or record name was matched on import, or when its
/// display name normalises to an owned name (log entries carry names only).
impl crate::enhance::owned::OwnedBlueprints for Selection {
    fn owns(&self, bp: &crate::enhance::owned::BlueprintRef<'_>) -> bool {
        (!bp.id.is_empty() && self.ids.contains(&bp.id.to_ascii_lowercase()))
            || (!bp.record.is_empty() && self.ids.contains(&bp.record.to_ascii_lowercase()))
            || (!bp.name.is_empty() && self.names.contains(&normalize(bp.name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_repeated_leading_tags() {
        assert_eq!(strip_tags("[CIV-S3-A] 7CA 'Nargun'"), "7CA 'Nargun'");
        assert_eq!(strip_tags("  [H] [E-S2]  ADP Core "), "ADP Core");
        assert_eq!(strip_tags("Plain Name"), "Plain Name");
        assert_eq!(strip_tags("Name [tail]"), "Name [tail]");
        assert_eq!(strip_tags("[unclosed Name"), "[unclosed Name");
    }

    #[test]
    fn normalizes_quotes_space_case() {
        assert_eq!(
            normalize("[L]  Foo\u{2019}s   \u{201C}Bar\u{201D}  Mk II"),
            "foo's \"bar\" mk ii"
        );
        assert_eq!(normalize("7CA `Nargun\u{00B4}"), "7ca 'nargun'");
        assert_eq!(normalize("[H]"), "");
    }

    #[test]
    fn characters_resolve_and_newest() {
        let mut c = Characters::default();
        c.seen("111", Some("9"), "Alpha", Some("2026-01-01T00:00:00.000Z"));
        c.seen("222", Some("9"), "Beta", Some("2026-02-01T00:00:00.000Z"));
        c.seen("333", None, "beta", Some("2026-01-15T00:00:00.000Z"));
        assert_eq!(c.newest(), Some("222"));
        assert_eq!(c.resolve("alpha").unwrap(), "111");
        assert_eq!(c.resolve("333").unwrap(), "333");
        assert!(matches!(
            c.resolve("BETA"),
            Err(ResolveError::Ambiguous(_, 2))
        ));
        assert!(matches!(c.resolve("Gamma"), Err(ResolveError::Unknown(_))));
        // Older line does not overwrite the latest name.
        c.seen("111", None, "OldAlpha", Some("2025-01-01T00:00:00.000Z"));
        assert_eq!(c.characters["111"].name, "Alpha");
        c.seen("111", None, "NewAlpha", Some("2026-03-01T00:00:00.000Z"));
        assert_eq!(c.characters["111"].name, "NewAlpha");
        assert_eq!(c.newest(), Some("111"));
    }

    #[test]
    fn select_modes_and_empty_hash() {
        let d = tempfile::tempdir().unwrap();
        let s = select(d.path(), None).unwrap();
        assert_eq!((s.mode.as_str(), s.count, s.hash.as_str()), ("auto", 0, ""));
        let mut c = Characters::default();
        c.seen("111", None, "Alpha", Some("2026-01-01T00:00:00Z"));
        c.seen("222", None, "Beta", Some("2026-02-01T00:00:00Z"));
        save_characters(d.path(), &c).unwrap();
        let src = || Source {
            kind: SOURCE_LOG.into(),
            time: None,
            file: None,
        };
        let mut a = OwnedSet::new(Some("111"));
        a.add("[H] Thing One", src());
        save_set(d.path(), &a).unwrap();
        let mut b = OwnedSet::new(Some("222"));
        b.add("Thing Two", src());
        b.add("thing  one", src());
        save_set(d.path(), &b).unwrap();
        let auto = select(d.path(), None).unwrap();
        assert_eq!(auto.characters, vec!["222".to_string()]);
        assert_eq!(auto.count, 2);
        assert!(!auto.hash.is_empty());
        let alpha = select(d.path(), Some("alpha")).unwrap();
        assert_eq!(alpha.count, 1);
        let any = select(d.path(), Some("any")).unwrap();
        assert_eq!(any.count, 2);
        assert_eq!(any.hash, auto.hash);
        assert!(select(d.path(), Some("nobody")).is_err());
    }

    #[test]
    fn set_path_rejects_odd_ids() {
        assert!(set_path(Path::new("/x"), "../etc").is_err());
        assert!(set_path(Path::new("/x"), "").is_err());
        assert!(set_path(Path::new("/x"), "12345").is_ok());
    }

    #[test]
    fn add_source_keeps_earliest() {
        let mut s = OwnedSet::new(Some("1"));
        let t = |x: &str| Source {
            kind: SOURCE_LOG.into(),
            time: Some(x.into()),
            file: Some(x.into()),
        };
        assert!(s.add("X", t("2026-02")));
        assert!(!s.add("x", t("2026-01")));
        assert!(!s.add("X", t("2026-03")));
        let e = &s.entries["x"];
        assert_eq!(e.sources.len(), 1);
        assert_eq!(e.sources[0].time.as_deref(), Some("2026-01"));
    }
}
