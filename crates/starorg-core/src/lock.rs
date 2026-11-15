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
    match file.try_lock() {
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
        let _b = try_lock(&p).unwrap();
        assert!(p.exists(), "the file stays; only the lock is gone");
    }
}
