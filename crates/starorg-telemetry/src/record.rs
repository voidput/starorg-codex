//! Session recordings (sc-telemetry `server.py` `--record`): the frames of a live stream as
//! JSONL in a folder of the app's own, for [`crate::replay`]. Off unless the user turned on
//! "Record sessions"; nothing is sent anywhere.
//!
//! A file is `<YYYYMMDD-HHMMSS>-<CHANNEL>.jsonl` (UTC): a first line
//! `{"t": <unix s>, "starorg": {"format": 1, "channel": .., "schema": ..}}`, then one
//! `{"t": <unix s>, "frame": {..}}` per frame, at most [`RECORD_HZ`] a second. sc-telemetry
//! replays these files and this app replays its files (lines without a frame are skipped by
//! both).
//!
//! Bounded ([`Caps`]): a file stops growing at [`MAX_FILE_BYTES`]; before a new one starts the
//! oldest go until at most [`MAX_FILES`] - 1 are left and a full new file still fits under
//! [`MAX_TOTAL_BYTES`]. Only files named like ours are listed, counted or removed.

use serde::Serialize;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Frames a second written at most (the UI gets 10, the game sends about 20).
pub const RECORD_HZ: u32 = 4;
/// One file at most this big; recording stops there until the next stream.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// All recordings together at most this big.
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// Recordings kept at most.
pub const MAX_FILES: usize = 20;
/// Version of the first line's `starorg` object.
pub const FORMAT: u32 = 1;

const EXT: &str = ".jsonl";
const FLUSH_EVERY: Duration = Duration::from_secs(1);

/// The bounds of the recordings folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub file_bytes: u64,
    pub total_bytes: u64,
    pub files: usize,
    /// Least time between two frames written.
    pub interval: Duration,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            file_bytes: MAX_FILE_BYTES,
            total_bytes: MAX_TOTAL_BYTES,
            files: MAX_FILES,
            interval: Duration::from_millis(1000 / RECORD_HZ as u64),
        }
    }
}

/// One recording, as a list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordingInfo {
    /// The file name (the webview names a recording by this, never by a path).
    pub name: String,
    pub bytes: u64,
    /// Unix ms of the first frame (else of the first line), when readable.
    pub started_ms: Option<u64>,
    /// From the first frame to the last, when readable.
    pub duration_ms: Option<u64>,
    /// The channel and schema key of the first line.
    pub channel: Option<String>,
    pub schema: Option<String>,
}

/// Whether `name` is one of ours: `[A-Za-z0-9_-]{1,80}.jsonl`. Anything else is never read
/// or removed (no separators, no `..`).
pub fn valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(EXT) else {
        return false;
    };
    (1..=80).contains(&stem.len())
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `YYYYMMDD-HHMMSS` (UTC) of a Unix time in ms.
pub fn stamp(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn channel_part(channel: &str) -> String {
    let s: String = channel
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect();
    if s.is_empty() { "game".into() } else { s }
}

/// Our files in `dir` as (name, bytes, modified), oldest first. Links and folders are skipped.
fn ours(dir: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    let mut v: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if !valid_name(&name) {
                return None;
            }
            let m = fs::symlink_metadata(e.path()).ok()?;
            m.is_file()
                .then(|| (name, m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)))
        })
        .collect();
    // Names start with the time: ties of the modified time go by name.
    v.sort_by(|a, b| (a.2, &a.0).cmp(&(b.2, &b.0)));
    v
}

/// Remove the oldest of our files (never `keep`) until at most `files` are left and they
/// take at most `bytes`. Returns the names removed.
pub fn prune(dir: &Path, files: usize, bytes: u64, keep: Option<&str>) -> Vec<String> {
    let mut list = ours(dir);
    let mut total: u64 = list.iter().map(|x| x.1).sum();
    let mut count = list.len();
    let mut gone = Vec::new();
    for (name, n, _) in list.drain(..) {
        if count <= files && total <= bytes {
            break;
        }
        if keep == Some(name.as_str()) {
            continue;
        }
        if fs::remove_file(dir.join(&name)).is_ok() {
            count -= 1;
            total = total.saturating_sub(n);
            gone.push(name);
        }
    }
    gone
}

/// `{t, starorg}` of a first line, as (unix ms, channel, schema).
fn header(line: &str) -> Option<(Option<u64>, Option<String>, Option<String>)> {
    let v: Value = serde_json::from_str(line).ok()?;
    let h = v.get("starorg")?;
    let s = |k: &str| h.get(k).and_then(Value::as_str).map(str::to_owned);
    Some((t_ms(&v), s("channel"), s("schema")))
}

/// `t` (unix seconds) of a record in ms.
pub(crate) fn t_ms(v: &Value) -> Option<u64> {
    let t = v.get("t")?.as_f64()?;
    (t.is_finite() && t >= 0.0).then(|| (t * 1000.0).round() as u64)
}

/// The `t` of the first and of the last frame line in the first and last 64 KB.
fn frame_times(f: &mut File, len: u64) -> (Option<u64>, Option<u64>) {
    const WINDOW: u64 = 64 * 1024;
    let read = |f: &mut File, from: u64| -> Option<String> {
        f.seek(SeekFrom::Start(from)).ok()?;
        let mut buf = Vec::new();
        f.take(WINDOW).read_to_end(&mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf).into_owned())
    };
    let frame_t = |line: &str| -> Option<u64> {
        let v: Value = serde_json::from_str(line).ok()?;
        v.get("frame").filter(|f| f.is_object())?;
        t_ms(&v)
    };
    let first = read(f, 0).and_then(|s| s.lines().find_map(frame_t));
    let last = read(f, len.saturating_sub(WINDOW)).and_then(|s| s.lines().rev().find_map(frame_t));
    (first, last)
}

/// What a list shows of one file (bytes, times, channel); unreadable parts are `None`.
pub fn info(dir: &Path, name: &str) -> io::Result<RecordingInfo> {
    if !valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a recording name",
        ));
    }
    let p = dir.join(name);
    let m = fs::symlink_metadata(&p)?;
    if !m.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "not a file"));
    }
    let mut f = File::open(&p)?;
    let mut head = String::new();
    let _ = (&mut f).take(4096).read_to_string(&mut head);
    let h = head.lines().next().and_then(header);
    let (first, last) = frame_times(&mut f, m.len());
    let started = first.or(h.as_ref().and_then(|h| h.0));
    Ok(RecordingInfo {
        name: name.to_owned(),
        bytes: m.len(),
        started_ms: started,
        duration_ms: first.zip(last).map(|(a, b)| b.saturating_sub(a)),
        channel: h.as_ref().and_then(|h| h.1.clone()),
        schema: h.and_then(|h| h.2),
    })
}

/// Every recording in `dir`, newest first (none when the folder is not there).
pub fn list(dir: &Path) -> Vec<RecordingInfo> {
    let mut v: Vec<RecordingInfo> = ours(dir)
        .into_iter()
        .rev()
        .filter_map(|(n, _, _)| info(dir, &n).ok())
        .collect();
    v.sort_by(|a, b| b.started_ms.cmp(&a.started_ms).then(b.name.cmp(&a.name)));
    v
}

/// Remove one recording by name.
pub fn delete(dir: &Path, name: &str) -> io::Result<()> {
    if !valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a recording name",
        ));
    }
    let p = dir.join(name);
    if !fs::symlink_metadata(&p)?.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "not a file"));
    }
    fs::remove_file(p)
}

/// The full path of a recording, checked (for [`crate::replay::FileReplay::open`]).
pub fn path_of(dir: &Path, name: &str) -> io::Result<PathBuf> {
    if !valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a recording name",
        ));
    }
    Ok(dir.join(name))
}

/// Why a recorder stopped writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stopped {
    /// The file reached its cap.
    Full,
    /// A write failed (disk full, folder gone).
    Failed,
}

/// Writes one recording. Dropping it flushes what is buffered.
#[derive(Debug)]
pub struct Recorder {
    name: String,
    out: BufWriter<File>,
    bytes: u64,
    limit: u64,
    interval: Duration,
    last_frame_ms: Option<u64>,
    last_flush: Instant,
    frames: u64,
    stopped: Option<Stopped>,
}

impl Recorder {
    /// Start a file in `dir` (made when missing) after pruning the oldest so this one fits.
    pub fn create(
        dir: &Path,
        channel: &str,
        schema: &str,
        now_ms: u64,
        caps: Caps,
    ) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let limit = caps.file_bytes.min(caps.total_bytes);
        prune(
            dir,
            caps.files.saturating_sub(1),
            caps.total_bytes.saturating_sub(limit),
            None,
        );
        let base = format!("{}-{}", stamp(now_ms), channel_part(channel));
        let mut n = 1;
        let (name, file) = loop {
            let name = if n == 1 {
                format!("{base}{EXT}")
            } else {
                format!("{base}-{n}{EXT}")
            };
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(&name))
            {
                Ok(f) => break (name, f),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && n < 100 => n += 1,
                Err(e) => return Err(e),
            }
        };
        let mut r = Self {
            name,
            out: BufWriter::new(file),
            bytes: 0,
            limit,
            interval: caps.interval,
            last_frame_ms: None,
            last_flush: Instant::now(),
            frames: 0,
            stopped: None,
        };
        let head = serde_json::json!({
            "t": now_ms as f64 / 1000.0,
            "starorg": {"format": FORMAT, "channel": channel, "schema": schema},
        });
        r.line(&head)?;
        r.out.flush()?;
        Ok(r)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn stopped(&self) -> Option<Stopped> {
        self.stopped
    }

    fn line(&mut self, v: &Value) -> io::Result<()> {
        let mut s = serde_json::to_string(v).map_err(io::Error::other)?;
        s.push('\n');
        self.out.write_all(s.as_bytes())?;
        self.bytes += s.len() as u64;
        Ok(())
    }

    /// Write a frame received at `t_ms` unless one was written less than an interval ago.
    /// Returns whether it was written; once full or failed it writes nothing more.
    pub fn frame(&mut self, t_ms: u64, raw: &Value) -> bool {
        if self.stopped.is_some()
            || self
                .last_frame_ms
                .is_some_and(|l| t_ms.saturating_sub(l) < self.interval.as_millis() as u64)
        {
            return false;
        }
        let rec = serde_json::json!({"t": t_ms as f64 / 1000.0, "frame": raw});
        let len = serde_json::to_string(&rec).map_or(u64::MAX, |s| s.len() as u64 + 1);
        if self.bytes.saturating_add(len) > self.limit {
            self.stopped = Some(Stopped::Full);
            let _ = self.out.flush();
            return false;
        }
        if self.line(&rec).is_err() {
            self.stopped = Some(Stopped::Failed);
            return false;
        }
        self.last_frame_ms = Some(t_ms);
        self.frames += 1;
        if self.last_flush.elapsed() >= FLUSH_EVERY {
            self.last_flush = Instant::now();
            if self.out.flush().is_err() {
                self.stopped = Some(Stopped::Failed);
            }
        }
        true
    }

    /// Flush and close the file.
    pub fn finish(mut self) -> io::Result<String> {
        self.out.flush()?;
        Ok(std::mem::take(&mut self.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn caps() -> Caps {
        Caps {
            interval: Duration::from_millis(250),
            ..Caps::default()
        }
    }

    #[test]
    fn names_are_checked() {
        assert!(valid_name("20261123-120000-PTU.jsonl"));
        assert!(valid_name("a_b-2.jsonl"));
        for bad in [
            "",
            ".jsonl",
            "../x.jsonl",
            "a/b.jsonl",
            "a\\b.jsonl",
            "x.json",
            "x.jsonl.tmp",
            "a b.jsonl",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
        assert!(!valid_name(&format!("{}.jsonl", "a".repeat(81))));
    }

    #[test]
    fn stamp_is_utc_civil_time() {
        assert_eq!(stamp(0), "19700101-000000");
        // 2026-11-23 12:34:56 UTC.
        assert_eq!(stamp(1_795_437_296_000), "20261123-123456");
        assert_eq!(stamp(951_782_400_000), "20000229-000000");
    }

    #[test]
    fn records_throttled_frames_and_lists_them() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("rec");
        let t0 = 1_795_437_296_000u64;
        let mut r = Recorder::create(&dir, "PTU", "sc-alpha-5.0.0-ptu-1", t0, caps()).unwrap();
        assert_eq!(r.name(), "20261123-123456-PTU.jsonl");
        // 10 Hz for 2 s: 4 Hz written.
        let wrote = (0..20u64)
            .filter(|i| r.frame(t0 + i * 100, &json!({"actor": {"n": i}})))
            .count();
        assert_eq!(wrote, 7);
        assert_eq!(r.frames(), 7);
        let name = r.finish().unwrap();
        let text = fs::read_to_string(dir.join(&name)).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 8);
        assert_eq!(lines[0]["starorg"]["channel"], json!("PTU"));
        assert_eq!(lines[1]["frame"]["actor"]["n"], json!(0));
        let l = list(&dir);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, name);
        assert_eq!(l[0].started_ms, Some(t0));
        assert_eq!(l[0].duration_ms, Some(1800));
        assert_eq!(l[0].channel.as_deref(), Some("PTU"));
        assert_eq!(l[0].schema.as_deref(), Some("sc-alpha-5.0.0-ptu-1"));
        assert_eq!(l[0].bytes, text.len() as u64);
    }

    #[test]
    fn a_file_stops_at_its_cap() {
        let t = tempfile::tempdir().unwrap();
        let c = Caps {
            file_bytes: 400,
            interval: Duration::ZERO,
            ..Caps::default()
        };
        let mut r = Recorder::create(t.path(), "LIVE", "k", 1000, c).unwrap();
        let mut n = 0;
        while r.frame(2000 + n, &json!({"pad": "x".repeat(40)})) {
            n += 1;
            assert!(n < 100);
        }
        assert_eq!(r.stopped(), Some(Stopped::Full));
        assert!(r.bytes() <= 400);
        assert!(!r.frame(99_999, &json!({})));
        let name = r.finish().unwrap();
        assert!(fs::metadata(t.path().join(name)).unwrap().len() <= 400);
    }

    #[test]
    fn a_new_file_prunes_the_oldest_and_never_foreign_files() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        for i in 0..4 {
            fs::write(d.join(format!("2026010{i}-000000-PTU.jsonl")), [b'x'; 100]).unwrap();
        }
        fs::write(d.join("notes.txt"), "mine").unwrap();
        fs::create_dir(d.join("sub.jsonl")).unwrap();
        let c = Caps { files: 3, ..caps() };
        let r = Recorder::create(d, "PTU", "k", 1_795_437_296_000, c).unwrap();
        let name = r.finish().unwrap();
        let left: Vec<String> = ours(d).into_iter().map(|x| x.0).collect();
        assert_eq!(left.len(), 3);
        assert!(left.contains(&name));
        assert!(!left.contains(&"20260100-000000-PTU.jsonl".to_owned()));
        assert!(!left.contains(&"20260101-000000-PTU.jsonl".to_owned()));
        assert!(d.join("notes.txt").is_file());
        assert!(d.join("sub.jsonl").is_dir());
        // Bytes: room for a full new file under the total.
        let gone = prune(d, 10, 150, Some(&name));
        assert_eq!(gone.len(), 2, "{gone:?}");
        assert!(d.join(&name).is_file());
    }

    #[test]
    fn same_second_gets_a_suffix_and_delete_checks_the_name() {
        let t = tempfile::tempdir().unwrap();
        let a = Recorder::create(t.path(), "P/TU", "k", 5000, caps()).unwrap();
        let b = Recorder::create(t.path(), "P/TU", "k", 5000, caps()).unwrap();
        assert_eq!(a.name(), "19700101-000005-PTU.jsonl");
        assert_eq!(b.name(), "19700101-000005-PTU-2.jsonl");
        let (a, b) = (a.finish().unwrap(), b.finish().unwrap());
        assert!(delete(t.path(), "../x.jsonl").is_err());
        assert!(delete(t.path(), "nope.jsonl").is_err());
        delete(t.path(), &a).unwrap();
        assert_eq!(list(t.path()).len(), 1);
        assert_eq!(list(t.path())[0].name, b);
        assert!(list(&t.path().join("missing")).is_empty());
    }

    #[test]
    fn info_survives_a_torn_last_line() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x.jsonl");
        fs::write(
            &p,
            "{\"t\": 1.0, \"frame\": {}}\n{\"t\": 3.5, \"frame\": {}}\n{\"t\": 9, \"fra",
        )
        .unwrap();
        let i = info(t.path(), "x.jsonl").unwrap();
        assert_eq!(i.started_ms, Some(1000));
        assert_eq!(i.duration_ms, Some(2500));
        assert_eq!(i.channel, None);
    }
}
