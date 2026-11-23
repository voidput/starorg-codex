//! Small in-memory caches that keep repeated reads cheap: file stamps that say when a
//! channel's table inputs changed, the Strings query results per table, and install
//! discovery for a few seconds.
//!
//! Correctness first: a stamp only proves "unchanged" once every file in it is older than
//! [`SETTLE`] (the "racy" rule git uses for its index), so a write that lands in the same
//! timestamp tick as the read before it is never missed. Write commands still drop the
//! caches they affect (`forget_tables`, `forget_installs`).

use super::table::Table;
use crate::contract::strings_query::Prepared;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant, SystemTime};

/// How old a file's mtime must be before its stamp is trusted (filesystem timestamp
/// resolution is as coarse as 2 s on FAT, 1 s on some network shares).
pub const SETTLE: Duration = Duration::from_secs(2);

/// How long a discovery answer is reused when config.toml and outputs.json did not change.
pub const INSTALLS_TTL: Duration = Duration::from_secs(5);

/// Strings queries kept (each is a sorted index list over a table, plus its facets).
pub const STRINGS_QUERIES: usize = 4;

/// Length and modification time of one path (`None`: missing or not a regular file).
pub type FileStamp = Option<(u64, SystemTime)>;

pub fn file_stamp(path: &Path) -> FileStamp {
    let m = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some((m.len(), m.modified().ok()?))
}

/// The stamps of a set of files and folders, taken at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    files: Vec<(PathBuf, FileStamp)>,
    /// When it was taken (not compared).
    taken: SystemTime,
}

impl Stamp {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            taken: SystemTime::now(),
        }
    }

    /// One file.
    pub fn file(mut self, path: PathBuf) -> Self {
        let s = file_stamp(&path);
        self.files.push((path, s));
        self
    }

    /// Every regular file directly in `dir` (a missing folder stamps as empty).
    pub fn dir(mut self, dir: &Path) -> Self {
        let mut found: Vec<(PathBuf, FileStamp)> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter_map(|p| file_stamp(&p).map(|s| (p, Some(s))))
                    .collect()
            })
            .unwrap_or_default();
        // A missing folder and an empty one read the same (no files), so they stamp alike.
        found.sort();
        self.files.extend(found);
        self
    }

    /// Same files with the same stamps (the time taken does not count).
    pub fn same(&self, other: &Stamp) -> bool {
        self.files == other.files
    }

    /// Every file was older than [`SETTLE`] when this was taken, so an equal stamp later
    /// means equal contents.
    pub fn settled(&self) -> bool {
        self.files.iter().all(|(_, s)| match s {
            None => true,
            Some((_, m)) => self.taken.duration_since(*m).is_ok_and(|age| age >= SETTLE),
        })
    }
}

impl Default for Stamp {
    fn default() -> Self {
        Self::new()
    }
}

/// What a channel's cached table was built from, beyond its own key: trusted without
/// reading the inputs again while [`Stamp::same`] and [`Stamp::settled`] hold.
#[derive(Debug, Clone)]
pub struct TableStamp {
    /// The game data key (`LoadedData::key`).
    pub data_key: String,
    /// The table key the stamp belongs to.
    pub table_key: String,
    pub files: Stamp,
}

/// Strings queries over a table, newest last. Entries hold the table weakly: a table that
/// was replaced is never matched again (its allocation stays until the entry is dropped,
/// so the pointer cannot be reused).
#[derive(Default)]
pub struct StringsCache {
    entries: VecDeque<(Weak<Table>, String, Arc<Prepared>)>,
}

impl StringsCache {
    pub fn get(&mut self, tbl: &Arc<Table>, key: &str) -> Option<Arc<Prepared>> {
        let ptr = Arc::as_ptr(tbl);
        let i = self
            .entries
            .iter()
            .position(|(t, k, _)| std::ptr::eq(t.as_ptr(), ptr) && k == key)?;
        let hit = self.entries.remove(i)?;
        let p = hit.2.clone();
        self.entries.push_back(hit);
        Some(p)
    }

    pub fn put(&mut self, tbl: &Arc<Table>, key: String, p: Arc<Prepared>) {
        self.entries.retain(|(t, _, _)| t.strong_count() > 0);
        self.entries.push_back((Arc::downgrade(tbl), key, p));
        while self.entries.len() > STRINGS_QUERIES {
            self.entries.pop_front();
        }
    }
}

/// The last install discovery.
#[derive(Debug, Clone)]
pub struct InstallsCache<T> {
    pub at: Instant,
    /// config.toml and outputs.json.
    pub stamp: Stamp,
    /// Folders picked this session when it was made.
    pub manual: Vec<PathBuf>,
    pub list: Vec<T>,
}

impl<T: Clone> InstallsCache<T> {
    /// The list, while it is young enough and nothing it was built from changed.
    pub fn fresh(&self, now: Instant, stamp: &Stamp, manual: &[PathBuf]) -> Option<Vec<T>> {
        (now.duration_since(self.at) < INSTALLS_TTL
            && self.stamp.same(stamp)
            && self.manual == manual)
            .then(|| self.list.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "starorg-caches-{name}-{}-{}",
            std::process::id(),
            fsutil_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fsutil_nanos() -> u128 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn set_mtime(p: &Path, t: SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn a_stamp_sees_edits_new_and_removed_files() {
        let d = tmp("edits");
        let old = SystemTime::now() - Duration::from_secs(60);
        std::fs::write(d.join("a.toml"), "x").unwrap();
        set_mtime(&d.join("a.toml"), old);
        let s1 = Stamp::new().dir(&d).file(d.join("missing.json"));
        assert!(s1.settled());
        assert!(s1.same(&Stamp::new().dir(&d).file(d.join("missing.json"))));
        // Same length, newer time.
        std::fs::write(d.join("a.toml"), "y").unwrap();
        let s2 = Stamp::new().dir(&d).file(d.join("missing.json"));
        assert!(!s1.same(&s2));
        // Just written: not settled, so not trusted.
        assert!(!s2.settled());
        std::fs::write(d.join("b.toml"), "y").unwrap();
        assert!(!s2.same(&Stamp::new().dir(&d).file(d.join("missing.json"))));
        std::fs::remove_file(d.join("b.toml")).unwrap();
        std::fs::write(d.join("missing.json"), "{}").unwrap();
        assert!(!s2.same(&Stamp::new().dir(&d).file(d.join("missing.json"))));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn installs_cache_expires_and_follows_its_inputs() {
        let d = tmp("installs");
        let stamp = Stamp::new().file(d.join("config.toml"));
        let c = InstallsCache {
            at: Instant::now(),
            stamp: stamp.clone(),
            manual: vec![],
            list: vec![1u8],
        };
        assert_eq!(c.fresh(Instant::now(), &stamp, &[]), Some(vec![1]));
        assert_eq!(
            c.fresh(Instant::now(), &stamp, std::slice::from_ref(&d)),
            None
        );
        assert_eq!(c.fresh(c.at + INSTALLS_TTL, &stamp, &[]), None);
        std::fs::write(d.join("config.toml"), "x").unwrap();
        assert_eq!(
            c.fresh(
                Instant::now(),
                &Stamp::new().file(d.join("config.toml")),
                &[]
            ),
            None
        );
        std::fs::remove_dir_all(&d).unwrap();
    }
}
