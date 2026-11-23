//! Run-time schema: the telemetry `FileDescriptorProto`s embedded in the user's own
//! `StarCitizen.exe`, cached per build (port of sc-telemetry `sctelemetry/schema.py`).
//!
//! The exe carries its protobuf descriptors as serialized `FileDescriptorProto`s. Each one
//! starts with field 1 (`name`): byte `0x0A`, a length varint, then the file name. The scan
//! looks for the [`PREFIX`] of the telemetry files, steps back over the tag and length, walks
//! the wire-format fields a `FileDescriptorProto` may hold, and decodes what it walked. For a
//! name seen more than once the longest decode wins. [`MARKER`](crate::MARKER) and the files
//! it imports (transitively) become a [`DescriptorPool`]; `google/protobuf/*` imports come
//! from prost-reflect's built-in well-known types.
//!
//! The exe is a few hundred MB, so it is read in windows of [`CHUNK`] bytes with
//! [`MAX_DESCRIPTOR`] bytes of overlap, never whole. A descriptor longer than the overlap that
//! straddles a window is cut short and decodes as a partial file; the telemetry files are
//! tens of KB.
//!
//! The cache holds only the extracted `FileDescriptorSet` (no `.proto` text), one file per
//! [`CacheKey`] in a folder the caller names (the app's cache folder, under its storage cap).
//! [`KEEP_KEYS`] newest keys are kept; older ones are removed when a new one is written.

use crate::{MARKER, SERVICE};
use memchr::memmem;
use prost_reflect::prost::Message;
use prost_reflect::prost_types::{FileDescriptorProto, FileDescriptorSet};
use prost_reflect::{DescriptorPool, MethodDescriptor, ServiceDescriptor};
use sha2::{Digest, Sha256};
use starorg_core::build::BuildInfo;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Name prefix of the files the scan keeps.
pub const PREFIX: &str = "sc/game/telemetry/";

/// Bytes read per window.
pub const CHUNK: usize = 8 << 20;

/// Overlap between windows: the longest descriptor found whole across a window edge.
pub const MAX_DESCRIPTOR: usize = 2 << 20;

/// Cached schema files kept in the cache folder (one per build key).
pub const KEEP_KEYS: usize = 3;

/// Extension of a cached `FileDescriptorSet`.
const CACHE_EXT: &str = "fds";

/// Fields a `FileDescriptorProto` may carry, with their wire type (schema.py `FDP_FIELDS`):
/// 1 name, 2 package, 3 dependency, 4 message_type, 5 enum_type, 6 service, 7 extension,
/// 8 options, 9 source_code_info, 10 public_dependency, 11 weak_dependency, 12 syntax,
/// 14 edition, 15 (reserved by newer protoc). Wire type 0 is a varint, 2 length-delimited.
const FDP_FIELDS: [(u32, u8); 14] = [
    (1, 2),
    (2, 2),
    (3, 2),
    (4, 2),
    (5, 2),
    (6, 2),
    (7, 2),
    (8, 2),
    (9, 2),
    (10, 0),
    (11, 0),
    (12, 2),
    (14, 0),
    (15, 2),
];

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "no telemetry schema in this StarCitizen.exe (the build may not include GameTelemetryService)"
    )]
    NotFound,
    #[error("the telemetry schema imports {0}, which is not in the exe")]
    MissingImport(String),
    #[error("the telemetry schema is invalid: {0}")]
    Invalid(String),
    #[error("the telemetry schema has no {SERVICE}")]
    NoService,
}

/// Where a [`Schema`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaOrigin {
    /// Read from the cache folder; the exe was not scanned.
    Cache,
    /// Scanned from the exe just now (and written to the cache).
    Extracted,
    /// Built in code ([`crate::mock`]).
    Synthetic,
}

/// Cache key of one build's schema. Only `[A-Za-z0-9._-]`, so it is a safe file name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(String);

impl CacheKey {
    /// `<Branch>-<RequestedP4ChangeNum>` from the channel's `build_manifest.id`, as in
    /// schema.py. `None` without a change number.
    pub fn for_build(build: &BuildInfo) -> Option<Self> {
        let change = build.change?;
        Some(Self::sanitized(&format!("{}-{change}", build.branch)))
    }

    /// `exe-<hash of path, size and mtime>` for an exe without a manifest (schema.py
    /// `_exe_key`).
    pub fn for_exe(exe: &Path) -> io::Result<Self> {
        let meta = fs::metadata(exe)?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        let mut h = Sha256::new();
        h.update(exe.to_string_lossy().as_bytes());
        h.update(format!("|{}|{mtime}", meta.len()).as_bytes());
        let hex: String = h.finalize()[..6]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self(format!("exe-{hex}")))
    }

    /// A key from any text: characters outside `[A-Za-z0-9._-]` become `_`, at most 96 long,
    /// never empty or starting with a dot.
    pub fn sanitized(s: &str) -> Self {
        let mut k: String = s
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .take(96)
            .collect();
        if k.is_empty() || k.starts_with('.') {
            k.insert(0, '_');
        }
        Self(k)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The telemetry schema of one build: a descriptor pool holding [`SERVICE`].
#[derive(Debug, Clone)]
pub struct Schema {
    pool: DescriptorPool,
    service: ServiceDescriptor,
    files: FileDescriptorSet,
    key: String,
    origin: SchemaOrigin,
}

impl Schema {
    /// A schema from the telemetry files (in any order). `google/protobuf/*` imports may be
    /// left out; any other import must be in `files`.
    pub fn from_files(
        files: Vec<FileDescriptorProto>,
        key: &str,
        origin: SchemaOrigin,
    ) -> Result<Self, SchemaError> {
        let mut pool = DescriptorPool::global();
        pool.add_file_descriptor_protos(files.iter().cloned())
            .map_err(|e| SchemaError::Invalid(e.to_string()))?;
        let service = pool
            .get_service_by_name(SERVICE)
            .ok_or(SchemaError::NoService)?;
        Ok(Self {
            pool,
            service,
            files: FileDescriptorSet { file: files },
            key: key.to_owned(),
            origin,
        })
    }

    /// A schema from serialized `FileDescriptorSet` bytes (the cache format).
    pub fn from_set_bytes(
        bytes: &[u8],
        key: &str,
        origin: SchemaOrigin,
    ) -> Result<Self, SchemaError> {
        let set =
            FileDescriptorSet::decode(bytes).map_err(|e| SchemaError::Invalid(e.to_string()))?;
        if !set.file.iter().any(|f| f.name() == MARKER) {
            return Err(SchemaError::NotFound);
        }
        Self::from_files(set.file, key, origin)
    }

    pub fn pool(&self) -> &DescriptorPool {
        &self.pool
    }

    pub fn service(&self) -> &ServiceDescriptor {
        &self.service
    }

    /// A method of [`SERVICE`] by its short name, e.g. `TelemetryStream`.
    pub fn method(&self, name: &str) -> Option<MethodDescriptor> {
        self.service.methods().find(|m| m.name() == name)
    }

    /// The cache key it was stored under (`mock` for the synthetic schema).
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn origin(&self) -> SchemaOrigin {
        self.origin
    }

    /// Names of the telemetry files, sorted.
    pub fn file_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .files
            .file
            .iter()
            .map(|f| f.name().to_owned())
            .collect();
        v.sort();
        v
    }

    /// The telemetry files as a serialized `FileDescriptorSet` (what the cache stores).
    pub fn to_set_bytes(&self) -> Vec<u8> {
        self.files.encode_to_vec()
    }
}

fn varint(b: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut r = 0u64;
    let mut shift = 0u32;
    loop {
        let c = *b.get(i)?;
        i += 1;
        if shift > 63 {
            return None;
        }
        r |= u64::from(c & 0x7F) << shift;
        shift += 7;
        if c & 0x80 == 0 {
            return Some((r, i));
        }
    }
}

/// Walk wire-format fields valid for a `FileDescriptorProto` from `start`; the end of the
/// last whole field (schema.py `walk`).
fn walk(b: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < b.len() {
        let Some((tag, j)) = varint(b, i) else { break };
        let (num, wt) = (tag >> 3, (tag & 7) as u8);
        if !FDP_FIELDS
            .iter()
            .any(|&(n, w)| u64::from(n) == num && w == wt)
        {
            break;
        }
        let next = if wt == 0 {
            match varint(b, j) {
                Some((_, k)) => k,
                None => break,
            }
        } else {
            let Some((len, k)) = varint(b, j) else { break };
            match usize::try_from(len).ok().and_then(|l| k.checked_add(l)) {
                Some(end) if end <= b.len() => end,
                _ => break,
            }
        };
        i = next;
    }
    i
}

/// Try a descriptor whose name starts at `i` (the prefix match).
fn try_at(b: &[u8], i: usize, seen: &mut BTreeMap<String, (usize, FileDescriptorProto)>) {
    for back in [2usize, 3] {
        let Some(s) = i.checked_sub(back) else {
            continue;
        };
        if b[s] != 0x0A || varint(b, s + 1).map(|(_, j)| j) != Some(i) {
            continue;
        }
        let end = walk(b, s);
        let Ok(fd) = FileDescriptorProto::decode(&b[s..end]) else {
            continue;
        };
        let useful =
            !fd.message_type.is_empty() || !fd.service.is_empty() || !fd.enum_type.is_empty();
        if !fd.name().starts_with(PREFIX) || !useful {
            continue;
        }
        let len = end - s;
        if seen.get(fd.name()).is_none_or(|(l, _)| len > *l) {
            seen.insert(fd.name().to_owned(), (len, fd));
        }
    }
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Every telemetry `FileDescriptorProto` ([`PREFIX`]) found in `r`, by name.
pub fn scan(r: impl Read) -> io::Result<BTreeMap<String, FileDescriptorProto>> {
    scan_windowed(r, CHUNK, MAX_DESCRIPTOR)
}

/// [`scan`] with explicit window sizes (tests use small ones).
pub fn scan_windowed(
    mut r: impl Read,
    chunk: usize,
    overlap: usize,
) -> io::Result<BTreeMap<String, FileDescriptorProto>> {
    let finder = memmem::Finder::new(PREFIX.as_bytes());
    let chunk = chunk.max(1);
    let mut seen = BTreeMap::new();
    let mut buf: Vec<u8> = Vec::with_capacity(chunk + overlap);
    // Matches before `from` were tried in an earlier window.
    let mut from = 0usize;
    loop {
        let old = buf.len();
        buf.resize(old + chunk, 0);
        let n = read_full(&mut r, &mut buf[old..])?;
        buf.truncate(old + n);
        let eof = n < chunk;
        // A match at or past `limit` waits for the next window, which keeps `overlap` bytes
        // after it for the descriptor body.
        let limit = if eof {
            buf.len()
        } else {
            buf.len().saturating_sub(overlap)
        };
        let mut pos = from;
        while pos < limit {
            let Some(off) = finder.find(&buf[pos..]) else {
                break;
            };
            let i = pos + off;
            if i >= limit {
                break;
            }
            try_at(&buf, i, &mut seen);
            pos = i + 1;
        }
        if eof {
            break;
        }
        // Keep the 3 bytes before `limit` too: the tag and length of a name at `limit`.
        let keep = limit.max(from).saturating_sub(3);
        buf.drain(..keep);
        from = limit.max(from) - keep;
    }
    Ok(seen.into_iter().map(|(k, (_, fd))| (k, fd)).collect())
}

/// [`MARKER`] and the telemetry files it imports, transitively. `google/protobuf/*` imports
/// are left to the well-known types.
fn closure(
    seen: &BTreeMap<String, FileDescriptorProto>,
) -> Result<Vec<FileDescriptorProto>, SchemaError> {
    if !seen.contains_key(MARKER) {
        return Err(SchemaError::NotFound);
    }
    let mut want = vec![MARKER.to_owned()];
    let mut done = BTreeSet::new();
    let mut out = Vec::new();
    while let Some(name) = want.pop() {
        if !done.insert(name.clone()) {
            continue;
        }
        if name.starts_with("google/protobuf/") {
            continue;
        }
        let fd = seen
            .get(&name)
            .ok_or_else(|| SchemaError::MissingImport(name.clone()))?;
        want.extend(fd.dependency.iter().cloned());
        out.push(fd.clone());
    }
    Ok(out)
}

/// Scan an exe (or any reader) and build the schema.
pub fn extract(r: impl Read, key: &str) -> Result<Schema, SchemaError> {
    let seen = scan(r).map_err(|source| SchemaError::Io {
        path: PathBuf::from("StarCitizen.exe"),
        source,
    })?;
    Schema::from_files(closure(&seen)?, key, SchemaOrigin::Extracted)
}

fn cache_file(cache_dir: &Path, key: &CacheKey) -> PathBuf {
    cache_dir.join(format!("{key}.{CACHE_EXT}"))
}

/// The cached schema for `key`, if there is a valid one.
pub fn load_cached(cache_dir: &Path, key: &CacheKey) -> Option<Schema> {
    let bytes = fs::read(cache_file(cache_dir, key)).ok()?;
    Schema::from_set_bytes(&bytes, key.as_str(), SchemaOrigin::Cache).ok()
}

/// The newest valid cached schema of any key: for a start without the game installed.
pub fn load_newest_cached(cache_dir: &Path) -> Option<Schema> {
    let mut entries = cached_entries(cache_dir);
    entries.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    entries.into_iter().find_map(|(p, _)| {
        let key = p.file_stem()?.to_str()?.to_owned();
        let bytes = fs::read(&p).ok()?;
        Schema::from_set_bytes(&bytes, &key, SchemaOrigin::Cache).ok()
    })
}

fn cached_entries(cache_dir: &Path) -> Vec<(PathBuf, std::time::SystemTime)> {
    let Ok(rd) = fs::read_dir(cache_dir) else {
        return Vec::new();
    };
    rd.filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == CACHE_EXT))
        .filter_map(|e| {
            let t = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), t))
        })
        .collect()
}

/// The schema for `key`: from the cache when it has one, else scanned from `exe` and
/// written to the cache (atomically; a failed write only costs a rescan next time).
pub fn load_or_extract(
    exe: &Path,
    cache_dir: &Path,
    key: &CacheKey,
) -> Result<Schema, SchemaError> {
    if let Some(s) = load_cached(cache_dir, key) {
        return Ok(s);
    }
    let io_err = |source| SchemaError::Io {
        path: exe.to_owned(),
        source,
    };
    let f = fs::File::open(exe).map_err(io_err)?;
    let seen = scan(io::BufReader::with_capacity(1 << 20, f)).map_err(io_err)?;
    let schema = Schema::from_files(closure(&seen)?, key.as_str(), SchemaOrigin::Extracted)?;
    let _ = store(cache_dir, key, &schema.to_set_bytes());
    Ok(schema)
}

fn store(cache_dir: &Path, key: &CacheKey, bytes: &[u8]) -> io::Result<()> {
    fs::create_dir_all(cache_dir)?;
    let dst = cache_file(cache_dir, key);
    let tmp = dst.with_extension(format!("{CACHE_EXT}.partial"));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, &dst)?;
    prune(cache_dir, &dst);
    Ok(())
}

/// Keep the [`KEEP_KEYS`] newest cached schemas (always `keep`).
fn prune(cache_dir: &Path, keep: &Path) {
    let mut entries: Vec<_> = cached_entries(cache_dir)
        .into_iter()
        .filter(|(p, _)| p != keep)
        .collect();
    entries.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    for (p, _) in entries.into_iter().skip(KEEP_KEYS.saturating_sub(1)) {
        let _ = fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock;

    /// Deterministic filler (xorshift), with the prefix sprinkled in as decoys.
    fn noise(n: usize, seed: u64) -> Vec<u8> {
        let mut x = seed | 1;
        let mut v: Vec<u8> = (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        let decoy = format!("\x0a\x20{PREFIX}not_a_descriptor.proto\x12");
        for at in (100..n.saturating_sub(200)).step_by(997) {
            v[at..at + decoy.len()].copy_from_slice(decoy.as_bytes());
        }
        v
    }

    fn exe_bytes(at: &[usize]) -> Vec<u8> {
        let files = mock::file_descriptor_protos();
        let mut b = noise(40_000, 7);
        for (i, fd) in files.iter().enumerate() {
            let enc = fd.encode_to_vec();
            let pos = at[i];
            b.splice(pos..pos, enc);
        }
        // A cut-short copy of api.proto earlier on: the whole one must win.
        let api = files
            .iter()
            .find(|f| f.name() == MARKER)
            .unwrap()
            .encode_to_vec();
        b.splice(50..50, api[..api.len() / 2].iter().copied());
        b
    }

    #[test]
    fn finds_synthetic_descriptors_in_noise() {
        let b = exe_bytes(&[5_000, 21_000]);
        let seen = scan(&b[..]).unwrap();
        let names: Vec<&str> = seen.keys().map(String::as_str).collect();
        assert!(names.contains(&MARKER), "{names:?}");
        assert!(
            names.contains(&"sc/game/telemetry/v1/types.proto"),
            "{names:?}"
        );
        for fd in mock::file_descriptor_protos() {
            assert_eq!(seen[fd.name()], fd, "{} decodes whole", fd.name());
        }
        let s = Schema::from_files(closure(&seen).unwrap(), "t", SchemaOrigin::Extracted).unwrap();
        assert!(s.method("TelemetryStream").is_some());
        assert!(s.method("TriggerGameAction").is_some());
    }

    #[test]
    fn small_windows_find_descriptors_across_edges() {
        let b = exe_bytes(&[4_095, 12_287]);
        for chunk in [1_000, 4_096, 4_097, 7_777] {
            let seen = scan_windowed(&b[..], chunk, 16_384).unwrap();
            for fd in mock::file_descriptor_protos() {
                assert_eq!(
                    seen.get(fd.name()),
                    Some(&fd),
                    "chunk {chunk}: {}",
                    fd.name()
                );
            }
        }
    }

    #[test]
    fn missing_marker_or_import_is_an_error() {
        let files = mock::file_descriptor_protos();
        let only_types: Vec<u8> = files
            .iter()
            .filter(|f| f.name() != MARKER)
            .flat_map(|f| f.encode_to_vec())
            .collect();
        assert!(matches!(
            extract(&only_types[..], "k"),
            Err(SchemaError::NotFound)
        ));
        let only_api: Vec<u8> = files
            .iter()
            .filter(|f| f.name() == MARKER)
            .flat_map(|f| f.encode_to_vec())
            .collect();
        assert!(matches!(
            extract(&only_api[..], "k"),
            Err(SchemaError::MissingImport(n)) if n == "sc/game/telemetry/v1/types.proto"
        ));
    }

    #[test]
    fn cache_hit_skips_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("StarCitizen.exe");
        fs::write(&exe, exe_bytes(&[3_000, 30_000])).unwrap();
        let cache = dir.path().join("cache").join("telemetry");
        let key = CacheKey::sanitized("sc-alpha-5.0.0-1234");
        let first = load_or_extract(&exe, &cache, &key).unwrap();
        assert_eq!(first.origin(), SchemaOrigin::Extracted);
        assert!(cache.join("sc-alpha-5.0.0-1234.fds").is_file());
        // The exe no longer holds a schema: only the cache can answer.
        fs::write(&exe, b"not an exe").unwrap();
        let second = load_or_extract(&exe, &cache, &key).unwrap();
        assert_eq!(second.origin(), SchemaOrigin::Cache);
        assert_eq!(second.file_names(), first.file_names());
        assert_eq!(second.key(), key.as_str());
        // Another key means another build: scanned again (and now not found).
        let other = CacheKey::sanitized("sc-alpha-5.0.0-1235");
        assert!(matches!(
            load_or_extract(&exe, &cache, &other),
            Err(SchemaError::NotFound)
        ));
        assert_eq!(load_newest_cached(&cache).unwrap().key(), key.as_str());
    }

    #[test]
    fn corrupt_cache_is_rescanned() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("StarCitizen.exe");
        fs::write(&exe, exe_bytes(&[3_000, 30_000])).unwrap();
        let key = CacheKey::sanitized("k");
        fs::write(dir.path().join("k.fds"), b"\xff\xff garbage").unwrap();
        let s = load_or_extract(&exe, dir.path(), &key).unwrap();
        assert_eq!(s.origin(), SchemaOrigin::Extracted);
        assert!(load_cached(dir.path(), &key).is_some());
    }

    #[test]
    fn prune_keeps_the_newest_keys() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = mock::schema().to_set_bytes();
        for i in 0..5 {
            store(dir.path(), &CacheKey::sanitized(&format!("k{i}")), &bytes).unwrap();
            // Distinct mtimes on coarse file systems.
            let t = std::time::SystemTime::now() + std::time::Duration::from_secs(i);
            let f = fs::File::options()
                .write(true)
                .open(dir.path().join(format!("k{i}.fds")))
                .unwrap();
            f.set_modified(t).unwrap();
        }
        let mut left: Vec<String> = cached_entries(dir.path())
            .into_iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left.len(), KEEP_KEYS);
        assert!(left.contains(&"k4.fds".to_owned()));
    }

    #[test]
    fn cache_keys_are_file_name_safe() {
        assert_eq!(
            CacheKey::sanitized("sc-alpha-5.0.0-1").as_str(),
            "sc-alpha-5.0.0-1"
        );
        assert_eq!(CacheKey::sanitized("../x/y").as_str(), "_.._x_y");
        assert_eq!(CacheKey::sanitized("").as_str(), "_");
        let b = BuildInfo {
            branch: "sc-alpha-5.0.0".into(),
            game_version: None,
            change: Some(13248297),
            version: String::new(),
        };
        assert_eq!(
            CacheKey::for_build(&b).unwrap().as_str(),
            "sc-alpha-5.0.0-13248297"
        );
        assert!(CacheKey::for_build(&BuildInfo { change: None, ..b }).is_none());
    }

    /// Reads a real exe: `STARORG_SC_EXE=/path/to/StarCitizen.exe cargo test -p
    /// starorg-telemetry -- --ignored real_exe`. Prints counts only; writes nothing to the repo.
    #[test]
    #[ignore = "needs a real StarCitizen.exe in STARORG_SC_EXE"]
    fn real_exe() {
        let Some(exe) = std::env::var_os("STARORG_SC_EXE") else {
            eprintln!("STARORG_SC_EXE not set; skipped");
            return;
        };
        let exe = PathBuf::from(exe);
        let dir = tempfile::tempdir().unwrap();
        let key = CacheKey::for_exe(&exe).unwrap();
        let t = std::time::Instant::now();
        let s = load_or_extract(&exe, dir.path(), &key).unwrap();
        eprintln!(
            "{} files, {} methods, scanned in {:?}",
            s.file_names().len(),
            s.service().methods().len(),
            t.elapsed()
        );
        for m in [
            "TelemetryStream",
            "ListTriggerableActions",
            "TriggerGameAction",
        ] {
            assert!(s.method(m).is_some(), "{m}");
        }
        assert_eq!(
            load_or_extract(&exe, dir.path(), &key).unwrap().origin(),
            SchemaOrigin::Cache
        );
    }
}
