//! Case-insensitive path resolution (spec 10.3).
//!
//! A Wine prefix lives on a case-sensitive filesystem that Wine reads case-insensitively. Our
//! machine has `Data/` and `USER.cfg`; writing `data/` or `user.cfg` would create a second
//! entry, and which one Wine uses is undefined. So every path component under a channel is
//! matched against what is on disk.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("two entries differ only in case: {} and {}", .first.display(), .second.display())]
    Duplicate { first: PathBuf, second: PathBuf },
    #[error("cannot list {}: {source}", .dir.display())]
    Io {
        dir: PathBuf,
        source: std::io::Error,
    },
}

/// Result of [`resolve_ci`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The path with on-disk spelling for components that exist and the given spelling for
    /// the rest.
    pub path: PathBuf,
    /// Number of leading components that exist on disk.
    pub existing: usize,
    /// Number of components asked for.
    pub total: usize,
}

impl Resolved {
    pub fn exists(&self) -> bool {
        self.existing == self.total
    }
}

/// Resolve `components` under `base`, matching each one case-insensitively against the
/// directory listing.
///
/// - One match: use the on-disk spelling.
/// - Two or more matches: error naming two of them.
/// - No match: the component and all later ones keep the given spelling (callers pass the
///   spelling the game uses on Windows: `Data`, `Localization`, `english`, `global.ini`,
///   `user.cfg`).
///
/// On Windows the filesystem is already case-insensitive and this reduces to the plain path.
pub fn resolve_ci(base: &Path, components: &[&str]) -> Result<Resolved, ResolveError> {
    let mut path = base.to_path_buf();
    let mut existing = 0;
    let mut missing = false;
    for comp in components {
        if missing {
            path.push(comp);
            continue;
        }
        match find_ci(&path, comp)? {
            Some(name) => {
                path.push(name);
                existing += 1;
            }
            None => {
                missing = true;
                path.push(comp);
            }
        }
    }
    Ok(Resolved {
        path,
        existing,
        total: components.len(),
    })
}

/// Like [`resolve_ci`] but `None` when the full path does not exist.
pub fn find_existing_ci(base: &Path, components: &[&str]) -> Result<Option<PathBuf>, ResolveError> {
    let r = resolve_ci(base, components)?;
    Ok(r.exists().then_some(r.path))
}

fn find_ci(dir: &Path, name: &str) -> Result<Option<OsString>, ResolveError> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => return Ok(None),
        Err(source) => {
            return Err(ResolveError::Io {
                dir: dir.to_path_buf(),
                source,
            });
        }
    };
    let want = name.to_lowercase();
    let mut found: Option<OsString> = None;
    for entry in rd {
        let entry = entry.map_err(|source| ResolveError::Io {
            dir: dir.to_path_buf(),
            source,
        })?;
        let os = entry.file_name();
        let Some(s) = os.to_str() else { continue };
        if s.to_lowercase() == want {
            if let Some(first) = found {
                // Report in a stable order.
                let (a, b) = if first <= os {
                    (first, os)
                } else {
                    (os, first)
                };
                return Err(ResolveError::Duplicate {
                    first: dir.join(a),
                    second: dir.join(b),
                });
            }
            found = Some(os);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn picks_on_disk_spelling() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("Data/Localization/english")).unwrap();
        fs::write(t.path().join("USER.cfg"), "").unwrap();
        let r = resolve_ci(t.path(), &["data", "localization", "ENGLISH"]).unwrap();
        assert_eq!(r.path, t.path().join("Data/Localization/english"));
        assert!(r.exists());
        let r = resolve_ci(t.path(), &["user.cfg"]).unwrap();
        assert_eq!(r.path, t.path().join("USER.cfg"));
    }

    #[test]
    fn missing_components_keep_given_spelling() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("Data")).unwrap();
        let r = resolve_ci(t.path(), &["data", "Localization", "english", "global.ini"]).unwrap();
        assert_eq!(
            r.path,
            t.path().join("Data/Localization/english/global.ini")
        );
        assert_eq!((r.existing, r.total), (1, 4));
        assert!(!r.exists());
        assert_eq!(find_existing_ci(t.path(), &["data", "x"]).unwrap(), None);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    #[test]
    fn duplicate_case_entries_are_an_error() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("USER.cfg"), "").unwrap();
        fs::write(t.path().join("user.cfg"), "").unwrap();
        let err = resolve_ci(t.path(), &["User.Cfg"]).unwrap_err();
        match err {
            ResolveError::Duplicate { first, second } => {
                assert_eq!(first, t.path().join("USER.cfg"));
                assert_eq!(second, t.path().join("user.cfg"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn file_in_the_middle_counts_as_missing() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("Data"), "").unwrap();
        let r = resolve_ci(t.path(), &["Data", "x"]).unwrap();
        assert_eq!(r.existing, 1);
        assert!(!r.exists());
    }
}
