//! A short history of the headline values for the screen's graphs (sc-telemetry `server.py`
//! `history` and `/api/history`): one sample a second per key metric, the last
//! [`HISTORY_LEN`] (10 minutes), in a ring that never grows.
//!
//! The host pushes every frame it hands to the UI; [`History::push`] keeps one per
//! [`HISTORY_INTERVAL`]. The UI asks for what is new since its last answer
//! ([`History::since`]), so a poll carries a few points, not the whole ring. A clear (another
//! channel, a replay, a seek) bumps the [`History::epoch`]: an answer for an older epoch is the
//! whole ring with `reset`.

use crate::frames::Snapshot;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::Duration;

/// One sample per this long.
pub const HISTORY_INTERVAL: Duration = Duration::from_secs(1);

/// Samples kept: 10 minutes at [`HISTORY_INTERVAL`].
pub const HISTORY_LEN: usize = 600;

/// The values graphed. Units as in [`Snapshot`]: health 0..100, oxygen and the ship's
/// fractions 0..1, speed m/s, G.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Sample {
    pub speed: Option<f64>,
    pub g: Option<f64>,
    pub hydrogen_fuel: Option<f64>,
    pub quantum_fuel: Option<f64>,
    pub hull: Option<f64>,
    pub shield: Option<f64>,
    pub health: Option<f64>,
    pub oxygen: Option<f64>,
}

impl Sample {
    pub fn of(s: &Snapshot) -> Self {
        let ship = s.ship.as_ref();
        Self {
            speed: ship.and_then(|x| x.speed),
            g: ship.and_then(|x| x.g),
            hydrogen_fuel: ship.and_then(|x| x.hydrogen_fuel),
            quantum_fuel: ship.and_then(|x| x.quantum_fuel),
            hull: ship.and_then(|x| x.hull),
            shield: ship.and_then(|x| x.shield),
            health: s.vitals.health,
            oxygen: s.vitals.oxygen,
        }
    }
}

/// The samples by series, in the order of `t_ms` (columns keep the answer small).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Series {
    pub speed: Vec<Option<f64>>,
    pub g: Vec<Option<f64>>,
    pub hydrogen_fuel: Vec<Option<f64>>,
    pub quantum_fuel: Vec<Option<f64>>,
    pub hull: Vec<Option<f64>>,
    pub shield: Vec<Option<f64>>,
    pub health: Vec<Option<f64>>,
    pub oxygen: Vec<Option<f64>>,
}

impl Series {
    fn push(&mut self, s: &Sample) {
        self.speed.push(s.speed);
        self.g.push(s.g);
        self.hydrogen_fuel.push(s.hydrogen_fuel);
        self.quantum_fuel.push(s.quantum_fuel);
        self.hull.push(s.hull);
        self.shield.push(s.shield);
        self.health.push(s.health);
        self.oxygen.push(s.oxygen);
    }
}

/// An answer of [`History::since`].
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HistoryView {
    pub epoch: u64,
    /// The asker's points are gone (another epoch): this is the whole ring.
    pub reset: bool,
    pub interval_ms: u64,
    pub capacity: u32,
    /// Unix ms of each sample (a replay: the time it was recorded).
    pub t_ms: Vec<u64>,
    pub series: Series,
}

/// The ring.
#[derive(Debug, Clone)]
pub struct History {
    epoch: u64,
    interval_ms: u64,
    cap: usize,
    points: VecDeque<(u64, Sample)>,
}

impl Default for History {
    fn default() -> Self {
        Self::new(HISTORY_LEN, HISTORY_INTERVAL)
    }
}

impl History {
    pub fn new(cap: usize, interval: Duration) -> Self {
        let cap = cap.max(1);
        Self {
            epoch: 1,
            interval_ms: (interval.as_millis() as u64).max(1),
            cap,
            points: VecDeque::with_capacity(cap),
        }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Drop every sample; the next answer to an older epoch says `reset`.
    pub fn clear(&mut self) {
        self.points.clear();
        self.epoch += 1;
    }

    /// Offer a frame's snapshot at `t_ms`; kept when at least one interval after the last one
    /// kept. A time before the last one (a seek back, the clock set back) starts over.
    pub fn push(&mut self, t_ms: u64, s: &Snapshot) -> bool {
        if let Some(&(last, _)) = self.points.back() {
            if t_ms < last {
                self.clear();
            } else if t_ms - last < self.interval_ms {
                return false;
            }
        }
        if self.points.len() == self.cap {
            self.points.pop_front();
        }
        self.points.push_back((t_ms, Sample::of(s)));
        true
    }

    /// The samples after `since_ms` when `epoch` is the current one, else all (`reset`).
    pub fn since(&self, epoch: Option<u64>, since_ms: Option<u64>) -> HistoryView {
        let reset = epoch != Some(self.epoch);
        let after = if reset { None } else { since_ms };
        let mut v = HistoryView {
            epoch: self.epoch,
            reset,
            interval_ms: self.interval_ms,
            capacity: self.cap as u32,
            ..Default::default()
        };
        for (t, s) in &self.points {
            if after.is_some_and(|a| *t <= a) {
                continue;
            }
            v.t_ms.push(*t);
            v.series.push(s);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{Ship, Vitals};

    fn snap(speed: f64) -> Snapshot {
        Snapshot {
            vitals: Vitals {
                health: Some(90.0),
                ..Default::default()
            },
            ship: Some(Ship {
                speed: Some(speed),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn one_sample_a_second_and_bounded() {
        let mut h = History::new(5, Duration::from_secs(1));
        // 10 Hz for 10 s: 10 admitted, 5 kept.
        let kept = (0..100u64)
            .filter(|i| h.push(1_000_000 + i * 100, &snap(*i as f64)))
            .count();
        assert_eq!(kept, 10);
        assert_eq!(h.len(), 5);
        let v = h.since(None, None);
        assert!(v.reset);
        assert_eq!(
            v.t_ms,
            [1_005_000, 1_006_000, 1_007_000, 1_008_000, 1_009_000]
        );
        assert_eq!(v.series.speed[0], Some(50.0));
        assert_eq!(v.series.health, [Some(90.0); 5]);
        assert_eq!(v.series.hull, [None; 5]);
    }

    #[test]
    fn since_is_incremental_within_an_epoch() {
        let mut h = History::default();
        for i in 0..3u64 {
            h.push(i * 1000, &snap(1.0));
        }
        let all = h.since(None, None);
        let e = all.epoch;
        let none = h.since(Some(e), Some(2000));
        assert!(!none.reset && none.t_ms.is_empty());
        h.push(3000, &snap(2.0));
        let one = h.since(Some(e), Some(2000));
        assert_eq!(one.t_ms, [3000]);
        assert_eq!(one.series.speed, [Some(2.0)]);
        // A stale epoch gets everything again.
        assert_eq!(h.since(Some(e - 1), Some(2000)).t_ms.len(), 4);
    }

    #[test]
    fn going_back_in_time_starts_over() {
        let mut h = History::default();
        h.push(10_000, &snap(1.0));
        h.push(11_000, &snap(1.0));
        let e = h.epoch();
        assert!(h.push(5_000, &snap(3.0)));
        assert_eq!(h.len(), 1);
        assert_eq!(h.epoch(), e + 1);
        let v = h.since(Some(e), Some(11_000));
        assert!(v.reset);
        assert_eq!(v.t_ms, [5_000]);
        h.clear();
        assert!(h.is_empty());
    }

    #[test]
    fn json_is_columns() {
        let mut h = History::default();
        h.push(1, &Snapshot::default());
        let j = serde_json::to_value(h.since(None, None)).unwrap();
        assert_eq!(j["series"]["speed"], serde_json::json!([null]));
        assert_eq!(j["interval_ms"], serde_json::json!(1000));
        assert_eq!(j["capacity"], serde_json::json!(600));
    }
}
