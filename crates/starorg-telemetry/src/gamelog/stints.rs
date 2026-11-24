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
//!   never held open.
//! - [`Backfill`]: the newest [`BACKFILL_FILES`] backups not read before, [`READ_CHUNK`] bytes
//!   per read (opened and closed for each), at most [`BACKFILL_FILE_MAX`] bytes of one file.

use super::session::SessionState;
use super::{Ev, EventKind, LINE_MAX, parse_ts, split_ts};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
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
    /// The stint you are on now.
    pub fn current(&self) -> Option<&Stint> {
        self.open.as_ref().map(|(s, _)| s)
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
    /// Bumped at every change (the UI reads the list again).
    pub revision: u64,
    pub error: Option<String>,
}

impl Store {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            doc: Doc {
                format: 1,
                ..Doc::default()
            },
            keys: HashSet::new(),
            loaded: false,
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
                    self.doc = d;
                    self.doc.format = 1;
                    self.doc.stints.sort_by_key(|s| s.joined_ms);
                    self.keys = self.doc.stints.iter().map(Stint::key).collect();
                }
                Err(e) => self.error = Some(format!("{} is not a history file: {e}", p.display())),
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

    /// Add rows not seen before (and mark `scanned`); saved when anything changed. Returns
    /// how many rows were new.
    pub fn add(&mut self, rows: Vec<Stint>, scanned: Option<Scanned>) -> usize {
        self.load();
        let mut n = 0;
        for r in rows {
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

    /// Forget every row (the backups read stay marked, so they are not read in again).
    pub fn clear(&mut self) {
        self.load();
        self.doc.stints.clear();
        self.keys.clear();
        self.revision += 1;
        self.save();
    }

    fn save(&mut self) {
        let Some(dir) = self.dir.clone() else { return };
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
#[derive(Debug, Default)]
pub struct Backfill {
    queue: VecDeque<(PathBuf, Scanned)>,
    reading: Option<Reading>,
    pub files_read: u32,
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
            .filter(|(_, _, s)| !store.is_scanned(s))
            .map(|(_, p, s)| (p, s))
            .collect();
        Self {
            queue,
            reading: None,
            files_read: 0,
        }
    }

    pub fn pending(&self) -> usize {
        self.queue.len() + usize::from(self.reading.is_some())
    }

    pub fn is_done(&self) -> bool {
        self.pending() == 0
    }

    /// Read one chunk (no lock needed); when a file is done, its stints and its mark for
    /// [`Store::add`].
    pub fn step(&mut self) -> Option<(Vec<Stint>, Scanned)> {
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
        let left = BACKFILL_FILE_MAX.saturating_sub(r.pos).min(READ_CHUNK);
        let done = match read_chunk(&r.path, r.pos, left) {
            Ok(b) => {
                r.pos += b.len() as u64;
                r.fold.bytes(&b);
                (b.len() as u64) < left || left == 0
            }
            // Gone or unreadable: what was read counts, the file is not read again.
            Err(_) => true,
        };
        if !done {
            return None;
        }
        let r = self.reading.take()?;
        self.files_read += 1;
        Some((r.fold.finish(), r.scanned))
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
