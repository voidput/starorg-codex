//! Where `GameData` and the stock `global.ini` come from (spec 3.4).
//!
//! One source feeds apply: [`local::LocalExtraction`] (the worker process in spec 3.3),
//! from the channel's own `Data.p4k`. [`dump`] holds the published LIVE builds of
//! `voidput/sc-gamedata-dumps`, which since 2026-11-17 serve only Developer mode's Diff
//! (addendum §1) and are never turned into `GameData`.

pub mod dump;
pub mod local;

use crate::discovery::Channel;
use crate::model::GameData;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Time budget for a load. The parent kills a worker that overruns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Budget {
    deadline: Option<Instant>,
}

impl Budget {
    pub fn unlimited() -> Self {
        Self { deadline: None }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            deadline: Some(Instant::now() + timeout),
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Time left, or `None` when unlimited. Zero once the deadline passed.
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
    }

    pub fn expired(&self) -> bool {
        self.remaining() == Some(Duration::ZERO)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Origin {
    /// Extracted from the channel's `Data.p4k` by the worker process.
    LocalExtraction { starbreaker_commit: String },
    /// Downloaded datasets (addendum §1): `repo@tag (commit)`. No longer produced since
    /// dumps were rescoped to Diff only (2026-11-17); kept so older records still read.
    PublishedDump { dataset: String },
}

/// A non-fatal finding. Warnings make an apply "applied with warnings" (exit 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub kind: String,
    pub detail: String,
}

impl Warning {
    pub fn new(kind: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            detail: detail.into(),
        }
    }
}

/// Everything a build needs from the game.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// Stock `Data\Localization\english\global.ini`, byte for byte (BOM, CRLF).
    pub base_ini: Vec<u8>,
    pub game_data: GameData,
    /// Loc-reference pass for Strings grouping; only app-started local extractions have it.
    pub keyrefs: Option<crate::groups::KeyRefs>,
    pub origin: Origin,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// The budget ran out; nothing was produced (exit 3).
    #[error("out of time after {0:?}")]
    Timeout(Duration),
    /// The worker exited unsuccessfully, crashed or was killed.
    #[error("extraction worker failed: {0}")]
    Worker(String),
    /// The archive failed a safety check (entry count, size, CRC).
    #[error("archive check failed: {0}")]
    Check(String),
    /// This source cannot serve the channel (e.g. no dump for this build).
    #[error("source unavailable: {0}")]
    Unavailable(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// A provider of stock text and `GameData` for one channel.
pub trait GameSource {
    fn load(&self, channel: &Channel, budget: &Budget) -> Result<Loaded, SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget() {
        assert_eq!(Budget::unlimited().remaining(), None);
        assert!(!Budget::unlimited().expired());
        assert!(Budget::with_timeout(Duration::ZERO).expired());
        assert!(!Budget::with_timeout(Duration::from_secs(60)).expired());
    }

    /// The trait is object safe, so the GUI and CLI can pick a source at run time.
    #[test]
    fn trait_is_object_safe() {
        struct Fixed;
        impl GameSource for Fixed {
            fn load(&self, _: &Channel, _: &Budget) -> Result<Loaded, SourceError> {
                Err(SourceError::Unavailable("test".into()))
            }
        }
        let sources: Vec<Box<dyn GameSource>> = vec![Box::new(Fixed)];
        assert_eq!(sources.len(), 1);
    }
}
