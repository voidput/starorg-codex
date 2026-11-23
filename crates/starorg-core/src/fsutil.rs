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

/// Set a file's modification time to now (best effort: "last used", for the storage cap).
pub fn touch(file: &Path) {
    if let Ok(f) = std::fs::File::options().write(true).open(file) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
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

/// Run a file operation, retrying briefly on Windows while a virus scanner, the search
/// indexer or a sync client holds the file open: sharing and lock violations (32, 33) get 20
/// tries over about 5 s, access denied (5) only 6 tries over about 0.4 s, because it is also
/// what a read-only library returns and the caller should show its hint quickly (see
/// [`lock_retry_budget`]). Elsewhere it runs once. Other errors return at once.
pub fn retry_locked<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    #[cfg(windows)]
    {
        let mut i = 0u64;
        loop {
            match op() {
                Ok(v) => return Ok(v),
                Err(e) => match lock_retry_budget(&e) {
                    Some(budget) if i + 1 < budget => {
                        i += 1;
                        std::thread::sleep(std::time::Duration::from_millis(25 * i));
                    }
                    _ => return Err(e),
                },
            }
        }
    }
    #[cfg(not(windows))]
    {
        op()
    }
}

/// Windows error codes that may mean "someone else has the file open right now".
pub fn is_transient_lock(e: &std::io::Error) -> bool {
    lock_retry_budget(e).is_some()
}

/// How many tries [`retry_locked`] gives an error: 20 for `ERROR_SHARING_VIOLATION` (32) and
/// `ERROR_LOCK_VIOLATION` (33), which only ever mean another process has the file open; 6 for
/// access denied (5, or any `PermissionDenied`), which a scanner holding a file causes too but
/// so does a read-only folder; `None` for everything else (no retry).
pub fn lock_retry_budget(e: &std::io::Error) -> Option<u64> {
    match e.raw_os_error() {
        Some(32 | 33) => Some(20),
        Some(5) => Some(6),
        _ if e.kind() == std::io::ErrorKind::PermissionDenied => Some(6),
        _ => None,
    }
}

/// `rename` with [`retry_locked`].
pub fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_locked(|| std::fs::rename(from, to))
}

/// `remove_file` with [`retry_locked`].
pub fn remove_retry(path: &Path) -> std::io::Result<()> {
    retry_locked(|| std::fs::remove_file(path))
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

/// `std::fs::canonicalize`, but on Windows without the `\\?\` prefix when the plain form
/// means the same path (see [`strip_verbatim`]). Every path we compare, store or show goes
/// through this one function, so two spellings of one folder never meet: a running game's
/// image path is `C:\...`, and std's canonical form is `\\?\C:\...`.
pub fn canonicalize(p: &Path) -> std::io::Result<PathBuf> {
    let c = std::fs::canonicalize(p)?;
    #[cfg(windows)]
    {
        if let Some(plain) = c.to_str().and_then(strip_verbatim) {
            return Ok(PathBuf::from(plain));
        }
    }
    Ok(c)
}

/// The plain form of a Windows verbatim path, when it is safe to use: `\\?\C:\x` is
/// `C:\x` and `\\?\UNC\srv\share\x` is `\\srv\share\x`. `None` (keep the verbatim
/// form) when the plain form would be read differently: 260 characters or more, a reserved
/// device name (`CON`, `NUL`, `COM1`, ...), a component ending in a dot or space, a `/`,
/// `.` or `..` components. Pure string code, so it is tested on every OS; std itself adds
/// the prefix again for long paths when it calls Windows.
pub fn strip_verbatim(s: &str) -> Option<String> {
    let plain = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        let rest = s.strip_prefix(r"\\?\")?;
        let b = rest.as_bytes();
        if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
            return None;
        }
        rest.to_string()
    };
    if plain.len() >= 260 || plain.contains('/') {
        return None;
    }
    let body = plain.trim_start_matches('\\');
    for (i, comp) in body.split('\\').enumerate() {
        if comp.is_empty() {
            // Only a trailing separator (`C:\`) may be empty.
            if i + 1 == body.split('\\').count() {
                continue;
            }
            return None;
        }
        if i == 0 && comp.ends_with(':') {
            continue;
        }
        if comp == "." || comp == ".." || comp.ends_with(['.', ' ']) || reserved_name(comp) {
            return None;
        }
    }
    Some(plain)
}

/// DOS device names, which Windows reads as devices in any folder and with any extension.
fn reserved_name(comp: &str) -> bool {
    let stem = comp.split('.').next().unwrap_or(comp).trim_end();
    let up = stem.to_ascii_uppercase();
    matches!(
        up.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((up.starts_with("COM") || up.starts_with("LPT"))
        && up.len() == 4
        && matches!(up.as_bytes()[3], b'1'..=b'9'))
}

/// Is the Windows path `child` inside `dir` (or equal to it)? Components compare without
/// case, `\` and `/` both separate, and a `\\?\` prefix on either side is ignored. Used for
/// process image paths, which Windows reports in plain form. Pure string code (tested on every
/// OS).
pub fn windows_path_within(child: &str, dir: &str) -> bool {
    fn comps(s: &str) -> Vec<String> {
        // A verbatim path strip_verbatim refuses (260+ characters, ...) still names the same
        // file as its plain form for this comparison: `\\?\UNC\srv\share` is `\\srv\share`.
        let s = strip_verbatim(s).unwrap_or_else(|| {
            s.strip_prefix(r"\\?\UNC\")
                .or_else(|| s.strip_prefix(r"\\?\"))
                .map(str::to_string)
                .unwrap_or_else(|| s.to_string())
        });
        s.split(['\\', '/'])
            .filter(|c| !c.is_empty() && *c != ".")
            .map(str::to_lowercase)
            .collect()
    }
    let (c, d) = (comps(child), comps(dir));
    !d.is_empty() && c.len() >= d.len() && c[..d.len()] == d[..]
}

/// Canonical form of a path that may not exist yet: canonicalise the deepest existing
/// ancestor and append the rest.
pub fn canonical_lenient(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut existing = abs.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(c) = canonicalize(existing) {
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

/// Is this process running as root (Linux) or elevated (Windows)? Apply refuses if so
/// (through [`elevation_blocks_apply`]); diagnostics show the plain answer.
///
/// Windows: the process token's integrity level, read from `whoami /groups` (the SID
/// `S-1-16-12288` "High Mandatory Level" or `S-1-16-16384` "System" is present only in an
/// elevated or SYSTEM process; SIDs are not translated, so this works in every language).
/// Asked once per process: elevation cannot change while a process runs. If `whoami` cannot
/// run, `net session` decides (it succeeds only elevated, but needs the Server service).
pub fn is_elevated() -> bool {
    #[cfg(unix)]
    {
        current_uid() == Some(0)
    }
    #[cfg(windows)]
    {
        static ELEVATED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ELEVATED.get_or_init(windows_elevated)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(windows)]
fn windows_elevated() -> bool {
    let system32 = system32();
    let mut whoami = std::process::Command::new(system32.join("whoami.exe"));
    whoami.args(["/groups", "/fo", "csv", "/nh"]);
    if let Ok(o) = no_console_window(&mut whoami)
        .stderr(std::process::Stdio::null())
        .output()
        && o.status.success()
    {
        return groups_show_elevated(&String::from_utf8_lossy(&o.stdout));
    }
    let mut net = std::process::Command::new(system32.join("net.exe"));
    net.arg("session");
    no_console_window(&mut net)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `%SystemRoot%\System32`: system tools are started by full path, never through `PATH`.
#[cfg(windows)]
pub fn system32() -> PathBuf {
    std::env::var_os("SystemRoot")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
}

/// Does `whoami /groups` output list an elevated (High) or System integrity level?
pub fn groups_show_elevated(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.to_ascii_uppercase();
        l.contains("S-1-16-12288") || l.contains("S-1-16-16384")
    })
}

/// Windows: start a console program without flashing a console window over the GUI
/// (`CREATE_NO_WINDOW`). Elsewhere a no-op.
pub fn no_console_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(feature = "test-hooks")]
static TEST_ALLOW_ELEVATED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Test hook (feature `test-hooks` only, never in a release binary): let apply, remove and
/// restore run in an elevated process. GitHub's Windows runners are administrators with UAC
/// off, so without this the apply-flow tests could not run there. Child processes get the
/// same with `STARORG_TEST_ALLOW_ELEVATED=1`.
#[cfg(feature = "test-hooks")]
pub fn test_allow_elevated(on: bool) {
    TEST_ALLOW_ELEVATED.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// The apply guard: refuse when root or elevated (spec 11). Without the `test-hooks` feature
/// this is exactly [`is_elevated`].
pub fn elevation_blocks_apply() -> bool {
    #[cfg(feature = "test-hooks")]
    {
        if TEST_ALLOW_ELEVATED.load(std::sync::atomic::Ordering::SeqCst)
            || std::env::var_os("STARORG_TEST_ALLOW_ELEVATED").is_some_and(|v| v == "1")
        {
            return false;
        }
    }
    is_elevated()
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
    fn verbatim_prefix_is_dropped_only_when_safe() {
        let ok = |a: &str, b: &str| assert_eq!(strip_verbatim(a).as_deref(), Some(b), "{a}");
        ok(
            r"\\?\C:\Program Files\Roberts Space Industries\StarCitizen",
            r"C:\Program Files\Roberts Space Industries\StarCitizen",
        );
        ok(r"\\?\D:\", r"D:\");
        ok(r"\\?\UNC\nas\games\StarCitizen", r"\\nas\games\StarCitizen");
        for keep in [
            r"C:\plain",
            r"\\?\Volume{0d7e}\x",
            r"\\?\C:\Games\NUL",
            r"\\?\C:\Games\com1.log",
            r"\\?\C:\Games\trailing.",
            r"\\?\C:\Games\trailing \x",
            r"\\?\C:\a/b",
            r"\\?\C:\a\..\b",
        ] {
            assert_eq!(strip_verbatim(keep), None, "{keep}");
        }
        let long = format!(r"\\?\C:\{}", "x".repeat(300));
        assert_eq!(strip_verbatim(&long), None, "long paths keep the prefix");
    }

    #[test]
    fn windows_containment_ignores_case_separators_and_prefix() {
        let dir = r"\\?\C:\Program Files\Roberts Space Industries\StarCitizen\LIVE";
        assert!(windows_path_within(
            r"c:\program files\roberts space industries\starcitizen\live\Bin64\StarCitizen.exe",
            dir
        ));
        assert!(windows_path_within(
            r"C:/Program Files/Roberts Space Industries/StarCitizen/LIVE",
            dir
        ));
        assert!(!windows_path_within(
            r"C:\Program Files\Roberts Space Industries\StarCitizen\LIVE2\Bin64\StarCitizen.exe",
            dir
        ));
        assert!(!windows_path_within(
            r"D:\Program Files\Roberts Space Industries\StarCitizen\LIVE\Bin64\StarCitizen.exe",
            dir
        ));
        assert!(!windows_path_within(r"C:\x", ""));
    }

    #[test]
    fn windows_containment_long_verbatim_unc_matches_plain_image_path() {
        // 260+ characters: strip_verbatim keeps the prefix, the fallback must still drop `UNC`.
        let deep = "d".repeat(270);
        let dir = format!(r"\\?\UNC\nas\games\{deep}\StarCitizen\LIVE");
        assert_eq!(strip_verbatim(&dir), None);
        let exe = format!(r"\\nas\games\{deep}\StarCitizen\LIVE\Bin64\StarCitizen.exe");
        assert!(windows_path_within(&exe, &dir));
        let long_drive = format!(r"\\?\C:\{deep}\LIVE");
        assert!(windows_path_within(
            &format!(r"C:\{deep}\LIVE\Bin64\x.exe"),
            &long_drive
        ));
        assert!(!windows_path_within(
            &format!(r"\\other\games\{deep}\StarCitizen\LIVE\x.exe"),
            &dir
        ));
    }

    #[test]
    fn lock_retry_budget_keeps_access_denied_short() {
        use std::io::{Error, ErrorKind};
        assert_eq!(lock_retry_budget(&Error::from_raw_os_error(32)), Some(20));
        assert_eq!(lock_retry_budget(&Error::from_raw_os_error(33)), Some(20));
        assert_eq!(lock_retry_budget(&Error::from_raw_os_error(5)), Some(6));
        assert_eq!(
            lock_retry_budget(&Error::new(ErrorKind::PermissionDenied, "read-only")),
            Some(6)
        );
        assert_eq!(
            lock_retry_budget(&Error::new(ErrorKind::NotFound, "x")),
            None
        );
        assert!(!is_transient_lock(&Error::new(ErrorKind::NotFound, "x")));
        assert!(is_transient_lock(&Error::from_raw_os_error(32)));
    }

    #[test]
    fn whoami_integrity_levels() {
        let medium = "\"Everyone\",\"Well-known group\",\"S-1-1-0\",\"Mandatory group\"\r\n\"Mandatory Label\\Medium Mandatory Level\",\"Label\",\"S-1-16-8192\",\"\"\r\n";
        assert!(!groups_show_elevated(medium));
        // Localised group names, same SID.
        let high = "\"Etiqueta obligatoria\\Nivel obligatorio alto\",\"Etiqueta\",\"S-1-16-12288\",\"\"\r\n";
        assert!(groups_show_elevated(high));
        assert!(groups_show_elevated(
            "\"x\",\"Label\",\"s-1-16-16384\",\"\""
        ));
        assert!(!groups_show_elevated(""));
    }

    #[test]
    fn canonicalize_is_plain_and_stable() {
        let t = tempfile::tempdir().unwrap();
        let c = canonicalize(t.path()).unwrap();
        assert!(!c.to_string_lossy().starts_with(r"\\?\"), "{}", c.display());
        assert_eq!(canonicalize(&c).unwrap(), c);
        assert_eq!(canonical_lenient(&c.join("a/b")), c.join("a/b"));
    }

    #[test]
    fn random_ids_differ() {
        let a = random_id(8);
        assert_eq!(a.len(), 16);
        assert_ne!(a, random_id(8));
    }
}
