//! Dataset stamps: what each cached dataset was made by.
//!
//! Beside every `<dataset>.json` of a build folder sits `<dataset>.stamp`, a small JSON file:
//! the key the dataset was written under ([`expected_code`]), where it came from (`local`:
//! extracted from this computer's `Data.p4k`; `dump:<tag>@r<n>-<commit12>`, or
//! `dump:<tag>@<commit12>` for a dump without a checked revision: a verified prebuilt dataset
//! of the same build from `voidput/sc-gamedata-dumps`, [`crate::prebuilt`], with `dumpRef`,
//! the tag ref it was read under; `derived`: built from other datasets), and the size and
//! SHA-256 of the file it describes. A `dump:<tag>` stamp of an older version names no
//! revision: it is served until the tag could be asked, then taken again.
//!
//! A dataset is served only while its stamp is current ([`current`]): the key is the one this
//! app expects for it now and the file is the one the stamp describes. Anything else (no
//! stamp, another key, another file) is extracted again. One file per dataset, written after
//! the dataset with a temp file and a rename, and only when the file on disk is the one the
//! writer made ([`write`]'s `expect`), so a stamp never vouches for another process's file.
//!
//! A served file is replaced as a pair with its stamp ([`swap`]): the stamp first names the
//! new file as `next` beside the old one, then the new file is renamed into place, then the
//! stamp names it alone. A crash in between leaves a stamp that still vouches for whichever
//! file is there ([`keyed`] picks `next` when the file is that one).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/dataset_codes.rs"));

/// Bumped when a stamp's meaning changes (every stamp is then stale). 2: dump stamps carry
/// their acceptance rule, the search index its input files.
pub const STAMP_FORMAT: u32 = 2;

/// Suffix of a stamp file.
pub const EXT: &str = ".stamp";

/// `local` source: extracted from the channel's own `Data.p4k`.
pub const LOCAL: &str = "local";
/// `derived` source: built from other datasets of the folder (the search index).
pub const DERIVED: &str = "derived";
/// Prefix of a prebuilt dataset's source: `dump:<tag>@<revision>`.
pub const DUMP_PREFIX: &str = "dump:";

/// The source of a dataset taken from revision `revision` (`r3-9a387f3c0d1e`, or a short
/// commit id; [`crate::prebuilt::Fetched::revision`]) of the dump tagged `tag`:
/// `dump:<tag>@<revision>`.
pub fn dump_source(tag: &str, revision: &str) -> String {
    format!("{DUMP_PREFIX}{tag}@{revision}")
}

/// The tag and revision of a `dump:` source (`None` revision: a stamp written before
/// revisions were recorded).
pub fn dump_parts(source: &str) -> Option<(&str, Option<&str>)> {
    let rest = source.strip_prefix(DUMP_PREFIX)?;
    Some(match rest.split_once('@') {
        Some((tag, rev)) => (tag, Some(rev)),
        None => (rest, None),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    /// [`expected_code`] when it was written.
    pub code: String,
    /// [`LOCAL`], [`DERIVED`] or `dump:<tag>@<revision>` ([`dump_source`]).
    pub source: String,
    pub bytes: u64,
    pub sha256: String,
    /// A prebuilt dataset: the dump tag's ref when it was read
    /// ([`starorg_core::source::dump::DumpTransport::tag_ref`]); the same ref later means the
    /// same revision, so checking it takes one request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dump_ref: Option<String>,
    /// The file replacing this one, while [`swap`] runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Stamp>>,
}

impl Stamp {
    /// The dump tag of a prebuilt dataset.
    pub fn dump_tag(&self) -> Option<&str> {
        dump_parts(&self.source).map(|p| p.0)
    }

    /// The dump revision of a prebuilt dataset (`r3-<commit12>` or a short commit id), when
    /// its stamp names one.
    pub fn dump_revision(&self) -> Option<&str> {
        dump_parts(&self.source).and_then(|p| p.1)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn raw_code(dataset: &str) -> Option<&'static str> {
    DATASET_CODES
        .iter()
        .find(|(d, _)| *d == dataset)
        .map(|(_, c)| *c)
}

/// The code key of a dataset: the hash of the code that writes it (`build.rs`), the cache
/// format and the stamp format. A dataset `build.rs` does not know gets a key of every
/// dataset's code, so it is still extracted again whenever any of that code changes.
pub fn code_of(dataset: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!(
        "{}:{STAMP_FORMAT}:{dataset}:",
        crate::store::CACHE_FORMAT
    ));
    match raw_code(dataset) {
        Some(c) => h.update(c),
        None => {
            for (d, c) in DATASET_CODES {
                h.update(format!("{d}={c};"));
            }
        }
    }
    hex(&h.finalize()[..8])
}

/// The key a stamp of `dataset` from `source` must carry in `dir` now: [`code_of`], plus for
/// a prebuilt dataset the rule it was accepted under ([`crate::prebuilt::rule_key`]: a
/// changed rule takes it again or extracts it), and for the search index the files it was
/// built from (the item-tags and ships stamps' SHA-256).
pub fn expected_code(dir: &Path, dataset: &str, source: &str) -> String {
    let base = code_of(dataset);
    let mut extra = String::new();
    if source.starts_with(DUMP_PREFIX) {
        extra.push_str(&crate::prebuilt::rule_key(dataset));
    }
    if dataset == starorg_gamedata::search_index::NAME {
        for input in ["game-item-tags", "game-ships"] {
            extra.push_str(&keyed(dir, input).map(|s| s.sha256).unwrap_or_default());
            extra.push(';');
        }
    }
    if extra.is_empty() {
        return base;
    }
    let mut h = Sha256::new();
    h.update(base);
    h.update(extra);
    hex(&h.finalize()[..8])
}

/// `<dir>/<dataset>.stamp`.
pub fn path(dir: &Path, dataset: &str) -> PathBuf {
    dir.join(format!("{dataset}{EXT}"))
}

/// The stamp beside a dataset, as written (current or not).
pub fn read(dir: &Path, dataset: &str) -> Option<Stamp> {
    let b = std::fs::read(path(dir, dataset)).ok()?;
    serde_json::from_slice(&b).ok()
}

/// SHA-256 of bytes, lower-case hex.
pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// SHA-256 and size of a file.
pub fn file_sum(file: &Path) -> std::io::Result<(String, u64)> {
    let bytes = std::fs::read(file)?;
    Ok((sha256(&bytes), bytes.len() as u64))
}

/// Stamp `<dir>/<dataset>.json` as it is now, from `source`. `expect`: the SHA-256 of what
/// the writer wrote; when the file on disk is another (a second process replaced it
/// meanwhile) nothing is stamped and this fails. Written with a temp file and a rename.
pub fn write(
    dir: &Path,
    dataset: &str,
    source: &str,
    expect: Option<&str>,
) -> std::io::Result<Stamp> {
    let (sha256, bytes) = file_sum(&dir.join(format!("{dataset}.json")))?;
    if let Some(e) = expect
        && e != sha256
    {
        return Err(std::io::Error::other(format!(
            "{dataset}.json was replaced by another process before it was stamped"
        )));
    }
    let s = Stamp {
        code: expected_code(dir, dataset, source),
        source: source.to_string(),
        bytes,
        sha256,
        dump_ref: None,
        next: None,
    };
    put(dir, dataset, &s)?;
    Ok(s)
}

/// Write a stamp file as given (temp file and rename).
fn put(dir: &Path, dataset: &str, s: &Stamp) -> std::io::Result<()> {
    let text = serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?;
    let target = path(dir, dataset);
    let tmp = dir.join(format!("{dataset}{EXT}.tmp-{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    if let Err(e) = starorg_core::fsutil::rename_retry(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Replace `<dir>/<dataset>.json` with `new_file` (a finished temp file in `dir`) and its
/// stamp from `source` as one pair: the stamp names the new file as `next` first, the file
/// is renamed into place, then the stamp names it alone. At every step the stamp vouches for
/// the file on disk, the old one or the new one ([`keyed`]); a failed rename leaves the old
/// pair and removes the temp file.
pub fn swap(
    dir: &Path,
    dataset: &str,
    new_file: &Path,
    source: &str,
    dump_ref: Option<&str>,
) -> std::io::Result<Stamp> {
    let (sha256, bytes) = file_sum(new_file)?;
    let new = Stamp {
        code: expected_code(dir, dataset, source),
        source: source.to_string(),
        bytes,
        sha256,
        dump_ref: dump_ref.map(str::to_string),
        next: None,
    };
    let file = dir.join(format!("{dataset}.json"));
    if let Some(mut old) = read(dir, dataset).filter(|_| file.is_file()) {
        old.next = Some(Box::new(new.clone()));
        if let Err(e) = put(dir, dataset, &old) {
            let _ = std::fs::remove_file(new_file);
            return Err(e);
        }
    }
    if let Err(e) = starorg_core::fsutil::rename_retry(new_file, &file) {
        let _ = std::fs::remove_file(new_file);
        return Err(e);
    }
    put(dir, dataset, &new)?;
    Ok(new)
}

/// Point a prebuilt dataset's stamp at the dump tag's ref now (the same revision, pushed
/// again), so the next check is one request again. Only while the file is the stamped one.
pub fn set_dump_ref(dir: &Path, dataset: &str, dump_ref: &str) -> std::io::Result<()> {
    let Some(mut s) = read(dir, dataset).filter(|s| s.next.is_none()) else {
        return Ok(());
    };
    s.dump_ref = Some(dump_ref.to_string());
    put(dir, dataset, &s)
}

/// The stamp when its key is the one expected now (file not checked; while [`swap`] runs,
/// the stamp of whichever file has the size on disk).
pub fn keyed(dir: &Path, dataset: &str) -> Option<Stamp> {
    let s = read(dir, dataset)?;
    let len = std::fs::metadata(dir.join(format!("{dataset}.json")))
        .ok()
        .map(|m| m.len());
    effective(s, len).filter(|s| s.code == expected_code(dir, dataset, &s.source))
}

/// Of a stamp and its `next`, the one for a file of `len` bytes (the stamp itself when
/// neither or both fit).
fn effective(mut s: Stamp, len: Option<u64>) -> Option<Stamp> {
    let next = s.next.take();
    match next {
        Some(n) if len == Some(n.bytes) && len != Some(s.bytes) => Some(*n),
        _ => Some(s),
    }
}

/// Why `<dir>/<dataset>.json` is not current: missing, no stamp, a stamp of another key, or
/// a file other than the one stamped. `Ok` with the stamp when it may be served (the caller
/// adds its own rules for prebuilt datasets, [`crate::store::Store::is_current`]). Reads and
/// hashes the file.
pub fn current(dir: &Path, dataset: &str) -> Result<Stamp, String> {
    let file = dir.join(format!("{dataset}.json"));
    if !file.is_file() {
        return Err(format!("{dataset} is not cached"));
    }
    let mut s = read(dir, dataset).ok_or_else(|| format!("{dataset} has no stamp"))?;
    let (sha, bytes) = file_sum(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    // While a swap runs: the new file's stamp once it is in place.
    if let Some(n) = s.next.take()
        && n.bytes == bytes
        && n.sha256 == sha
    {
        s = *n;
    }
    if s.code != expected_code(dir, dataset, &s.source) {
        return Err(format!("{dataset} was made by other code"));
    }
    if bytes != s.bytes || sha != s.sha256 {
        return Err(format!("{dataset} is not the file its stamp describes"));
    }
    Ok(s)
}

/// [`current`] when the file's size alone matches its stamp (no hashing), and a prebuilt
/// dataset is not one [`crate::prebuilt::known_bad`]: for listings.
pub fn looks_current(dir: &Path, dataset: &str) -> bool {
    let Some(s) = keyed(dir, dataset) else {
        return false;
    };
    if s.dump_tag()
        .is_some_and(|t| crate::prebuilt::known_bad(t, dataset).is_some())
    {
        return false;
    }
    std::fs::metadata(dir.join(format!("{dataset}.json")))
        .is_ok_and(|m| m.is_file() && m.len() == s.bytes)
}

/// A digest of every stamp in a folder (code and file of each dataset): a value built from
/// the folder's datasets is current while this is the same.
pub fn digest(dir: &Path) -> String {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.ends_with(EXT))
        .collect();
    names.sort();
    let mut h = Sha256::new();
    for n in names {
        if let Some(s) = read(dir, n.trim_end_matches(EXT)) {
            h.update(format!("{n}={}:{}:{};", s.code, s.sha256, s.source));
        }
    }
    hex(&h.finalize()[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modules(dataset: &str) -> &'static str {
        DATASET_MODULES
            .iter()
            .find(|(d, _)| *d == dataset)
            .map(|(_, m)| *m)
            .unwrap_or_default()
    }

    #[test]
    fn every_dataset_has_its_own_code_key() {
        let mut all = crate::worker::extractable();
        all.push(starorg_gamedata::search_index::NAME);
        for d in &all {
            assert!(raw_code(d).is_some(), "build.rs gives {d} no key");
        }
        assert_eq!(
            DATASET_CODES.len(),
            all.len(),
            "build.rs keys exactly the datasets: {DATASET_CODES:?}"
        );
        // Domains of their own code get keys of their own.
        assert_ne!(code_of("game-ships"), code_of("game-loot"));
        assert_ne!(code_of("game-strings-english"), code_of("game-ships"));
        assert_eq!(code_of("game-nope").len(), 16);
    }

    #[test]
    fn code_keys_cover_the_modules_a_domain_uses_and_not_the_query_layer() {
        let ships: Vec<&str> = modules("game-ships").split(' ').collect();
        for m in [
            "domains/ships",
            "domains/components",
            "helpers_ships",
            "records",
            "pipeline",
        ] {
            assert!(ships.contains(&m), "game-ships lacks {m}: {ships:?}");
        }
        assert!(modules("game-starmap").contains("domains/socpak"));
        assert!(modules("game-refining").contains("domains/commodities"));
        for (d, m) in DATASET_MODULES {
            assert!(
                !m.split(' ')
                    .any(|x| x.starts_with("query") || x == "fixtures"),
                "{d}: {m}"
            );
            assert!(
                !m.contains("domains/ships") || *d == "game-ships",
                "{d}: {m}"
            );
        }
    }

    #[test]
    fn stamp_vouches_only_for_the_file_it_describes() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        assert!(current(d, "game-loot").is_err());
        std::fs::write(d.join("game-loot.json"), "{}").unwrap();
        assert!(current(d, "game-loot").unwrap_err().contains("no stamp"));
        // Another process's file: not stamped.
        assert!(write(d, "game-loot", LOCAL, Some("00")).is_err());
        assert!(read(d, "game-loot").is_none());
        let s = write(d, "game-loot", LOCAL, Some(&sha256(b"{}"))).unwrap();
        assert_eq!(current(d, "game-loot").unwrap(), s);
        assert!(looks_current(d, "game-loot"));
        let before = digest(d);
        // Replaced since: refused.
        std::fs::write(d.join("game-loot.json"), "{ }").unwrap();
        assert!(
            current(d, "game-loot")
                .unwrap_err()
                .contains("not the file")
        );
        // Other code: refused.
        let mut other = s.clone();
        other.code = "0000000000000000".into();
        std::fs::write(path(d, "game-loot"), serde_json::to_vec(&other).unwrap()).unwrap();
        assert!(current(d, "game-loot").unwrap_err().contains("other code"));
        assert!(!looks_current(d, "game-loot"));
        assert_ne!(digest(d), before);
        // A dump stamp carries its rule: not the local key.
        let dump = write(d, "game-loot", &dump_source("4.10.201.48085", "r3"), None).unwrap();
        assert_eq!(dump.source, "dump:4.10.201.48085@r3");
        assert_eq!(dump.dump_tag(), Some("4.10.201.48085"));
        assert_eq!(dump.dump_revision(), Some("r3"));
        // A stamp of an older version: the tag, no revision.
        assert_eq!(
            dump_parts("dump:4.10.201.48085"),
            Some(("4.10.201.48085", None))
        );
        assert_eq!(dump_parts(LOCAL), None);
        assert_ne!(dump.code, code_of("game-loot"));
        assert!(current(d, "game-loot").is_ok());
        // A known-bad dump is never listed.
        std::fs::write(d.join("game-ships.json"), "{}").unwrap();
        write(d, "game-ships", "dump:4.10.200.58746", None).unwrap();
        assert!(!looks_current(d, "game-ships"));
    }

    #[test]
    fn a_swap_keeps_a_valid_pair_at_every_step() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        let src = |r: &str| dump_source("4.10.201.48085", r);
        std::fs::write(d.join("game-loot.json"), "{\"v\":1}").unwrap();
        let old = write(d, "game-loot", &src("r2-aaaaaaaaaaaa"), None).unwrap();
        // Swapped in: the new pair, with its ref.
        std::fs::write(d.join("new.tmp"), "{\"v\":22}").unwrap();
        let new = swap(
            d,
            "game-loot",
            &d.join("new.tmp"),
            &src("r3-bbbbbbbbbbbb"),
            Some("ref3"),
        )
        .unwrap();
        assert!(!d.join("new.tmp").exists());
        assert_eq!(current(d, "game-loot").unwrap(), new);
        assert_eq!(
            keyed(d, "game-loot").unwrap().dump_ref.as_deref(),
            Some("ref3")
        );
        // A crash after the stamp named `next` but before the rename: the old pair holds.
        let mut pending = old.clone();
        pending.next = Some(Box::new(new.clone()));
        std::fs::write(d.join("game-loot.json"), "{\"v\":1}").unwrap();
        put(d, "game-loot", &pending).unwrap();
        assert_eq!(current(d, "game-loot").unwrap(), old);
        assert_eq!(keyed(d, "game-loot").unwrap().source, old.source);
        // ... and after the rename, before the final stamp: the new pair holds.
        std::fs::write(d.join("game-loot.json"), "{\"v\":22}").unwrap();
        assert_eq!(current(d, "game-loot").unwrap(), new);
        assert_eq!(keyed(d, "game-loot").unwrap().source, new.source);
        // Neither file: refused.
        std::fs::write(d.join("game-loot.json"), "{\"v\":333}").unwrap();
        assert!(current(d, "game-loot").is_err());
        // A failed swap (no temp file): the old pair is untouched.
        std::fs::write(d.join("game-loot.json"), "{\"v\":1}").unwrap();
        put(d, "game-loot", &old).unwrap();
        assert!(swap(d, "game-loot", &d.join("gone.tmp"), &src("r4-c"), None).is_err());
        assert_eq!(current(d, "game-loot").unwrap(), old);
        // The ref moves on its own when the same revision was pushed again.
        set_dump_ref(d, "game-loot", "ref3b").unwrap();
        assert_eq!(
            read(d, "game-loot").unwrap().dump_ref.as_deref(),
            Some("ref3b")
        );
        assert!(current(d, "game-loot").is_ok());
    }

    #[test]
    fn the_search_index_is_bound_to_the_files_it_was_built_from() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        let name = starorg_gamedata::search_index::NAME;
        for f in ["game-item-tags", "game-ships", name] {
            std::fs::write(d.join(format!("{f}.json")), "{}").unwrap();
        }
        write(d, "game-item-tags", LOCAL, None).unwrap();
        write(d, "game-ships", LOCAL, None).unwrap();
        write(d, name, DERIVED, None).unwrap();
        assert!(current(d, name).is_ok());
        std::fs::write(d.join("game-ships.json"), "{\"v\":2}").unwrap();
        write(d, "game-ships", LOCAL, None).unwrap();
        assert!(current(d, name).is_err(), "ships changed: built again");
    }
}
