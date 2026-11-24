//! Replaying a recording into the same screen (sc-telemetry `server.py` `replay_records` and
//! `run_replay`): the frames of a [`crate::record`] file, in order, with their recorded times,
//! read line by line from an index of offsets so a file is never loaded whole and a seek is
//! one read.
//!
//! [`ReplaySource`] is what the app's player steps through: [`FileReplay`] for a file, and
//! [`SyntheticReplay`] (the mock generator over a fixed length) for the mock backend and tests.
//! Lines that are not a frame record (the first line, sc-telemetry's `session`/`signals`
//! records, torn or foreign lines, a NaN time) are skipped, as sc-telemetry skips them.

use crate::frames::{Frame, Snapshot};
use crate::schema::Schema;
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::Value;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

/// Longest pause between two frames while playing: a longer gap in the file is cut to this
/// (sc-telemetry caps it at 5 s too).
pub const MAX_GAP_MS: u64 = 5_000;
/// Slowest and fastest replay.
pub const MIN_SPEED: f64 = 0.25;
pub const MAX_SPEED: f64 = 16.0;
/// Frame lines indexed at most (a capped file at [`crate::record::RECORD_HZ`] holds fewer).
pub const MAX_FRAMES: usize = 2_000_000;
/// A line longer than this is skipped, not read.
pub const MAX_LINE: usize = 4 * 1024 * 1024;

/// Clamp a speed to [`MIN_SPEED`]..[`MAX_SPEED`] (NaN is 1).
pub fn clamp_speed(s: f64) -> f64 {
    if s.is_nan() {
        1.0
    } else {
        s.clamp(MIN_SPEED, MAX_SPEED)
    }
}

/// What a player steps through: frames `0..len()` in time order.
pub trait ReplaySource: Send {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Unix ms of frame `i` (recorded time), never decreasing with `i`.
    fn t_ms(&self, i: usize) -> u64;
    /// Frame `i` as JSON, `None` when it cannot be read now.
    fn frame(&mut self, i: usize) -> Option<Value>;

    fn start_ms(&self) -> u64 {
        if self.is_empty() { 0 } else { self.t_ms(0) }
    }

    fn duration_ms(&self) -> u64 {
        if self.is_empty() {
            0
        } else {
            self.t_ms(self.len() - 1).saturating_sub(self.t_ms(0))
        }
    }

    /// The first frame at or after `pos_ms` from the start (the last one past the end).
    fn index_at(&self, pos_ms: u64) -> usize {
        let n = self.len();
        if n == 0 {
            return 0;
        }
        let want = self.start_ms().saturating_add(pos_ms);
        let (mut lo, mut hi) = (0usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.t_ms(mid) < want {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.min(n - 1)
    }

    /// Frame `i` as the UI gets it: `seq` is its index + 1, `received_ms` its recorded time.
    fn ui_frame(&mut self, i: usize) -> Option<Frame> {
        let raw = self.frame(i)?;
        Some(Frame {
            seq: i as u64 + 1,
            received_ms: self.t_ms(i),
            snapshot: Snapshot::from_json(&raw),
            raw,
        })
    }
}

#[derive(Deserialize)]
struct Probe {
    t: f64,
    frame: Option<IsObject>,
}

/// Whether a value is an object, read without keeping any of it.
struct IsObject(bool);

impl<'de> Deserialize<'de> for IsObject {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = IsObject;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut m: A,
            ) -> Result<IsObject, A::Error> {
                while m.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(IsObject(true))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut s: A,
            ) -> Result<IsObject, A::Error> {
                while s.next_element::<IgnoredAny>()?.is_some() {}
                Ok(IsObject(false))
            }
            fn visit_bool<E>(self, _: bool) -> Result<IsObject, E> {
                Ok(IsObject(false))
            }
            fn visit_i64<E>(self, _: i64) -> Result<IsObject, E> {
                Ok(IsObject(false))
            }
            fn visit_u64<E>(self, _: u64) -> Result<IsObject, E> {
                Ok(IsObject(false))
            }
            fn visit_f64<E>(self, _: f64) -> Result<IsObject, E> {
                Ok(IsObject(false))
            }
            fn visit_str<E>(self, _: &str) -> Result<IsObject, E> {
                Ok(IsObject(false))
            }
            fn visit_unit<E>(self) -> Result<IsObject, E> {
                Ok(IsObject(false))
            }
        }
        d.deserialize_any(V)
    }
}

#[derive(Deserialize)]
struct FrameRec {
    frame: Value,
}

/// A recording on disk: (time, offset, length) of each frame line.
#[derive(Debug)]
pub struct FileReplay {
    file: File,
    index: Vec<(u64, u64, u32)>,
}

impl FileReplay {
    /// Index `path` (one pass; every line is checked, none kept).
    pub fn open(path: &Path) -> io::Result<Self> {
        if !std::fs::symlink_metadata(path)?.is_file() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "not a file"));
        }
        let file = File::open(path)?;
        let mut r = BufReader::new(file.try_clone()?);
        let mut index = Vec::new();
        let mut off = 0u64;
        let mut line = Vec::new();
        let mut last_t = 0u64;
        loop {
            line.clear();
            let n = (&mut r)
                .take(MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            let start = off;
            off += n as u64;
            if n > MAX_LINE {
                // Skip the rest of an over-long line.
                let mut sink = Vec::new();
                off += r.read_until(b'\n', &mut sink)? as u64;
                continue;
            }
            let Ok(p) = serde_json::from_slice::<Probe>(&line) else {
                continue;
            };
            if !p.frame.is_some_and(|f| f.0) || !p.t.is_finite() || p.t < 0.0 {
                continue;
            }
            // Out-of-order times are held at the last one, so time never runs back.
            let t = ((p.t * 1000.0).round() as u64).max(last_t);
            last_t = t;
            index.push((t, start, n as u32));
            if index.len() >= MAX_FRAMES {
                break;
            }
        }
        Ok(Self { file, index })
    }
}

impl ReplaySource for FileReplay {
    fn len(&self) -> usize {
        self.index.len()
    }

    fn t_ms(&self, i: usize) -> u64 {
        self.index.get(i).map_or(0, |e| e.0)
    }

    fn frame(&mut self, i: usize) -> Option<Value> {
        let &(_, off, len) = self.index.get(i)?;
        self.file.seek(SeekFrom::Start(off)).ok()?;
        let mut buf = vec![0u8; len as usize];
        self.file.read_exact(&mut buf).ok()?;
        serde_json::from_slice::<FrameRec>(&buf)
            .ok()
            .map(|r| r.frame)
    }
}

/// The mock generator as a recording: `len` frames [`crate::record::RECORD_HZ`] a second
/// from `start_ms`. No file, no CIG data.
pub struct SyntheticReplay {
    schema: Arc<Schema>,
    start_ms: u64,
    step_ms: u64,
    len: usize,
}

impl SyntheticReplay {
    pub fn new(start_ms: u64, duration_ms: u64) -> Self {
        let step_ms = 1000 / crate::record::RECORD_HZ as u64;
        Self {
            schema: Arc::new(crate::mock::schema()),
            start_ms,
            step_ms,
            len: (duration_ms / step_ms) as usize + 1,
        }
    }
}

impl ReplaySource for SyntheticReplay {
    fn len(&self) -> usize {
        self.len
    }

    fn t_ms(&self, i: usize) -> u64 {
        self.start_ms + i as u64 * self.step_ms
    }

    fn frame(&mut self, i: usize) -> Option<Value> {
        (i < self.len).then(|| {
            let t = (i as u64 * self.step_ms) as f64 / 1000.0;
            crate::json::to_json(&crate::mock::frame(
                &self.schema,
                t,
                &crate::mock::MockState::default(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Caps, Recorder};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn a_recording_replays_its_frames_in_order() {
        let t = tempfile::tempdir().unwrap();
        let caps = Caps {
            interval: Duration::ZERO,
            ..Caps::default()
        };
        let mut r = Recorder::create(t.path(), "PTU", "k", 10_000, caps).unwrap();
        for i in 0..5u64 {
            let raw =
                json!({"vehicle": {"state": {"flight": {"instruments": {"speed": i as f64}}}}});
            assert!(r.frame(10_000 + i * 250, &raw));
        }
        let name = r.finish().unwrap();
        let mut p = FileReplay::open(&t.path().join(name)).unwrap();
        assert_eq!(p.len(), 5);
        assert_eq!(p.duration_ms(), 1000);
        assert_eq!(p.start_ms(), 10_000);
        let f = p.ui_frame(3).unwrap();
        assert_eq!(f.seq, 4);
        assert_eq!(f.received_ms, 10_750);
        assert_eq!(f.snapshot.ship.unwrap().speed, Some(3.0));
        assert_eq!(p.index_at(0), 0);
        assert_eq!(p.index_at(500), 2);
        assert_eq!(p.index_at(501), 3);
        assert_eq!(p.index_at(60_000), 4);
    }

    #[test]
    fn junk_and_other_records_are_skipped() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x.jsonl");
        let text = [
            r#"{"t": 1.0, "starorg": {"format": 1}}"#,
            r#"{"t": 1.0, "session": {"log_found": true}}"#,
            r#"{"t": 2.0, "frame": {"a": 1}}"#,
            r#"{"t": NaN, "frame": {}}"#,
            r#"{"t": 2.5, "frame": [1, 2]}"#,
            r#"{"t": "3", "frame": {}}"#,
            "not json",
            "",
            r#"{"t": 1.5, "frame": {"a": 2}}"#,
            r#"{"t": 4.0, "frame": {"a": 3}"#,
        ]
        .join("\n");
        std::fs::write(&p, text).unwrap();
        let mut r = FileReplay::open(&p).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r.frame(0), Some(json!({"a": 1})));
        assert_eq!(r.frame(1), Some(json!({"a": 2})));
        assert_eq!(r.t_ms(1), 2000, "time held, never back");
        assert_eq!(r.frame(2), None);
    }

    #[test]
    fn crlf_lines_and_a_folder_are_handled() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("w.jsonl");
        std::fs::write(
            &p,
            "{\"t\": 1, \"frame\": {}}\r\n{\"t\": 2, \"frame\": {\"b\": 1}}\r\n",
        )
        .unwrap();
        let mut r = FileReplay::open(&p).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r.frame(1), Some(json!({"b": 1})));
        assert!(FileReplay::open(t.path()).is_err());
    }

    #[test]
    fn synthetic_replay_is_the_mock() {
        let mut s = SyntheticReplay::new(1_000, 10_000);
        assert_eq!(s.len(), 41);
        assert_eq!(s.duration_ms(), 10_000);
        assert_eq!(s.index_at(2_500), 10);
        let f = s.ui_frame(10).unwrap();
        assert_eq!(f.received_ms, 3_500);
        assert!(f.snapshot.ship.is_some());
        assert!(s.frame(41).is_none());
    }

    #[test]
    fn speed_is_clamped() {
        assert_eq!(clamp_speed(f64::NAN), 1.0);
        assert_eq!(clamp_speed(0.0), MIN_SPEED);
        assert_eq!(clamp_speed(100.0), MAX_SPEED);
        assert_eq!(clamp_speed(2.0), 2.0);
    }
}
