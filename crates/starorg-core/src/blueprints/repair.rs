//! Keeping one character per person, and rebuilding the log-derived sets.
//!
//! Star Citizen has one character per account, but the character's `geid` changes when it is
//! reset or wiped: the logs then show the same account (and usually the same name) under a new
//! geid. Keyed by geid alone, that became a second character with an empty set, and the
//! default "newest session" choice picked the empty one, so nothing was marked `[Owned]`.
//!
//! - [`consolidate`] merges characters that share an `accountId` *and* a name into the newest
//!   one (sets unioned, earliest time per source kept), records the old geid as an alias, and
//!   lists characters that only share a name (no common account to prove it): those are kept
//!   apart and shown as a notice.
//! - [`ensure`] is the one-time repair of a store written by an older version: cleans ids and
//!   names (a `\r`, BOM or stray space made a second key), re-keys every set with the current
//!   [`normalize`](super::normalize), then consolidates. It backs the store up first and runs
//!   once per [`REPAIR_VERSION`] (recorded in `characters.json`).
//! - [`rebuild_from_logs`] re-reads every log of the channel from the start.
//!
//! Backups go to `blueprints/backups/<UTC time>-<reason>/`, a copy of every JSON file of the
//! store as it was before the change. They are never removed automatically.

use super::logscan::{self, ScanReport, Store};
use super::{Character, OwnedSet, SOURCE_LOG, clean_id, clean_name, normalize};
use crate::fsutil;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Bump when [`ensure`] gains a step that stored data needs once.
pub const REPAIR_VERSION: u32 = 1;
pub const BACKUPS: &str = "backups";

/// One character merged into another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Merge {
    pub from: String,
    pub into: String,
    /// Names the merged set added to the kept one.
    pub added: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Consolidated {
    pub merged: Vec<Merge>,
    /// Groups of geids that share a name but no account: kept apart.
    pub same_name: Vec<Vec<String>>,
}

/// Where a backup of the store goes; the time keeps backups apart.
pub fn backup_dir(channel_data: &Path, reason: &str) -> PathBuf {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let stamp = fsutil::rfc3339(secs).replace(':', "-");
    let mut p = super::dir(channel_data)
        .join(BACKUPS)
        .join(format!("{stamp}-{reason}"));
    let mut n = 1;
    while p.exists() {
        n += 1;
        p = super::dir(channel_data)
            .join(BACKUPS)
            .join(format!("{stamp}-{reason}-{n}"));
    }
    p
}

/// Copy every JSON file of the store into `to`. Nothing to copy: no folder is made.
pub fn backup(channel_data: &Path, to: &Path) -> Result<usize, String> {
    if to.exists() {
        // Already taken by an earlier save of the same change: keep the older copy.
        return Ok(0);
    }
    let dir = super::dir(channel_data);
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if !e.file_type().is_ok_and(|t| t.is_file())
            || p.extension()
                .is_none_or(|x| !x.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        if n == 0 {
            std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        }
        std::fs::copy(&p, to.join(e.file_name())).map_err(|e| format!("{}: {e}", p.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Merge characters that share an account and a name into the newest of them; list the ones that only
/// share a name. Changes `store` in memory; the next save writes it, backing up first when
/// anything was merged.
pub fn consolidate(channel_data: &Path, store: &mut Store) -> Result<Consolidated, String> {
    let mut out = Consolidated::default();
    // Same account and same name: two independent signals that it is one character with a
    // new geid. Either alone is not enough (a renamed handle, a test account with an old name).
    let mut by_account: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (g, c) in &store.characters.characters {
        let name = normalize(&c.name);
        if let Some(a) = &c.account_id
            && !name.is_empty()
        {
            by_account
                .entry((a.clone(), name))
                .or_default()
                .push(g.clone());
        }
    }
    for geids in by_account.into_values().filter(|v| v.len() > 1) {
        let chars = &store.characters.characters;
        let into = geids
            .iter()
            .max_by(|a, b| {
                chars[*a]
                    .last_seen
                    .cmp(&chars[*b].last_seen)
                    .then_with(|| a.cmp(b))
            })
            .expect("two or more")
            .clone();
        for from in geids.into_iter().filter(|g| *g != into) {
            if store.backup.is_none() {
                store.backup = Some(backup_dir(channel_data, "merge"));
            }
            let set = store.take_set(channel_data, &from)?;
            let added = store.set_mut(channel_data, &into)?.absorb(set);
            merge_character(store, &from, &into);
            out.merged.push(Merge {
                from,
                into: into.clone(),
                added,
            });
        }
    }
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (g, c) in &store.characters.characters {
        let k = normalize(&c.name);
        if !k.is_empty() {
            by_name.entry(k).or_default().push(g.clone());
        }
    }
    out.same_name = by_name.into_values().filter(|v| v.len() > 1).collect();
    Ok(out)
}

/// The character record of `from` folds into `into`; every reference follows.
fn merge_character(store: &mut Store, from: &str, into: &str) {
    if let Some(old) = store.characters.characters.remove(from) {
        let keep = store
            .characters
            .characters
            .get_mut(into)
            .expect("kept character");
        if old.last_seen > keep.last_seen {
            keep.name = old.name;
            keep.last_seen = old.last_seen;
        }
        if keep.account_id.is_none() {
            keep.account_id = old.account_id;
        }
    }
    for v in store.characters.aliases.values_mut() {
        if v == from {
            *v = into.to_string();
        }
    }
    store
        .characters
        .aliases
        .insert(from.to_string(), into.to_string());
    store.mark_characters();
    let mut moved = false;
    for f in store.scan.files.values_mut() {
        if f.geid.as_deref() == Some(from) {
            f.geid = Some(into.to_string());
            moved = true;
        }
    }
    if moved {
        store.mark_scan();
    }
}

/// What [`ensure`] changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Repaired {
    /// Character records whose id or name had stray characters, or that were the same id twice.
    pub cleaned: usize,
    /// Owned entries re-keyed (or folded together) by the current name matching.
    pub rekeyed: usize,
    /// Owned entries checked.
    pub entries: usize,
    pub merged: Vec<Merge>,
    pub same_name: Vec<Vec<String>>,
    pub backup: Option<PathBuf>,
}

impl Repaired {
    pub fn changed(&self) -> bool {
        self.cleaned > 0 || self.rekeyed > 0 || !self.merged.is_empty()
    }

    /// One line for the activity log (no names, no ids).
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.merged.is_empty() {
            let into: BTreeSet<&str> = self.merged.iter().map(|m| m.into.as_str()).collect();
            parts.push(format!(
                "merged {} characters into {}",
                self.merged.len() + into.len(),
                into.len()
            ));
        }
        if self.cleaned > 0 {
            parts.push(format!("cleaned {} character records", self.cleaned));
        }
        parts.push(format!(
            "{} of {} blueprints re-matched",
            self.rekeyed, self.entries
        ));
        if !self.same_name.is_empty() {
            parts.push(format!(
                "{} names shared by characters of different accounts left apart",
                self.same_name.len()
            ));
        }
        let mut s = parts.join("; ");
        if let Some(first) = s.get(..1) {
            s = first.to_uppercase() + &s[1..];
        }
        s
    }
}

/// The one-time repair of a store written by an older version (see the module docs). Runs
/// when `characters.json` records an older [`REPAIR_VERSION`]; `Ok(None)` when it already ran
/// or there is no store. Changes `store`; the caller saves it (which writes the backup first).
pub fn ensure(channel_data: &Path, store: &mut Store) -> Result<Option<Repaired>, String> {
    if store.characters.repair >= REPAIR_VERSION {
        return Ok(None);
    }
    let dir = super::dir(channel_data);
    let has_store = std::fs::read_dir(&dir).is_ok_and(|mut r| r.next().is_some());
    let mut rep = Repaired::default();
    if !has_store {
        // Nothing to repair or write; the first save of a new store records the version.
        store.characters.repair = REPAIR_VERSION;
        return Ok(Some(rep));
    }
    let b = backup_dir(channel_data, &format!("repair-{REPAIR_VERSION}"));
    rep.backup = Some(b.clone());
    store.backup = Some(b);

    // Ids and names: a record whose id is not a clean number is folded into the clean one.
    let old = std::mem::take(&mut store.characters.characters);
    for (g, c) in old {
        let Some(id) = clean_id(&g) else {
            rep.cleaned += 1;
            continue;
        };
        let name = clean_name(&c.name);
        let account_id = c.account_id.as_deref().and_then(clean_id);
        if id != g || name != c.name || account_id != c.account_id {
            rep.cleaned += 1;
        }
        let cleaned = Character {
            account_id,
            name,
            last_seen: c.last_seen,
        };
        match store.characters.characters.get_mut(&id) {
            Some(k) => {
                rep.cleaned += 1;
                if cleaned.last_seen > k.last_seen {
                    k.name = cleaned.name;
                    k.last_seen = cleaned.last_seen;
                }
                if k.account_id.is_none() {
                    k.account_id = cleaned.account_id;
                }
            }
            None => {
                store.characters.characters.insert(id, cleaned);
            }
        }
    }

    // Every set re-keyed by the current matching.
    let geids: Vec<String> = store.characters.characters.keys().cloned().collect();
    for g in &geids {
        let set = store.set_mut(channel_data, g)?;
        rep.entries += set.entries.len();
        rep.rekeyed += set.rekey();
    }
    let un = store.unattributed_mut(channel_data)?;
    rep.entries += un.entries.len();
    rep.rekeyed += un.rekey();

    let c = consolidate(channel_data, store)?;
    rep.merged = c.merged;
    rep.same_name = c.same_name;
    store.characters.repair = REPAIR_VERSION;
    store.mark_characters();
    Ok(Some(rep))
}

/// Per-character difference of a rebuild or re-import.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CharacterDiff {
    /// geid, or `None` for the blueprints seen before any login.
    pub id: Option<String>,
    pub name: String,
    pub before: usize,
    pub after: usize,
    /// Display names.
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// Snapshot of every set: (character, normalised name -> display name).
pub type Snapshot = BTreeMap<Option<String>, BTreeMap<String, String>>;

/// Load and record every set of the store.
pub fn snapshot(channel_data: &Path, store: &mut Store) -> Result<Snapshot, String> {
    let mut out = Snapshot::new();
    let geids: Vec<String> = store.characters.characters.keys().cloned().collect();
    for g in geids {
        let s = store.set_mut(channel_data, &g)?;
        out.insert(Some(g), names(s));
    }
    let u = store.unattributed_mut(channel_data)?;
    out.insert(None, names(u));
    Ok(out)
}

fn names(s: &OwnedSet) -> BTreeMap<String, String> {
    s.entries
        .iter()
        .map(|(k, e)| (k.clone(), e.name.clone()))
        .collect()
}

/// The difference between two snapshots, by character (merged-away characters count into
/// the character they were merged into).
pub fn diff(store: &Store, before: &Snapshot, after: &Snapshot) -> Vec<CharacterDiff> {
    let mut folded: BTreeMap<Option<String>, BTreeMap<String, String>> = BTreeMap::new();
    for (g, n) in before {
        let k = g.as_deref().map(|g| store.characters.canonical(g));
        folded
            .entry(k)
            .or_default()
            .extend(n.iter().map(|(a, b)| (a.clone(), b.clone())));
    }
    let keys: BTreeSet<Option<String>> = folded.keys().chain(after.keys()).cloned().collect();
    let empty = BTreeMap::new();
    let mut out = Vec::new();
    for k in keys {
        let b = folded.get(&k).unwrap_or(&empty);
        let a = after.get(&k).unwrap_or(&empty);
        let added: Vec<String> = a
            .iter()
            .filter(|(x, _)| !b.contains_key(*x))
            .map(|(_, n)| n.clone())
            .collect();
        let removed: Vec<String> = b
            .iter()
            .filter(|(x, _)| !a.contains_key(*x))
            .map(|(_, n)| n.clone())
            .collect();
        let name = match &k {
            Some(g) => store
                .characters
                .characters
                .get(g)
                .map_or_else(|| g.clone(), |c| c.name.clone()),
            None => String::new(),
        };
        if k.is_none() && a.is_empty() && b.is_empty() {
            continue;
        }
        out.push(CharacterDiff {
            id: k,
            name,
            before: b.len(),
            after: a.len(),
            added,
            removed,
        });
    }
    out
}

/// What [`rebuild_from_logs`] found.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RebuildReport {
    pub characters: Vec<CharacterDiff>,
    pub scan: ScanReport,
    /// `replace`: entries from every source were dropped first, not only the log ones.
    pub replace: bool,
}

impl RebuildReport {
    pub fn changes(&self) -> bool {
        self.characters
            .iter()
            .any(|c| !c.added.is_empty() || !c.removed.is_empty())
            || !self.scan.merged.is_empty()
    }
}

/// Re-read every log of the channel from the start, rebuilding the log-derived part of the
/// sets: the `log` source is dropped from every entry (entries left with no source go), then
/// the logs are scanned as if never seen. Entries from an import or StarOrg Tools stay, unless
/// `replace`, which starts every set empty. Changes `store`; the caller saves it, or does not
/// for a dry run. A save backs the store up first.
pub fn rebuild_from_logs(
    channel_dir: &Path,
    channel_data: &Path,
    store: &mut Store,
    replace: bool,
    deadline: Option<Instant>,
) -> Result<RebuildReport, String> {
    let before = snapshot(channel_data, store)?;
    store.backup = Some(backup_dir(channel_data, "rescan"));
    let geids: Vec<String> = store.characters.characters.keys().cloned().collect();
    for g in &geids {
        strip(store.set_mut(channel_data, g)?, SOURCE_LOG, replace);
    }
    strip(store.unattributed_mut(channel_data)?, SOURCE_LOG, replace);
    store.scan.files.clear();
    store.mark_scan();
    let scan = logscan::scan(channel_dir, channel_data, store, deadline);
    let after = snapshot(channel_data, store)?;
    Ok(RebuildReport {
        characters: diff(store, &before, &after),
        scan,
        replace,
    })
}

/// Drop source `kind` from every entry (every source when `all`); entries left with no source
/// go. Returns the display names removed.
pub fn strip(set: &mut OwnedSet, kind: &str, all: bool) -> Vec<String> {
    let mut removed = Vec::new();
    set.entries.retain(|_, e| {
        if all {
            e.sources.clear();
        } else {
            e.sources.retain(|s| s.kind != kind);
        }
        let keep = !e.sources.is_empty();
        if !keep {
            removed.push(e.name.clone());
        }
        keep
    });
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprints::{Characters, SOURCE_SMART_CITIZEN, Source, load_characters, load_set};

    fn src(kind: &str, t: &str) -> Source {
        Source {
            kind: kind.into(),
            time: Some(t.into()),
            file: None,
        }
    }

    /// The store an older version left after a character reset: two geids of one account
    /// (same name), the import on the old one, and an id and a name with a stray `\r`.
    fn old_store(data: &Path, same_account: bool) {
        let mut c = Characters::default();
        c.characters.insert(
            "1001".into(),
            Character {
                account_id: Some("77".into()),
                name: "Test_Pilot".into(),
                last_seen: Some("2026-01-01T00:00:00.000Z".into()),
            },
        );
        c.characters.insert(
            "2002".into(),
            Character {
                account_id: Some(if same_account { "77" } else { "88" }.into()),
                name: "Test_Pilot\r".into(),
                last_seen: Some("2026-03-01T00:00:00.000Z".into()),
            },
        );
        crate::blueprints::save_characters(data, &c).unwrap();
        let mut a = OwnedSet::new(Some("1001"));
        a.add("Fake Rifle", src(SOURCE_SMART_CITIZEN, "2026-01-05"));
        a.add("Shared Helmet", src(SOURCE_LOG, "2026-01-02"));
        // A key an older normalize left with the `\r` glued on.
        a.entries.insert(
            "old arms\r".into(),
            crate::blueprints::OwnedEntry {
                name: "Old Arms\r".into(),
                ids: vec![],
                ambiguous: false,
                favorite: false,
                sources: vec![src(SOURCE_LOG, "2026-01-03")],
            },
        );
        crate::blueprints::save_set(data, &a).unwrap();
        let mut b = OwnedSet::new(Some("2002"));
        b.add("Shared Helmet", src(SOURCE_LOG, "2026-03-02"));
        b.add("New Legs", src(SOURCE_LOG, "2026-03-03"));
        crate::blueprints::save_set(data, &b).unwrap();
    }

    #[test]
    fn repair_merges_a_reset_character_once_with_a_backup() {
        let d = tempfile::tempdir().unwrap();
        old_store(d.path(), true);
        let before = crate::blueprints::select(d.path(), None).unwrap();
        assert_eq!(before.count, 2, "auto picks the new, nearly empty geid");

        let mut s = Store::load(d.path()).unwrap();
        let r = ensure(d.path(), &mut s)
            .unwrap()
            .expect("runs on an old store");
        s.save(d.path()).unwrap();
        assert_eq!(
            r.merged,
            vec![Merge {
                from: "1001".into(),
                into: "2002".into(),
                added: 2
            }]
        );
        assert_eq!(r.cleaned, 1, "the name with a \\r");
        assert_eq!(r.rekeyed, 1, "the key with a \\r");
        assert!(r.same_name.is_empty());
        assert!(
            r.summary().starts_with("Merged 2 characters into 1"),
            "{}",
            r.summary()
        );

        let chars = load_characters(d.path()).unwrap();
        assert_eq!(chars.characters.len(), 1);
        assert_eq!(chars.characters["2002"].name, "Test_Pilot");
        assert_eq!(chars.aliases["1001"], "2002");
        assert_eq!(chars.repair, REPAIR_VERSION);
        let set = load_set(d.path(), "2002").unwrap();
        assert_eq!(
            set.entries.keys().collect::<Vec<_>>(),
            vec!["fake rifle", "new legs", "old arms", "shared helmet"]
        );
        let helmet = &set.entries["shared helmet"];
        assert_eq!(helmet.sources.len(), 1);
        assert_eq!(
            helmet.sources[0].time.as_deref(),
            Some("2026-01-02"),
            "earliest"
        );
        assert!(!crate::blueprints::dir(d.path()).join("1001.json").exists());

        // The backup holds the store as it was.
        let b = r.backup.unwrap();
        assert!(b.starts_with(crate::blueprints::dir(d.path()).join(BACKUPS)));
        let old: Characters = fsutil::read_json(&b.join("characters.json"))
            .unwrap()
            .unwrap();
        assert_eq!(old.characters.len(), 2);
        assert!(b.join("1001.json").is_file() && b.join("2002.json").is_file());

        // Owned now marks what the old geid held; the old id still resolves.
        let after = crate::blueprints::select(d.path(), None).unwrap();
        assert_eq!(after.count, 4);
        assert_ne!(after.hash, before.hash, "the channel shows stale");
        assert_eq!(
            crate::blueprints::select(d.path(), Some("1001"))
                .unwrap()
                .characters,
            vec!["2002".to_string()]
        );

        // Idempotent: runs once.
        let mut s = Store::load(d.path()).unwrap();
        assert!(ensure(d.path(), &mut s).unwrap().is_none());
        let backups = std::fs::read_dir(b.parent().unwrap()).unwrap().count();
        assert_eq!(backups, 1);
    }

    #[test]
    fn same_name_on_two_accounts_is_kept_apart() {
        let d = tempfile::tempdir().unwrap();
        old_store(d.path(), false);
        let mut s = Store::load(d.path()).unwrap();
        let r = ensure(d.path(), &mut s).unwrap().unwrap();
        s.save(d.path()).unwrap();
        assert!(r.merged.is_empty());
        assert_eq!(r.same_name, vec![vec!["1001".to_string(), "2002".into()]]);
        assert_eq!(load_characters(d.path()).unwrap().characters.len(), 2);
        assert!(r.summary().contains("left apart"), "{}", r.summary());
    }

    #[test]
    fn empty_store_needs_no_backup() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::load(d.path()).unwrap();
        let r = ensure(d.path(), &mut s).unwrap().unwrap();
        assert!(!r.changed() && r.backup.is_none());
        s.save(d.path()).unwrap();
        assert!(
            !crate::blueprints::dir(d.path()).exists(),
            "nothing written"
        );
    }

    #[test]
    fn strip_keeps_other_sources() {
        let mut s = OwnedSet::new(Some("1"));
        s.add("A", src(SOURCE_LOG, "1"));
        s.add("B", src(SOURCE_SMART_CITIZEN, "1"));
        s.add("B", src(SOURCE_LOG, "1"));
        assert_eq!(strip(&mut s, SOURCE_LOG, false), vec!["A"]);
        assert_eq!(s.entries["b"].sources.len(), 1);
        assert_eq!(strip(&mut s, SOURCE_LOG, true), vec!["B"]);
        assert!(s.entries.is_empty());
    }
}
