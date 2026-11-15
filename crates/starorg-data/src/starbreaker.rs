//! Thin adapter over the StarBreaker crates.
//!
//! Everything StarBreaker-specific goes through this module, so a rev bump touches this file
//! (and the pin in the workspace `Cargo.toml`) only. Nothing outside `starorg-data` sees a
//! StarBreaker type.

use std::path::Path;

/// The StarBreaker commit we build against. Part of the cache key (spec 4.2). Must equal the
/// `rev` in the workspace `Cargo.toml`; a test checks this.
pub const STARBREAKER_COMMIT: &str = "041e559215e18e1b13b124cab0e3ecb0f3aa50d4";

/// Error from the StarBreaker layer, as text plus a kind we can act on.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("cannot open archive: {0}")]
    Open(String),
    #[error("cannot read entry {name}: {detail}")]
    Read { name: String, detail: String },
}

/// Metadata of one archive entry that we use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    pub name: String,
    pub uncompressed_size: u64,
    pub crc32: u32,
}

/// An opened `Data.p4k`. Holds StarBreaker's in-memory index (about 400 MB on LIVE); drop it
/// as soon as the needed entries are read (spec 3.3 step 4).
pub struct Archive {
    inner: starbreaker_p4k::MappedP4k,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        starbreaker_p4k::MappedP4k::open(path)
            .map(|inner| Self { inner })
            .map_err(|e| ArchiveError::Open(e.to_string()))
    }

    /// Number of entries StarBreaker parsed from the central directory.
    pub fn parsed_entries(&self) -> usize {
        self.inner.len()
    }

    /// Look up an entry by its exact name (backslash separated), then case-insensitively.
    pub fn entry(&self, name: &str) -> Option<EntryInfo> {
        let e = self
            .inner
            .entry(name)
            .or_else(|| self.inner.entry_case_insensitive(name))?;
        Some(EntryInfo {
            name: e.name.clone(),
            uncompressed_size: e.uncompressed_size,
            crc32: e.crc32,
        })
    }

    /// Read and decompress an entry found with [`Archive::entry`].
    pub fn read(&self, info: &EntryInfo) -> Result<Vec<u8>, ArchiveError> {
        let entry = self
            .inner
            .entry(&info.name)
            .ok_or_else(|| ArchiveError::Read {
                name: info.name.clone(),
                detail: "entry vanished from index".into(),
            })?;
        self.inner.read(entry).map_err(|e| ArchiveError::Read {
            name: info.name.clone(),
            detail: e.to_string(),
        })
    }
}

/// Summary of a parsed DataCore database. M2 replaces this with the real queries that fill
/// `GameData`; M0 only proves the database parses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataCoreSummary {
    pub records: usize,
    pub struct_defs: usize,
}

/// Parse DataCore bytes. May panic on malformed data (StarBreaker accessors do); callers run
/// this under `catch_unwind` inside the worker process.
pub fn parse_datacore(bytes: &[u8]) -> Result<DataCoreSummary, String> {
    let db =
        starbreaker_datacore::database::Database::from_bytes(bytes).map_err(|e| e.to_string())?;
    Ok(DataCoreSummary {
        records: db.records().len(),
        struct_defs: db.struct_defs().len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_constant_matches_workspace_pin() {
        let manifest = include_str!("../../../Cargo.toml");
        let pins: Vec<&str> = manifest
            .lines()
            .filter(|l| l.starts_with("starbreaker-"))
            .collect();
        assert_eq!(pins.len(), 3, "three StarBreaker crates are pinned");
        for line in pins {
            assert!(
                line.contains("https://github.com/voidput/StarBreaker"),
                "StarBreaker must come from our mirror: {line}"
            );
            assert!(
                line.contains(&format!("rev = \"{STARBREAKER_COMMIT}\"")),
                "pin and STARBREAKER_COMMIT differ: {line}"
            );
        }
    }

    #[test]
    fn open_missing_file_is_an_error() {
        let err = Archive::open(Path::new("/nonexistent/Data.p4k")).err();
        assert!(matches!(err, Some(ArchiveError::Open(_))));
    }
}
