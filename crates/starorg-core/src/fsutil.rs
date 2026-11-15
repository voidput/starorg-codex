//! Small file helpers shared by apply, backups and state files (spec 11, "File safety").
//!
//! - All writes are atomic: temp file beside the real target, fsync, rename, folder fsync.
//! - Hashes are SHA-256, lower-case hex.
//! - Times are UTC.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Hash several labelled parts into one id; order matters.
pub fn sha256_parts(parts: &[&[u8]]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

/// fsync a directory so a rename inside it is durable (Linux). A no-op elsewhere.
pub fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
    Ok(())
}

/// `rename`, retrying briefly on Windows when a scanner or indexer holds a just-written
/// file open (ERROR_ACCESS_DENIED / sharing violation). Elsewhere a plain rename.
pub fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let mut last = None;
        for i in 0..20u64 {
            match std::fs::rename(from, to) {
                Ok(()) => return Ok(()),
                Err(e)
                    if e.kind() == std::io::ErrorKind::PermissionDenied
                        || e.raw_os_error() == Some(32) =>
                {
                    last = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(25 * (i + 1)));
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
}

/// Write `bytes` to `tmp`, fsync it, rename it over `target`, fsync the folder. `tmp` must be
/// in the same folder as `target`. On error the temp file is removed.
pub fn write_via(tmp: &Path, target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let result = (|| {
        let mut f = std::fs::File::create(tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if let Ok(meta) = std::fs::metadata(target) {
            // Keep the permissions of the file we replace.
            let _ = std::fs::set_permissions(tmp, meta.permissions());
        }
        rename_retry(tmp, target)?;
        if let Some(dir) = target.parent().filter(|d| !d.as_os_str().is_empty()) {
            fsync_dir(dir)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result
}

/// Atomic write for our own files (app directories): temp `.<name>.tmp-<pid>` beside the
/// target. Creates the parent folder.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    if let Some(d) = dir {
        std::fs::create_dir_all(d)?;
    }
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("path has no file name"))?;
    let tmp = path.with_file_name(format!(
        ".{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    write_via(&tmp, path, bytes)
}

/// Write JSON atomically.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    write_atomic(path, &bytes)
}

/// Read JSON; `Ok(None)` when the file does not exist.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> std::io::Result<Option<T>> {
    match std::fs::read(path) {
        Ok(b) => serde_json::from_slice(&b)
            .map(Some)
            .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Read a file but refuse one larger than `limit` (spec 11: oversized input is rejected, not
/// truncated). `Ok(None)` when it does not exist.
pub fn read_limited(path: &Path, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let len = f.metadata()?.len();
    if len > limit {
        return Err(std::io::Error::other(format!(
            "{} is {len} bytes, more than the {limit} byte limit",
            path.display()
        )));
    }
    let mut buf = Vec::with_capacity(len as usize);
    f.take(limit + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(std::io::Error::other(format!(
            "{} is larger than the {limit} byte limit",
            path.display()
        )));
    }
    Ok(Some(buf))
}

/// Canonical form of a path that may not exist yet: canonicalise the deepest existing
/// ancestor and append the rest.
pub fn canonical_lenient(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut existing = abs.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(c) = std::fs::canonicalize(existing) {
            let mut out = c;
            for r in rest.iter().rev() {
                out.push(r);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return abs,
        }
    }
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Civil UTC date-time from Unix seconds: (year, month, day, hour, minute, second).
pub fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

/// `YYYYmmdd-HHMMSS` (UTC), used for backup ids.
pub fn stamp(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// RFC 3339 UTC, for manifests.
pub fn rfc3339(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Effective user id of this process (Linux: from `/proc/self/status`, no unsafe code).
#[cfg(unix)]
pub fn current_uid() -> Option<u32> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("Uid:"))?;
    // Uid: real effective saved fs
    line.split_whitespace().nth(2)?.parse().ok()
}

/// Is this process running as root (Linux) or elevated (Windows)? Refuse to apply if so.
pub fn is_elevated() -> bool {
    #[cfg(unix)]
    {
        current_uid() == Some(0)
    }
    #[cfg(windows)]
    {
        // `net session` succeeds only in an elevated process. No unsafe code, no extra crate.
        std::process::Command::new("net")
            .arg("session")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Is the file at `path` (already resolved) a regular file owned by the current user?
pub fn owned_regular_file(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.file_type().is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Some(uid) = current_uid()
            && meta.uid() != uid
        {
            return Err(format!(
                "{} is owned by uid {}, not by the current user (uid {uid})",
                path.display(),
                meta.uid()
            ));
        }
    }
    Ok(())
}

/// Random hex id (install ids).
pub fn random_id(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if getrandom::fill(&mut buf).is_err() {
        // Fall back to time and pid; uniqueness, not secrecy, is what matters here.
        let seed = format!(
            "{:?}-{}",
            SystemTime::now().duration_since(UNIX_EPOCH),
            std::process::id()
        );
        return sha256_hex(seed.as_bytes())[..bytes * 2].to_string();
    }
    hex(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).unwrap(), sha256_hex(b"abc"));
    }

    #[test]
    fn civil_dates() {
        assert_eq!(stamp(0), "19700101-000000");
        // 2026-11-14 21:30:05 UTC
        assert_eq!(stamp(1_794_691_805), "20261114-213005");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn atomic_write_leaves_no_temp_and_read_limited() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("sub/x.json");
        write_json(&p, &serde_json::json!({"a": 1})).unwrap();
        let v: serde_json::Value = read_json(&p).unwrap().unwrap();
        assert_eq!(v["a"], 1);
        let names: Vec<_> = std::fs::read_dir(t.path().join("sub"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec!["x.json"]);
        assert!(read_limited(&p, 3).is_err(), "oversized input is rejected");
        assert!(read_limited(&t.path().join("none"), 3).unwrap().is_none());
    }

    #[test]
    fn random_ids_differ() {
        let a = random_id(8);
        assert_eq!(a.len(), 16);
        assert_ne!(a, random_id(8));
    }
}
