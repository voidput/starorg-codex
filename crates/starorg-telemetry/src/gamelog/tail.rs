//! Follow one channel's `Game.log` (sc-telemetry `gamelog.py` `GameLogTailer`).
//!
//! - Polling, no file watcher. Each poll opens the file read-only, reads only the bytes added
//!   since the last poll (at most [`READ_MAX_PER_POLL`]) and closes it again: no handle is
//!   kept between polls, so on Windows the game can still move the file to `logbackups/` when
//!   it starts again.
//! - The game writes a new Game.log on every launch. A new file is noticed by a smaller size
//!   than the offset read so far, a changed file head (its first [`HEAD_BYTES`] bytes) or, on
//!   Unix, a changed inode (Windows has no reliable one: FAT, exFAT, network shares); then
//!   everything read is dropped and the new file is read from its start.
//! - The path is the channel folder's `Game.log` in its on-disk spelling
//!   (`starorg_core::paths::resolve_ci`, as the blueprint import finds it), looked up again
//!   while it is missing.

use super::GameLog;
use starorg_core::paths::resolve_ci;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Time between polls.
pub const POLL: Duration = Duration::from_millis(500);
/// File head fingerprint: a same-inode rewrite changes it.
pub const HEAD_BYTES: usize = 128;
/// Bytes read in one poll at most; a big log is caught up over several polls.
pub const READ_MAX_PER_POLL: u64 = 4 << 20;

/// What one poll read.
#[derive(Debug, Default)]
pub struct Read1 {
    /// A new file (or the file went away): drop what was read before.
    pub reset: bool,
    /// The bytes added since the last poll (may end in a partial line).
    pub data: Vec<u8>,
    /// More is waiting (the read hit [`READ_MAX_PER_POLL`]).
    pub more: bool,
}

#[derive(Debug)]
pub struct Tail {
    dir: Option<PathBuf>,
    path: Option<PathBuf>,
    pos: u64,
    head: Vec<u8>,
    #[cfg(unix)]
    ino: Option<(u64, u64)>,
    /// Read at least once (rotation is judged against that read).
    seen: bool,
    pub found: bool,
    pub rotations: u32,
}

impl Tail {
    /// The `Game.log` of a channel folder (`<install>/<CHANNEL>`).
    pub fn for_channel(channel_dir: &Path) -> Self {
        Self::new(Some(channel_dir.to_path_buf()), None)
    }

    /// One file, as given.
    pub fn for_file(path: &Path) -> Self {
        Self::new(None, Some(path.to_path_buf()))
    }

    fn new(dir: Option<PathBuf>, path: Option<PathBuf>) -> Self {
        Self {
            dir,
            path,
            pos: 0,
            head: Vec::new(),
            #[cfg(unix)]
            ino: None,
            seen: false,
            found: false,
            rotations: 0,
        }
    }

    fn resolve(&mut self) -> Option<PathBuf> {
        if self.path.is_none()
            && let Some(dir) = &self.dir
            && let Ok(r) = resolve_ci(dir, &["Game.log"])
            && r.exists()
        {
            self.path = Some(r.path);
        }
        self.path.clone()
    }

    fn restart(&mut self) {
        self.pos = 0;
        self.head.clear();
    }

    /// The file is not there (any more).
    fn gone(&mut self) -> Read1 {
        if self.dir.is_some() {
            self.path = None;
        }
        let was = self.found;
        self.found = false;
        self.seen = false;
        self.restart();
        #[cfg(unix)]
        {
            self.ino = None;
        }
        Read1 {
            reset: was,
            ..Read1::default()
        }
    }

    /// Poll once (blocking file IO; no lock needed: the caller folds the result).
    pub fn read(&mut self) -> Read1 {
        let Some(path) = self.resolve() else {
            return self.gone();
        };
        // Never open a directory (or anything but a plain file).
        match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => {}
            Ok(_) => return self.gone(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return self.gone(),
            Err(_) => return Read1::default(),
        }
        let mut f = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return self.gone(),
            Err(_) => return Read1::default(),
        };
        self.read_open(&mut f).unwrap_or_default()
    }

    fn read_open(&mut self, f: &mut File) -> std::io::Result<Read1> {
        let meta = f.metadata()?;
        let size = meta.len();
        let mut head = Vec::with_capacity(HEAD_BYTES);
        f.by_ref().take(HEAD_BYTES as u64).read_to_end(&mut head)?;
        #[cfg(unix)]
        let ino = {
            use std::os::unix::fs::MetadataExt;
            Some((meta.dev(), meta.ino()))
        };
        #[cfg(unix)]
        let ino_changed = self.ino.is_some() && ino != self.ino;
        #[cfg(not(unix))]
        let ino_changed = false;
        let n = head.len().min(self.head.len());
        let head_changed = head[..n] != self.head[..n];
        let mut out = Read1::default();
        if self.seen && (ino_changed || size < self.pos || head_changed) {
            out.reset = true;
            self.rotations += 1;
            self.restart();
        }
        #[cfg(unix)]
        {
            self.ino = ino;
        }
        if head.len() > self.head.len() {
            self.head = head;
        }
        f.seek(SeekFrom::Start(self.pos))?;
        let mut data = Vec::new();
        f.by_ref().take(READ_MAX_PER_POLL).read_to_end(&mut data)?;
        self.pos += data.len() as u64;
        out.more = data.len() as u64 >= READ_MAX_PER_POLL && self.pos < size;
        out.data = data;
        self.seen = true;
        self.found = true;
        Ok(out)
    }

    /// Poll once and fold what was read into `log`; true when something was read or reset.
    pub fn step(&mut self, log: &mut GameLog, now_ms: i64) -> bool {
        let r = self.read();
        apply(&r, log, now_ms);
        r.reset || !r.data.is_empty()
    }
}

/// Fold one [`Read1`] into `log` (the host does this under its lock, after the file IO).
pub fn apply(r: &Read1, log: &mut GameLog, now_ms: i64) {
    if r.reset {
        log.reset();
    }
    if !r.data.is_empty() {
        log.feed_bytes(&r.data);
    }
    log.end_poll(now_ms);
}
