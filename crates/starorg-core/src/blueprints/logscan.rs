//! `Game.log` / `logbackups/*.log` scan for owned blueprints (addendum §2 post-exit, §3).
//!
//! - A login line `<AccountLoginCharacterStatus_Character> Character: ... - geid G -
//!   accountId A - name N - state STATE_CURRENT` sets the current character of that file.
//!   Lines in any other state, and `nickname="..." playerGEID=...` lines (other players),
//!   are ignored.
//! - `Added notification "Received Blueprint: <name>: "` belongs to the current character of
//!   the same file. Before any login line it is *unattributed*. The queue dump and
//!   `UpdateNotificationItem` lines repeat the text and are not counted.
//! - Scans are incremental: per file we keep size, mtime, the hash of its first bytes, the
//!   offset after the last complete line and the character at that offset. The game renames
//!   `Game.log` into `logbackups/` on the next start; the head hash finds the old record so a
//!   rotated log is not read again.

use super::{Characters, OwnedSet, SCAN_JSON, SCHEMA, SOURCE_LOG, Source};
use crate::fsutil;
use crate::paths::resolve_ci;
use memchr::memmem;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Bytes hashed to recognise a file (the first lines hold the start time to the ms).
pub const HEAD_LEN: u64 = 4096;
/// Never read more than this from one file in one scan (logs are a few MB; a 1 GB log is
/// broken, not a session).
pub const MAX_READ: u64 = 512 << 20;

const LOGIN: &[u8] = b"<AccountLoginCharacterStatus_Character>";
const CURRENT: &[u8] = b" - state STATE_CURRENT";
const BLUEPRINT: &[u8] = b"Added notification \"Received Blueprint: ";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub size: u64,
    pub mtime_ns: u128,
    /// After the last complete line read.
    pub offset: u64,
    pub head_len: u64,
    pub head_sha256: String,
    /// Current character at `offset`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanState {
    pub schema: u32,
    /// Keyed by path relative to the channel folder (`Game.log`, `logbackups/<name>`).
    #[serde(default)]
    pub files: BTreeMap<String, FileState>,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            files: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Login {
        geid: String,
        account_id: Option<String>,
        name: String,
        time: Option<String>,
    },
    Blueprint {
        /// As in the log, tags included, trailing `: ` removed.
        name: String,
        time: Option<String>,
    },
}

/// Result of parsing a buffer of complete and incomplete lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub events: Vec<Event>,
    /// Bytes up to and including the last `\n`.
    pub consumed: usize,
}

fn line_bounds(buf: &[u8], at: usize) -> (usize, Option<usize>) {
    let start = memchr::memrchr(b'\n', &buf[..at]).map_or(0, |p| p + 1);
    let end = memchr::memchr(b'\n', &buf[at..]).map(|p| at + p);
    (start, end)
}

fn line_time(line: &str) -> Option<String> {
    let rest = line.strip_prefix('<')?;
    let end = rest.find('>')?;
    (end <= 40).then(|| rest[..end].to_string())
}

fn field<'a>(line: &'a str, key: &str, stop: &str) -> Option<&'a str> {
    let at = line.find(key)? + key.len();
    let rest = &line[at..];
    let end = rest.find(stop).unwrap_or(rest.len());
    let v = rest[..end].trim();
    (!v.is_empty()).then_some(v)
}

fn parse_login(line: &str) -> Option<Event> {
    if line.contains("nickname=\"") {
        return None;
    }
    let geid = field(line, " geid ", " ")?;
    if !geid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let account_id = field(line, " accountId ", " ")
        .filter(|a| a.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_string);
    let name = field(line, " name ", " - state")?;
    Some(Event::Login {
        geid: geid.to_string(),
        account_id,
        name: name.to_string(),
        time: line_time(line),
    })
}

fn parse_blueprint(line: &str) -> Option<Event> {
    let needle = std::str::from_utf8(BLUEPRINT).ok()?;
    let at = line.find(needle)? + needle.len();
    let rest = &line[at..];
    let end = rest.find('"')?;
    let name = rest[..end].trim().trim_end_matches(':').trim();
    (!name.is_empty()).then(|| Event::Blueprint {
        name: name.to_string(),
        time: line_time(line),
    })
}

/// Find login and blueprint events in the complete lines of `buf`.
pub fn parse(buf: &[u8]) -> Parsed {
    let consumed = memchr::memrchr(b'\n', buf).map_or(0, |p| p + 1);
    let body = &buf[..consumed];
    let mut hits: Vec<(usize, bool)> = memmem::find_iter(body, LOGIN)
        .map(|p| (p, true))
        .chain(memmem::find_iter(body, BLUEPRINT).map(|p| (p, false)))
        .collect();
    hits.sort_unstable();
    let mut events = Vec::new();
    let mut last_line = usize::MAX;
    for (at, login) in hits {
        let (start, end) = line_bounds(body, at);
        let Some(end) = end else { continue };
        if start == last_line {
            continue;
        }
        let raw = &body[start..end];
        let line = String::from_utf8_lossy(raw);
        let line = line.trim_end_matches('\r');
        let ev = if login {
            if memmem::find(raw, CURRENT).is_none() {
                continue;
            }
            parse_login(line)
        } else {
            parse_blueprint(line)
        };
        if let Some(e) = ev {
            last_line = start;
            events.push(e);
        }
    }
    Parsed { events, consumed }
}

/// Everything the scan reads and updates for one channel.
#[derive(Debug, Clone, Default)]
pub struct Store {
    pub characters: Characters,
    /// Loaded on demand; only these are written back.
    pub sets: BTreeMap<String, OwnedSet>,
    pub unattributed: Option<OwnedSet>,
    pub scan: ScanState,
    dirty_sets: std::collections::BTreeSet<String>,
    /// Sets merged into another character: their files are removed on save.
    removed_sets: std::collections::BTreeSet<String>,
    dirty_unattributed: bool,
    dirty_characters: bool,
    dirty_scan: bool,
    /// Back the store up here before the next save writes anything (a merge or a rebuild).
    pub(crate) backup: Option<PathBuf>,
}

impl Store {
    pub fn load(channel_data: &Path) -> Result<Self, String> {
        let scan: ScanState = fsutil::read_json(&super::dir(channel_data).join(SCAN_JSON))
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        Ok(Self {
            characters: super::load_characters(channel_data)?,
            scan,
            ..Self::default()
        })
    }

    pub fn set_mut(&mut self, channel_data: &Path, geid: &str) -> Result<&mut OwnedSet, String> {
        self.removed_sets.remove(geid);
        if !self.sets.contains_key(geid) {
            let s = super::load_set(channel_data, geid)?;
            self.sets.insert(geid.to_string(), s);
        }
        self.dirty_sets.insert(geid.to_string());
        Ok(self.sets.get_mut(geid).expect("inserted above"))
    }

    /// Take a character's set out of the store; its file is removed on save.
    pub(crate) fn take_set(&mut self, channel_data: &Path, geid: &str) -> Result<OwnedSet, String> {
        let s = match self.sets.remove(geid) {
            Some(s) => s,
            None => super::load_set(channel_data, geid)?,
        };
        self.dirty_sets.remove(geid);
        self.removed_sets.insert(geid.to_string());
        Ok(s)
    }

    /// Ids whose set files the next save removes.
    pub fn removed_sets(&self) -> impl Iterator<Item = &str> {
        self.removed_sets.iter().map(String::as_str)
    }

    pub(crate) fn mark_scan(&mut self) {
        self.dirty_scan = true;
    }

    pub(crate) fn unattributed_mut(
        &mut self,
        channel_data: &Path,
    ) -> Result<&mut OwnedSet, String> {
        if self.unattributed.is_none() {
            self.unattributed = Some(super::load_unattributed(channel_data)?);
        }
        self.dirty_unattributed = true;
        Ok(self.unattributed.as_mut().expect("loaded above"))
    }

    pub fn mark_characters(&mut self) {
        self.dirty_characters = true;
    }

    /// Write what changed. Merged-away sets are removed last, after the set they were merged
    /// into is written.
    pub fn save(&self, channel_data: &Path) -> Result<(), String> {
        let dirty = !self.dirty_sets.is_empty()
            || !self.removed_sets.is_empty()
            || self.dirty_unattributed
            || self.dirty_characters
            || self.dirty_scan;
        if dirty && let Some(b) = &self.backup {
            super::repair::backup(channel_data, b)?;
        }
        for g in &self.dirty_sets {
            if let Some(s) = self.sets.get(g) {
                super::save_set(channel_data, s)?;
            }
        }
        if self.dirty_unattributed
            && let Some(u) = &self.unattributed
        {
            super::save_set(channel_data, u)?;
        }
        if self.dirty_characters {
            super::save_characters(channel_data, &self.characters)?;
        }
        if self.dirty_scan {
            fsutil::write_json(&super::dir(channel_data).join(SCAN_JSON), &self.scan)
                .map_err(|e| e.to_string())?;
        }
        for g in &self.removed_sets {
            let p = super::set_path(channel_data, g)?;
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", p.display())),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    pub files: usize,
    pub files_read: usize,
    pub bytes_read: u64,
    pub logins: usize,
    /// New (character geid or `None` for unattributed, display name).
    pub new_blueprints: Vec<(Option<String>, String)>,
    /// Blueprint lines seen (including ones already known).
    pub blueprint_lines: usize,
    /// Character of the newest login line read in this scan.
    pub last_character: Option<String>,
    pub warnings: Vec<String>,
    /// Stopped early because the deadline passed; the rest is read next time.
    pub out_of_time: bool,
    /// "Import blueprints from Game.log" is off (`[sync] import_game_log`): no log was
    /// opened.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    /// Characters merged into another one after this scan (same account, new geid).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<super::repair::Merge>,
    /// Characters that share a name but not an account: kept apart, listed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub same_name: Vec<Vec<String>>,
    /// The one-time repair of an older store ran before this scan ([`super::repair::ensure`]).
    /// Set even when the import is off: the caller saves the store when it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repaired: Option<super::repair::Repaired>,
}

/// Log files of a channel: `logbackups/*.log` (oldest first), then `Game.log`.
pub fn log_files(channel_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(r) = resolve_ci(channel_dir, &["logbackups"])
        && let Ok(rd) = std::fs::read_dir(&r.path)
    {
        let dir_name = r
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "logbackups".into());
        let mut v: Vec<(std::time::SystemTime, String, PathBuf)> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let ok = Path::new(&name)
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("log"));
                let mtime = e.metadata().ok()?.modified().ok()?;
                ok.then(|| (mtime, format!("{dir_name}/{name}"), e.path()))
            })
            .collect();
        v.sort();
        out.extend(v.into_iter().map(|(_, rel, p)| (rel, p)));
    }
    if let Ok(r) = resolve_ci(channel_dir, &["Game.log"])
        && r.path.is_file()
    {
        let rel = r
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Game.log".into());
        out.push((rel, r.path));
    }
    out
}

fn head_hash(f: &mut std::fs::File, len: u64) -> std::io::Result<String> {
    let mut buf = Vec::with_capacity(len as usize);
    f.seek(SeekFrom::Start(0))?;
    f.by_ref().take(len).read_to_end(&mut buf)?;
    if (buf.len() as u64) < len {
        return Ok(String::new());
    }
    Ok(fsutil::sha256_hex(&buf))
}

/// [`scan`] when the Game.log import is on (`[sync] import_game_log`, read from
/// `config.toml` by the caller); off, no log file is listed or opened and `store` is left as
/// it is.
pub fn scan_if(
    import_game_log: bool,
    channel_dir: &Path,
    channel_data: &Path,
    store: &mut Store,
    deadline: Option<Instant>,
) -> ScanReport {
    if !import_game_log {
        let mut rep = ScanReport {
            disabled: true,
            ..ScanReport::default()
        };
        match super::repair::ensure(channel_data, store) {
            Ok(r) => rep.repaired = r,
            Err(e) => rep
                .warnings
                .push(format!("repairing the blueprint store: {e}")),
        }
        return rep;
    }
    scan(channel_dir, channel_data, store, deadline)
}

/// Scan one channel's logs into `store`. Stops between files once `deadline` passes.
pub fn scan(
    channel_dir: &Path,
    channel_data: &Path,
    store: &mut Store,
    deadline: Option<Instant>,
) -> ScanReport {
    let mut rep = ScanReport::default();
    match super::repair::ensure(channel_data, store) {
        Ok(r) => rep.repaired = r,
        Err(e) => rep
            .warnings
            .push(format!("repairing the blueprint store: {e}")),
    }
    let old = std::mem::take(&mut store.scan.files);
    let mut new_files = BTreeMap::new();
    let mut newest_login: Option<(Option<String>, String)> = None;
    let files = log_files(channel_dir);
    rep.files = files.len();
    let mut stopped = false;
    for (rel, path) in files {
        if stopped || deadline.is_some_and(|d| Instant::now() >= d) {
            stopped = true;
            if let Some(s) = old.get(&rel) {
                new_files.insert(rel, s.clone());
            }
            continue;
        }
        match scan_file(
            &rel,
            &path,
            &old,
            channel_data,
            store,
            &mut rep,
            &mut newest_login,
        ) {
            Ok(Some(st)) => {
                new_files.insert(rel, st);
            }
            Ok(None) => {}
            Err(e) => {
                rep.warnings.push(format!("{rel}: {e}"));
                if let Some(s) = old.get(&rel) {
                    new_files.insert(rel, s.clone());
                }
            }
        }
    }
    rep.out_of_time = stopped;
    if new_files != old {
        store.dirty_scan = true;
    }
    store.scan.files = new_files;
    match super::repair::consolidate(channel_data, store) {
        Ok(c) => {
            rep.merged = c.merged;
            rep.same_name = c.same_name;
        }
        Err(e) => rep.warnings.push(format!("merging characters: {e}")),
    }
    for (g, _) in &mut rep.new_blueprints {
        if let Some(x) = g {
            *x = store.characters.canonical(x);
        }
    }
    rep.last_character = newest_login.map(|(_, g)| store.characters.canonical(&g));
    rep
}

#[allow(clippy::too_many_arguments)]
fn scan_file(
    rel: &str,
    path: &Path,
    old: &BTreeMap<String, FileState>,
    channel_data: &Path,
    store: &mut Store,
    rep: &mut ScanReport,
    newest_login: &mut Option<(Option<String>, String)>,
) -> std::io::Result<Option<FileState>> {
    let mut f = std::fs::File::open(path)?;
    let meta = f.metadata()?;
    let size = meta.len();
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    // Continue where this file, or the file it was renamed from, stopped.
    let mut resume: Option<&FileState> = None;
    if let Some(s) = old.get(rel)
        && s.offset <= size
        && s.head_len <= size
        && head_hash(&mut f, s.head_len)? == s.head_sha256
    {
        if s.size == size && s.mtime_ns == mtime_ns {
            return Ok(Some(s.clone()));
        }
        resume = Some(s);
    }
    if resume.is_none() {
        let mut lens: Vec<u64> = old.values().map(|s| s.head_len).collect();
        lens.sort_unstable();
        lens.dedup();
        'find: for len in lens.into_iter().filter(|l| *l > 0 && *l <= size) {
            let h = head_hash(&mut f, len)?;
            for s in old.values() {
                if s.head_len == len && s.head_sha256 == h && s.offset <= size {
                    resume = Some(s);
                    break 'find;
                }
            }
        }
    }
    let (start, mut geid) = resume.map_or((0, None), |s| {
        (
            s.offset,
            s.geid.as_deref().map(|g| store.characters.canonical(g)),
        )
    });
    let want = size - start;
    if want > MAX_READ {
        rep.warnings.push(format!(
            "{rel}: {want} new bytes, more than the {MAX_READ} byte limit; skipped"
        ));
        return Ok(resume.cloned());
    }
    let mut buf = Vec::with_capacity(want as usize);
    f.seek(SeekFrom::Start(start))?;
    f.by_ref().take(want).read_to_end(&mut buf)?;
    rep.files_read += 1;
    rep.bytes_read += buf.len() as u64;
    let parsed = parse(&buf);
    for ev in parsed.events {
        match ev {
            Event::Login {
                geid: g,
                account_id,
                name,
                time,
            } => {
                let Some(g) =
                    store
                        .characters
                        .seen(&g, account_id.as_deref(), &name, time.as_deref())
                else {
                    continue;
                };
                rep.logins += 1;
                store.mark_characters();
                if newest_login
                    .as_ref()
                    .is_none_or(|(t, _)| time.as_ref().is_some_and(|nt| Some(nt) >= t.as_ref()))
                {
                    *newest_login = Some((time.clone(), g.clone()));
                }
                geid = Some(g);
            }
            Event::Blueprint { name, time } => {
                rep.blueprint_lines += 1;
                let src = Source {
                    kind: SOURCE_LOG.into(),
                    time,
                    file: Some(rel.to_string()),
                };
                let set = match &geid {
                    Some(g) => store.set_mut(channel_data, g),
                    None => store.unattributed_mut(channel_data),
                }
                .map_err(std::io::Error::other)?;
                if set.add(&name, src) {
                    rep.new_blueprints
                        .push((geid.clone(), super::strip_tags(&name).to_string()));
                }
            }
        }
    }
    let head_len = size.min(HEAD_LEN);
    Ok(Some(FileState {
        size,
        mtime_ns,
        offset: start + parsed.consumed as u64,
        head_len,
        head_sha256: head_hash(&mut f, head_len)?,
        geid,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const L1: &str = "<2026-01-02T03:04:05.000Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 1001 - accountId 77 - name Test_Pilot - state STATE_CURRENT [Team_GameServices][Login]\n";
    const L2: &str = "<2026-01-02T05:00:00.000Z> [Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 2002 - accountId 77 - name Alt_Pilot - state STATE_CURRENT [Team_GameServices][Login]\n";
    fn bp(t: &str, name: &str) -> String {
        format!(
            "<{t}> [Notice] <SHUDEvent_OnNotification> Added notification \"Received Blueprint: {name}: \" [62] to queue. New queue size: 3, MissionId: [0] [Team_CoreGameplayFeatures][Missions][Comms]\n<{t}>    \"Received Blueprint: {name}: \" [62]\n<{t}> [Notice] <UpdateNotificationItem> Notification \"Received Blueprint: {name}: \" [62], Action: Next [Team]\n"
        )
    }

    #[test]
    fn parse_events_and_ignores() {
        let text = format!(
            "{}<2026-01-02T03:05:00.000Z> nickname=\"Other\" playerGEID=999 <AccountLoginCharacterStatus_Character> geid 5 - name X - state STATE_CURRENT\n<t> <AccountLoginCharacterStatus_Character> Character: geid 3003 - accountId 1 - name Ghost - state STATE_UNSPECIFIED\n{L1}{}partial line \"Received Blueprint: Cut",
            bp("2026-01-02T03:00:00.000Z", "[H] Early Helmet"),
            bp(
                "2026-01-02T03:06:00.000Z",
                "Tailwind Flight Suit Raider Camo"
            ),
        );
        let p = parse(text.as_bytes());
        assert_eq!(p.consumed, text.rfind('\n').unwrap() + 1);
        assert_eq!(
            p.events,
            vec![
                Event::Blueprint {
                    name: "[H] Early Helmet".into(),
                    time: Some("2026-01-02T03:00:00.000Z".into())
                },
                Event::Login {
                    geid: "1001".into(),
                    account_id: Some("77".into()),
                    name: "Test_Pilot".into(),
                    time: Some("2026-01-02T03:04:05.000Z".into())
                },
                Event::Blueprint {
                    name: "Tailwind Flight Suit Raider Camo".into(),
                    time: Some("2026-01-02T03:06:00.000Z".into())
                },
            ]
        );
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let ch = d.path().join("LIVE");
        std::fs::create_dir_all(ch.join("logbackups")).unwrap();
        let data = d.path().join("data");
        (d, ch, data)
    }

    fn run(ch: &Path, data: &Path) -> ScanReport {
        let mut s = Store::load(data).unwrap();
        let r = scan(ch, data, &mut s, None);
        s.save(data).unwrap();
        r
    }

    #[test]
    fn import_off_reads_no_log() {
        let (_d, ch, data) = setup();
        std::fs::write(
            ch.join("Game.log"),
            format!("{L1}{}", bp("2026-01-02T03:06:00.000Z", "[L] Arms One")),
        )
        .unwrap();
        let mut s = Store::load(&data).unwrap();
        let r = scan_if(false, &ch, &data, &mut s, None);
        assert!(r.disabled);
        assert_eq!((r.files, r.files_read, r.bytes_read), (0, 0, 0));
        assert!(s.characters.characters.is_empty() && s.scan.files.is_empty());
        assert!(
            serde_json::to_value(&r).unwrap()["disabled"]
                .as_bool()
                .unwrap()
        );
        // On: the same call reads it.
        let r = scan_if(true, &ch, &data, &mut s, None);
        assert!(!r.disabled);
        assert_eq!((r.files_read, r.new_blueprints.len()), (1, 1));
        assert!(serde_json::to_value(&r).unwrap().get("disabled").is_none());
    }

    #[test]
    fn attribution_incremental_and_rotation() {
        let (_d, ch, data) = setup();
        let game = ch.join("Game.log");
        let pad = format!(
            "<2026-01-02T03:00:00.000Z> BackupNameAttachment=\" Build(1) A\"\n{}",
            "x".repeat(5000)
        );
        std::fs::write(
            &game,
            format!(
                "{pad}\n{}{L1}{}",
                bp("2026-01-02T03:01:00.000Z", "[H] Early Helmet"),
                bp("2026-01-02T03:06:00.000Z", "[L] Arms One")
            ),
        )
        .unwrap();
        let r = run(&ch, &data);
        assert_eq!((r.files_read, r.logins, r.new_blueprints.len()), (1, 1, 2));
        assert_eq!(r.last_character.as_deref(), Some("1001"));
        let set = crate::blueprints::load_set(&data, "1001").unwrap();
        assert_eq!(set.entries.keys().collect::<Vec<_>>(), vec!["arms one"]);
        let un = crate::blueprints::load_unattributed(&data).unwrap();
        assert_eq!(un.entries.keys().collect::<Vec<_>>(), vec!["early helmet"]);

        // Unchanged: nothing read.
        let r = run(&ch, &data);
        assert_eq!((r.files_read, r.bytes_read), (0, 0));

        // Appended lines: only the new bytes are read; still the same character.
        let before = std::fs::metadata(&game).unwrap().len();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&game)
            .unwrap();
        std::io::Write::write_all(
            &mut f,
            bp("2026-01-02T04:00:00.000Z", "Legs Two").as_bytes(),
        )
        .unwrap();
        drop(f);
        let r = run(&ch, &data);
        assert_eq!(r.files_read, 1);
        assert_eq!(
            r.bytes_read,
            std::fs::metadata(&game).unwrap().len() - before
        );
        assert_eq!(
            r.new_blueprints,
            vec![(Some("1001".into()), "Legs Two".into())]
        );

        // Rotation: the game moves Game.log into logbackups and starts a new one.
        std::fs::rename(
            &game,
            ch.join("logbackups/Game Build(1) 02 Jan 26 (03 00 00).log"),
        )
        .unwrap();
        std::fs::write(
            &game,
            format!(
                "<2026-01-02T05:00:00.000Z> start\n{L2}{}",
                bp("2026-01-02T05:10:00.000Z", "Arms One")
            ),
        )
        .unwrap();
        let r = run(&ch, &data);
        assert_eq!(r.files, 2);
        assert_eq!(
            r.files_read, 2,
            "rotated file is resumed at its old offset, new Game.log read"
        );
        assert_eq!(r.blueprint_lines, 1, "the rotated file has no new lines");
        assert_eq!(
            r.new_blueprints,
            vec![(Some("2002".into()), "Arms One".into())]
        );
        assert_eq!(r.last_character.as_deref(), Some("2002"));
        let st: ScanState = fsutil::read_json(&data.join("blueprints/scan.json"))
            .unwrap()
            .unwrap();
        assert_eq!(st.files.len(), 2);
        let alt = crate::blueprints::load_set(&data, "2002").unwrap();
        assert_eq!(alt.entries.len(), 1);
        let chars = crate::blueprints::load_characters(&data).unwrap();
        assert_eq!(chars.newest(), Some("2002"));

        // Idempotent.
        let r = run(&ch, &data);
        assert_eq!((r.files_read, r.new_blueprints.len()), (0, 0));
    }

    #[test]
    fn truncated_log_is_read_again() {
        let (_d, ch, data) = setup();
        let game = ch.join("Game.log");
        std::fs::write(
            &game,
            format!(
                "<2026-01-01T00:00:00.000Z> a\n{L1}{}",
                bp("2026-01-01T00:01:00.000Z", "One")
            ),
        )
        .unwrap();
        run(&ch, &data);
        std::fs::write(
            &game,
            format!(
                "<2026-02-01T00:00:00.000Z> b\n{}",
                bp("2026-02-01T00:01:00.000Z", "Two")
            ),
        )
        .unwrap();
        let r = run(&ch, &data);
        assert_eq!(
            r.new_blueprints,
            vec![(None, "Two".into())],
            "new file, no login yet"
        );
    }

    #[test]
    fn deadline_stops_between_files() {
        let (_d, ch, data) = setup();
        std::fs::write(ch.join("Game.log"), format!("{L1}{}", bp("t", "One"))).unwrap();
        let mut s = Store::load(&data).unwrap();
        let r = scan(&ch, &data, &mut s, Some(Instant::now()));
        assert!(r.out_of_time);
        assert_eq!(r.files_read, 0);
    }

    /// A Windows `Game.log`: UTF-8 BOM, CRLF lines. The character is reset between sessions
    /// (same account and name, new geid): one character, not two, holding both sessions'
    /// blueprints.
    #[test]
    fn crlf_log_and_a_reset_character_stay_one() {
        let (_d, ch, data) = setup();
        let reset = L1.replace("geid 1001", "geid 3003");
        let crlf = |t: String| t.replace('\n', "\r\n");
        std::fs::write(
            ch.join("logbackups/old.log"),
            format!(
                "\u{FEFF}{}",
                crlf(format!(
                    "{L1}{}",
                    bp("2026-01-02T03:06:00.000Z", "[L] Arms One")
                ))
            ),
        )
        .unwrap();
        std::fs::write(
            ch.join("Game.log"),
            crlf(format!(
                "<2026-03-01T00:00:00.000Z> start\n{}{}",
                reset.replace("2026-01-02T03:04:05.000Z", "2026-03-01T00:01:00.000Z"),
                bp("2026-03-01T00:02:00.000Z", "Legs Two")
            )),
        )
        .unwrap();
        let r = run(&ch, &data);
        assert_eq!(r.logins, 2);
        assert_eq!(r.merged.len(), 1, "{r:?}");
        let chars = crate::blueprints::load_characters(&data).unwrap();
        assert_eq!(chars.characters.len(), 1);
        let (g, c) = chars.characters.iter().next().unwrap();
        assert_eq!((g.as_str(), c.name.as_str()), ("3003", "Test_Pilot"));
        assert_eq!(chars.aliases["1001"], "3003");
        let set = crate::blueprints::load_set(&data, "3003").unwrap();
        assert_eq!(
            set.entries.keys().collect::<Vec<_>>(),
            vec!["arms one", "legs two"]
        );
        let sel = crate::blueprints::select(&data, None).unwrap();
        assert_eq!(sel.count, 2, "newest session owns both");

        // Re-reading everything from scratch keeps one character.
        let mut s = Store::load(&data).unwrap();
        let rb = super::super::repair::rebuild_from_logs(&ch, &data, &mut s, false, None).unwrap();
        s.save(&data).unwrap();
        assert!(!rb.changes(), "{rb:?}");
        assert_eq!(
            crate::blueprints::load_characters(&data)
                .unwrap()
                .characters
                .len(),
            1
        );
    }

    /// Same name on two accounts: kept apart, listed for a notice.
    #[test]
    fn same_name_on_another_account_is_listed_not_merged() {
        let (_d, ch, data) = setup();
        let other = L2
            .replace("accountId 77", "accountId 88")
            .replace("Alt_Pilot", "test_pilot");
        std::fs::write(ch.join("Game.log"), format!("{L1}{other}")).unwrap();
        let r = run(&ch, &data);
        assert!(r.merged.is_empty());
        assert_eq!(r.same_name, vec![vec!["1001".to_string(), "2002".into()]]);
        assert_eq!(
            crate::blueprints::load_characters(&data)
                .unwrap()
                .characters
                .len(),
            2
        );
    }

    /// Re-scan from scratch: the log part is rebuilt, an imported entry stays; a dry run
    /// (no save) writes nothing; `replace` drops the import too.
    #[test]
    fn rebuild_keeps_imports_unless_replace() {
        let (_d, ch, data) = setup();
        std::fs::write(
            ch.join("Game.log"),
            format!("{L1}{}", bp("2026-01-02T03:06:00.000Z", "Arms One")),
        )
        .unwrap();
        run(&ch, &data);
        let mut s = Store::load(&data).unwrap();
        let set = s.set_mut(&data, "1001").unwrap();
        set.add(
            "Imported Rifle",
            Source {
                kind: crate::blueprints::SOURCE_SMART_CITIZEN.into(),
                time: None,
                file: None,
            },
        );
        // A name the log no longer has (as if the old scan had read a line wrongly).
        set.add(
            "Ghost Entry",
            Source {
                kind: SOURCE_LOG.into(),
                time: None,
                file: Some("Game.log".into()),
            },
        );
        s.save(&data).unwrap();
        let disk = || crate::blueprints::load_set(&data, "1001").unwrap();
        let before = disk();

        let mut dry = Store::load(&data).unwrap();
        let r = super::super::repair::rebuild_from_logs(&ch, &data, &mut dry, false, None).unwrap();
        assert_eq!(r.characters.len(), 1);
        assert_eq!(r.characters[0].removed, vec!["Ghost Entry"]);
        assert!(r.characters[0].added.is_empty());
        assert_eq!(disk(), before, "dry run: nothing written");

        let mut s = Store::load(&data).unwrap();
        super::super::repair::rebuild_from_logs(&ch, &data, &mut s, false, None).unwrap();
        s.save(&data).unwrap();
        assert_eq!(
            disk().entries.keys().collect::<Vec<_>>(),
            vec!["arms one", "imported rifle"]
        );
        let backups = crate::blueprints::dir(&data).join(super::super::repair::BACKUPS);
        assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 1);

        let mut s = Store::load(&data).unwrap();
        let r = super::super::repair::rebuild_from_logs(&ch, &data, &mut s, true, None).unwrap();
        s.save(&data).unwrap();
        assert_eq!(r.characters[0].removed, vec!["Imported Rifle"]);
        assert_eq!(disk().entries.keys().collect::<Vec<_>>(), vec!["arms one"]);
    }
}
