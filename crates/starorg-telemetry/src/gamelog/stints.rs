//! Server and shard join history: one row per shard stint (join to leave), folded from the
//! session lines [`super::session`] already reads, kept in a small capped file in the app's
//! data folder, and backfilled from the channel's `logbackups/*.log`.
//!
//! - A stint starts at `<Join PU>` and ends at the next of: back at the main menu (the
//!   `Frontend` loading screen), another join, a real shard change (`<Update Shard Id>` with a
//!   new id; the repeat of the joined shard is not one), a new `Log started` or the end of the
//!   log. A game-server disconnect inside it (the `_Frontend` menu channel never counts; lines
//!   within 2 s are one disconnect) makes the reason `disconnect` (your own quit) or `dropped`,
//!   with the time and `uptime_secs` of that disconnect.
//! - A row holds the channel, build, shard and region, times, the reason, the star systems
//!   seen, the disconnect count and whether it dropped. Never the server address or port, a
//!   session id, a handle or a raw disconnect line (the session parser does not even read
//!   them). Rows are unique by (log start, join time, shard), so reading the same log again
//!   (the live `Game.log`, then its copy in `logbackups/`) adds nothing.
//! - [`Store`]: `<data>/telemetry/sessions/history.json`, at most [`MAX_ROWS`] rows and
//!   [`MAX_FILE_BYTES`] (the oldest go first), written through a temporary file and a rename,
//!   never held open. **Clear** stays cleared: it keeps a watermark (`cleared_ms`), and a stint
//!   that joined before it is never added again (the live log read again, or the same log
//!   later found in `logbackups/`). A file that is not a history is set aside as [`BAD_NAME`]
//!   before anything is written; one of a newer format is shown but never written.
//! - [`Backfill`]: the newest [`BACKFILL_FILES`] backups not read before, [`READ_CHUNK`] bytes
//!   per read (opened and closed for each), at most [`BACKFILL_FILE_MAX`] bytes of one file
//!   (the stint still open at that point is left out, not ended there). A read that fails
//!   (the file locked for a moment, gone) marks nothing: the file is read again at a later
//!   start, at most [`MAX_ATTEMPTS`] times while the app runs.

use super::session::SessionState;
use super::{Ev, EventKind, LINE_MAX, parse_ts, split_ts};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Rows kept at most (the oldest go first).
pub const MAX_ROWS: usize = 2000;
/// The history file stays under this (the oldest rows go first).
pub const MAX_FILE_BYTES: usize = 2 << 20;
/// Star systems kept per row.
pub const MAX_SYSTEMS: usize = 12;
/// Backups read per reader start at most (newest first).
pub const BACKFILL_FILES: usize = 40;
/// Bytes read from a backup in one read.
pub const READ_CHUNK: u64 = 4 << 20;
/// Bytes of one backup read at most (a longer one is read up to here).
pub const BACKFILL_FILE_MAX: u64 = 128 << 20;
/// Backups remembered as read.
pub const MAX_SCANNED: usize = 1000;
/// The history file's name in its folder.
pub const FILE_NAME: &str = "history.json";
/// Where a history file that cannot be read is set aside (before a new one is written).
pub const BAD_NAME: &str = "history.json.bad";
/// The history file format written (a newer one is left alone).
pub const FORMAT: u32 = 1;
/// Failed reads of one backup per app run before it waits for the next run.
pub const MAX_ATTEMPTS: u32 = 3;

/// Why a stint ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeftReason {
    /// Back at the main menu, or another join without a disconnect first.
    Leave,
    /// The game moved you to another shard.
    ShardChange,
    /// You disconnected (quit, exit to menu).
    Disconnect,
    /// The connection dropped (kicked, timed out, server gone).
    Dropped,
    /// The log ended (or the game started again) while you were on the shard.
    LogEnd,
}

impl LeftReason {
    pub const ALL: [LeftReason; 5] = [
        LeftReason::Leave,
        LeftReason::ShardChange,
        LeftReason::Disconnect,
        LeftReason::Dropped,
        LeftReason::LogEnd,
    ];
}

/// One shard stint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stint {
    /// `LIVE`, `PTU`... (the log's `Executable:` line).
    pub channel: Option<String>,
    pub build: Option<String>,
    pub shard: String,
    /// `USE1B · 020`.
    pub shard_short: Option<String>,
    pub region_code: Option<String>,
    /// `US East`.
    pub region: Option<String>,
    /// The log's `Log started on` text (part of the row's identity).
    pub log_started: Option<String>,
    /// Log time, Unix ms.
    pub joined_ms: i64,
    /// `None` while still on the shard.
    pub left_ms: Option<i64>,
    pub left_reason: Option<LeftReason>,
    /// Seconds on the shard (the disconnect's own `uptime_secs` when it gave one).
    pub uptime_s: Option<f64>,
    /// Star systems seen, in order, at most [`MAX_SYSTEMS`].
    pub systems: Vec<String>,
    pub disconnects: u32,
    pub dropped: bool,
}

impl Stint {
    /// (log start, join time, shard): one row per stint, however often the log is read.
    pub fn key(&self) -> (String, i64, String) {
        (
            self.log_started.clone().unwrap_or_default(),
            self.joined_ms,
            self.shard.clone(),
        )
    }
}

/// The last disconnect of the open stint: time, dropped, uptime.
#[derive(Debug, Clone, Copy)]
struct Dc {
    t: Option<i64>,
    dropped: bool,
    uptime_s: Option<f64>,
}

/// Folds session events into stints.
#[derive(Debug, Default)]
pub struct Tracker {
    open: Option<(Stint, Option<Dc>)>,
    closed: Vec<Stint>,
    last_t: Option<i64>,
}

impl Tracker {
    /// The stint you are on now. After a disconnect that nothing followed yet (the game
    /// closed, or it is on its way to the menu) it carries that disconnect's reason and time,
    /// so it does not read as still on the shard.
    pub fn current(&self) -> Option<Stint> {
        let (s, dc) = self.open.as_ref()?;
        let mut s = s.clone();
        if let Some(d) = dc {
            s.left_reason = Some(if d.dropped {
                LeftReason::Dropped
            } else {
                LeftReason::Disconnect
            });
            s.left_ms = d.t.map(|t| t.max(s.joined_ms));
            s.uptime_s = d.uptime_s;
        }
        Some(s)
    }

    /// Stints that ended since the last call, oldest first.
    pub fn drain(&mut self) -> Vec<Stint> {
        std::mem::take(&mut self.closed)
    }

    /// A time-stamped line was read (the end time of a log that just stops).
    pub fn saw(&mut self, t: Option<i64>) {
        if t.is_some() {
            self.last_t = t;
        }
    }

    /// The events of one session line, with the session state after it.
    pub fn observe(&mut self, evs: &[Ev], s: &SessionState, t: Option<i64>) {
        for (kind, _, detail) in evs {
            match kind {
                EventKind::LogStarted => {
                    self.close(LeftReason::LogEnd, self.last_t);
                    self.last_t = None;
                }
                EventKind::SessionJoin => {
                    self.close(LeftReason::Leave, t);
                    self.start(s, t);
                }
                EventKind::SessionShardChange => {
                    self.close(LeftReason::ShardChange, t);
                    self.start(s, t);
                }
                EventKind::SessionLeave => self.close(LeftReason::Leave, t),
                EventKind::SessionSystem => {
                    if let Some((st, _)) = &mut self.open
                        && detail != "Main menu"
                        && st.systems.last() != Some(detail)
                        && st.systems.len() < MAX_SYSTEMS
                    {
                        st.systems.push(detail.clone());
                    }
                }
                EventKind::SessionDisconnect | EventKind::SessionDropped => {
                    if let Some((st, dc)) = &mut self.open {
                        let dropped = *kind == EventKind::SessionDropped;
                        st.disconnects += 1;
                        st.dropped |= dropped;
                        *dc = Some(Dc {
                            t,
                            dropped,
                            uptime_s: s.last_dc_uptime_s,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// The log ended (end of a backup, the live file replaced): an open stint ends with it.
    pub fn end_log(&mut self) {
        self.close(LeftReason::LogEnd, self.last_t);
    }

    /// Only part of the log was read: a stint still open there did not end there, so it is
    /// left out.
    pub fn cut_log(&mut self) {
        self.open = None;
    }

    fn start(&mut self, s: &SessionState, t: Option<i64>) {
        let v = s.view();
        let (Some(joined_ms), Some(shard)) = (t, v.shard) else {
            return;
        };
        let system = v.system.filter(|x| x != "Main menu");
        self.open = Some((
            Stint {
                channel: v.channel,
                build: v.build,
                shard,
                shard_short: v.shard_short,
                region_code: v.region_code,
                region: v.region_name,
                log_started: v.log_started,
                joined_ms,
                left_ms: None,
                left_reason: None,
                uptime_s: None,
                systems: system.into_iter().collect(),
                disconnects: 0,
                dropped: false,
            },
            None,
        ));
    }

    fn close(&mut self, reason: LeftReason, t: Option<i64>) {
        let Some((mut st, dc)) = self.open.take() else {
            return;
        };
        let (reason, left, up) = match dc {
            Some(d) => (
                if d.dropped {
                    LeftReason::Dropped
                } else {
                    LeftReason::Disconnect
                },
                d.t.or(t).or(self.last_t),
                d.uptime_s,
            ),
            None => (reason, t.or(self.last_t), None),
        };
        st.left_reason = Some(reason);
        st.left_ms = left.map(|l| l.max(st.joined_ms));
        st.uptime_s = up.or_else(|| st.left_ms.map(|l| (l - st.joined_ms) as f64 / 1000.0));
        self.closed.push(st);
    }
}

/// Session lines only (a backup): the [`SessionState`] and a [`Tracker`], no signals or people.
#[derive(Debug, Default)]
pub struct SessionFold {
    session: SessionState,
    pub tracker: Tracker,
    partial: Vec<u8>,
}

impl SessionFold {
    pub fn line(&mut self, line: &str) {
        let line = line
            .trim_end_matches(['\r', '\n'])
            .trim_start_matches('\u{feff}');
        let (ts, body) = split_ts(line);
        let t = ts.and_then(parse_ts);
        self.tracker.saw(t);
        if let Some(evs) = self.session.apply(body, t) {
            self.tracker.observe(&evs, &self.session, t);
        }
    }

    pub fn bytes(&mut self, data: &[u8]) {
        let mut rest = data;
        while let Some(nl) = memchr::memchr(b'\n', rest) {
            let (line, tail) = rest.split_at(nl);
            rest = &tail[1..];
            if self.partial.is_empty() {
                self.line(&String::from_utf8_lossy(line));
            } else {
                let mut l = std::mem::take(&mut self.partial);
                l.extend_from_slice(line);
                if l.len() <= LINE_MAX {
                    self.line(&String::from_utf8_lossy(&l));
                }
            }
        }
        self.partial.extend_from_slice(rest);
        if self.partial.len() > LINE_MAX {
            self.partial.clear();
        }
    }

    /// End of the file: a last line without a newline, then an open stint ends.
    pub fn finish(mut self) -> Vec<Stint> {
        let p = std::mem::take(&mut self.partial);
        if !p.is_empty() && p.len() <= LINE_MAX {
            self.line(&String::from_utf8_lossy(&p));
        }
        self.tracker.end_log();
        self.tracker.drain()
    }

    /// Only the start of the file was read (the size cap): the stints that ended in that part;
    /// one still open at the cut is left out.
    pub fn finish_cut(mut self) -> Vec<Stint> {
        self.tracker.cut_log();
        self.tracker.drain()
    }
}

/// A backup already read: by name and size (the game never writes to one again).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scanned {
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Doc {
    #[serde(default)]
    format: u32,
    /// The last Clear (wall time, Unix ms): a stint that joined at or before it is not added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cleared_ms: Option<i64>,
    /// Oldest first.
    #[serde(default)]
    scanned: Vec<Scanned>,
    /// Oldest first (by join time).
    #[serde(default)]
    stints: Vec<Stint>,
}

/// The join history: a file in the data folder, or memory only (`dir` `None`: the mock).
#[derive(Debug)]
pub struct Store {
    dir: Option<PathBuf>,
    doc: Doc,
    keys: HashSet<(String, i64, String)>,
    loaded: bool,
    /// The file is of a newer format: shown, never written.
    frozen: bool,
    /// A file that could not be parsed is set aside before the first write.
    set_aside: bool,
    /// Failed reads of a backup this app run (memory only).
    attempts: HashMap<Scanned, u32>,
    /// Bumped at every change (the UI reads the list again).
    pub revision: u64,
    pub error: Option<String>,
}

impl Store {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            doc: Doc {
                format: FORMAT,
                ..Doc::default()
            },
            keys: HashSet::new(),
            loaded: false,
            frozen: false,
            set_aside: false,
            attempts: HashMap::new(),
            revision: 0,
            error: None,
        }
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(FILE_NAME))
    }

    /// Read the file once (a missing or unreadable one is an empty history).
    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let Some(p) = self.path() else { return };
        match std::fs::read(&p) {
            Ok(b) => match serde_json::from_slice::<Doc>(&b) {
                Ok(d) => {
                    self.frozen = d.format > FORMAT;
                    if self.frozen {
                        self.error = Some(format!(
                            "{} was written by a newer version of the app (format {}); it is shown but not changed",
                            p.display(),
                            d.format
                        ));
                    }
                    self.doc = d;
                    self.doc.format = self.doc.format.max(FORMAT);
                    self.doc.stints.sort_by_key(|s| s.joined_ms);
                    self.keys = self.doc.stints.iter().map(Stint::key).collect();
                }
                Err(e) => {
                    self.set_aside = true;
                    self.error = Some(format!(
                        "{} is not a history file ({e}); it is kept as {BAD_NAME} when a new one is written",
                        p.display()
                    ));
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => self.error = Some(format!("cannot read {}: {e}", p.display())),
        }
    }

    /// Newest first.
    pub fn rows(&mut self) -> Vec<Stint> {
        self.load();
        self.doc.stints.iter().rev().cloned().collect()
    }

    pub fn is_scanned(&mut self, s: &Scanned) -> bool {
        self.load();
        self.doc.scanned.contains(s)
    }

    /// A backup to read at this start: not read before, and not failed [`MAX_ATTEMPTS`] times
    /// while the app runs.
    pub fn wants(&mut self, s: &Scanned) -> bool {
        !self.is_scanned(s) && self.attempts.get(s).copied().unwrap_or(0) < MAX_ATTEMPTS
    }

    /// Reading `s` failed: nothing is marked, a later start tries again.
    pub fn failed(&mut self, s: Scanned) {
        if self.attempts.len() >= MAX_SCANNED {
            self.attempts.clear();
        }
        *self.attempts.entry(s).or_default() += 1;
    }

    /// The last Clear, if any (wall time, Unix ms).
    pub fn cleared_ms(&mut self) -> Option<i64> {
        self.load();
        self.doc.cleared_ms
    }

    /// Add rows not seen before (and mark `scanned`); saved when anything changed. Returns
    /// how many rows were new.
    pub fn add(&mut self, rows: Vec<Stint>, scanned: Option<Scanned>) -> usize {
        self.load();
        let mut n = 0;
        let cleared = self.doc.cleared_ms;
        for r in rows {
            // Joined before the last Clear: it was cleared (a log read again brings it back).
            if cleared.is_some_and(|c| r.joined_ms <= c) {
                continue;
            }
            if self.keys.insert(r.key()) {
                self.doc.stints.push(r);
                n += 1;
            }
        }
        let mut changed = n > 0;
        if let Some(s) = scanned
            && !self.doc.scanned.contains(&s)
        {
            self.doc.scanned.push(s);
            let over = self.doc.scanned.len().saturating_sub(MAX_SCANNED);
            self.doc.scanned.drain(..over);
            changed = true;
        }
        if n > 0 {
            self.doc.stints.sort_by_key(|s| s.joined_ms);
            let over = self.doc.stints.len().saturating_sub(MAX_ROWS);
            for r in self.doc.stints.drain(..over) {
                self.keys.remove(&r.key());
            }
            self.revision += 1;
        }
        if changed {
            self.save();
        }
        n
    }

    /// Forget every row as of `now_ms` (wall time, Unix ms): no stint that joined before then
    /// comes back, whichever log it is read from again. The backups read stay marked.
    pub fn clear(&mut self, now_ms: i64) {
        self.load();
        self.doc.stints.clear();
        self.keys.clear();
        self.doc.cleared_ms = Some(self.doc.cleared_ms.map_or(now_ms, |c| c.max(now_ms)));
        self.revision += 1;
        self.save();
    }

    fn save(&mut self) {
        let Some(dir) = self.dir.clone() else { return };
        if self.frozen {
            return;
        }
        if self.set_aside {
            // The unreadable file is kept, once, before it is replaced.
            match std::fs::rename(dir.join(FILE_NAME), dir.join(BAD_NAME)) {
                Ok(()) => self.set_aside = false,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.set_aside = false,
                Err(e) => {
                    self.error = Some(format!(
                        "cannot set {} aside: {e}; it is not replaced",
                        dir.join(FILE_NAME).display()
                    ));
                    return;
                }
            }
        }
        let mut text = match serde_json::to_vec(&self.doc) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        while text.len() > MAX_FILE_BYTES && !self.doc.stints.is_empty() {
            let cut = (self.doc.stints.len() / 10).max(1);
            for r in self.doc.stints.drain(..cut) {
                self.keys.remove(&r.key());
            }
            text = serde_json::to_vec(&self.doc).unwrap_or_default();
        }
        self.error = write_atomic(&dir, FILE_NAME, &text)
            .err()
            .map(|e| format!("cannot write {}: {e}", dir.join(FILE_NAME).display()));
    }
}

/// `dir/name` through `dir/name.tmp` and a rename (replaces the old file, on Windows too).
fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, dir.join(name))
}

/// One backup being read.
#[derive(Debug)]
struct Reading {
    path: PathBuf,
    scanned: Scanned,
    pos: u64,
    fold: SessionFold,
}

/// Reading the channel's `logbackups/*.log` in, one chunk per [`Backfill::step`].
#[derive(Debug)]
pub struct Backfill {
    queue: VecDeque<(PathBuf, Scanned)>,
    reading: Option<Reading>,
    pub files_read: u32,
    /// Bytes of one file read at most ([`BACKFILL_FILE_MAX`]).
    pub file_max: u64,
}

impl Default for Backfill {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            reading: None,
            files_read: 0,
            file_max: BACKFILL_FILE_MAX,
        }
    }
}

/// What one [`Backfill::step`] finished.
#[derive(Debug)]
pub enum Step {
    /// A file was read (to its end or the size cap): its stints, and its mark for
    /// [`Store::add`].
    Read(Vec<Stint>, Scanned),
    /// A read failed: nothing of the file counts ([`Store::failed`]; tried again later).
    Failed(Scanned),
}

/// The channel's `logbackups` folder (on-disk spelling).
pub fn backups_dir(channel_dir: &Path) -> Option<PathBuf> {
    let r = starorg_core::paths::resolve_ci(channel_dir, &["logbackups"]).ok()?;
    r.exists().then_some(r.path)
}

impl Backfill {
    /// The newest [`BACKFILL_FILES`] `*.log` in `dir` that `store` has not read.
    pub fn plan(dir: Option<&Path>, store: &mut Store) -> Self {
        let mut files: Vec<(std::time::SystemTime, PathBuf, Scanned)> = Vec::new();
        if let Some(dir) = dir
            && let Ok(rd) = std::fs::read_dir(dir)
        {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let is_log = Path::new(&name)
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("log"));
                // Plain files only: never open a directory.
                let Ok(m) = e.metadata() else { continue };
                if !is_log || !m.is_file() {
                    continue;
                }
                let mtime = m.modified().unwrap_or(std::time::UNIX_EPOCH);
                files.push((
                    mtime,
                    e.path(),
                    Scanned {
                        name,
                        bytes: m.len(),
                    },
                ));
            }
        }
        files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.2.name.cmp(&a.2.name)));
        let queue = files
            .into_iter()
            .take(BACKFILL_FILES)
            .filter(|(_, _, s)| store.wants(s))
            .map(|(_, p, s)| (p, s))
            .collect();
        Self {
            queue,
            ..Self::default()
        }
    }

    pub fn pending(&self) -> usize {
        self.queue.len() + usize::from(self.reading.is_some())
    }

    pub fn is_done(&self) -> bool {
        self.pending() == 0
    }

    /// Read one chunk (no lock needed); `Some` when a file is done or failed.
    pub fn step(&mut self) -> Option<Step> {
        if self.reading.is_none() {
            let (path, scanned) = self.queue.pop_front()?;
            self.reading = Some(Reading {
                path,
                scanned,
                pos: 0,
                fold: SessionFold::default(),
            });
        }
        let r = self.reading.as_mut()?;
        let left = self.file_max.saturating_sub(r.pos).min(READ_CHUNK);
        let done = match read_chunk(&r.path, r.pos, left) {
            Ok(b) => {
                r.pos += b.len() as u64;
                r.fold.bytes(&b);
                (b.len() as u64) < left || left == 0
            }
            // Locked for a moment (Windows sharing, a virus scanner) or gone: nothing of it
            // counts and it is not marked read, so a later start reads it whole.
            Err(_) => {
                let r = self.reading.take()?;
                return Some(Step::Failed(r.scanned));
            }
        };
        if !done {
            return None;
        }
        let r = self.reading.take()?;
        self.files_read += 1;
        let rows = if r.pos >= self.file_max && r.scanned.bytes > self.file_max {
            r.fold.finish_cut()
        } else {
            r.fold.finish()
        };
        Some(Step::Read(rows, r.scanned))
    }
}

/// Up to `max` bytes of `path` from `pos`; the file is closed again before this returns.
fn read_chunk(path: &Path, pos: u64, max: u64) -> std::io::Result<Vec<u8>> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::Error::other("not a file"));
    }
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(pos))?;
    let mut b = Vec::new();
    f.take(max).read_to_end(&mut b)?;
    Ok(b)
}

#[cfg(test)]
mod tests;
