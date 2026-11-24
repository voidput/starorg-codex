//! OS advisory locks (spec 7.1 step 1, 11).
//!
//! `flock` on Linux, `LockFileEx` on Windows, through `std::fs::File::try_lock`. The lock is
//! held by the open file handle and the system releases it when the process dies, so a
//! leftover lock file never blocks anything. Locks are never "a file exists".

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// A held lock. Dropping it releases the lock (the file itself stays, harmlessly).
#[derive(Debug)]
pub struct Lock {
    _file: File,
    path: PathBuf,
}

impl Lock {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Another process holds it (CLI exit 3).
    #[error("{} is held by another process", .0.display())]
    Held(PathBuf),
    #[error("cannot open lock file {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Try to take an exclusive lock without waiting.
pub fn try_lock(path: &Path) -> Result<Lock, LockError> {
    take(path, false)
}

/// Try to take a shared lock without waiting: any number of shared holders at once, none
/// while someone holds it exclusively ([`try_lock`]).
pub fn try_lock_shared(path: &Path) -> Result<Lock, LockError> {
    take(path, true)
}

fn take(path: &Path, shared: bool) -> Result<Lock, LockError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| LockError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| LockError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let got = if shared {
        file.try_lock_shared()
    } else {
        file.try_lock()
    };
    match got {
        Ok(()) => Ok(Lock {
            _file: file,
            path: path.to_path_buf(),
        }),
        Err(TryLockError::WouldBlock) => Err(LockError::Held(path.to_path_buf())),
        Err(TryLockError::Error(source)) => Err(LockError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lock_is_refused_until_the_first_is_dropped() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("locks/x.lock");
        let a = try_lock(&p).unwrap();
        assert!(matches!(try_lock(&p), Err(LockError::Held(_))));
        drop(a);
        let _b = eventually(&p);
        assert!(p.exists(), "the file stays; only the lock is gone");
    }

    /// The exclusive lock once nobody holds the file. Another test of this process starting
    /// a child can hold a copy of an open lock file for a moment (between fork and exec, the
    /// child shares it), so a lock just dropped may still read as held for an instant.
    fn eventually(p: &Path) -> Lock {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match try_lock(p) {
                Ok(l) => return l,
                Err(LockError::Held(_)) if std::time::Instant::now() < until => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => panic!("{e}"),
            }
        }
    }

    #[test]
    fn shared_holders_coexist_and_keep_the_exclusive_one_out() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x.lock");
        let r1 = try_lock_shared(&p).unwrap();
        let r2 = try_lock_shared(&p).unwrap();
        assert!(matches!(try_lock(&p), Err(LockError::Held(_))));
        drop(r1);
        assert!(
            matches!(try_lock(&p), Err(LockError::Held(_))),
            "one reader left"
        );
        drop(r2);
        let x = eventually(&p);
        assert!(matches!(try_lock_shared(&p), Err(LockError::Held(_))));
        drop(x);
    }
}
