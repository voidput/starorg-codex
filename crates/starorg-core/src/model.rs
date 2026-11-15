//! `GameData`: the normalised model generators read (spec 3.4).
//!
//! M0 holds the build identity and a DataCore summary only. M2 adds items, ships, contracts
//! and the rest; every change of meaning bumps [`GAMEDATA_SCHEMA`], which is part of the
//! cache key.

use crate::build::BuildInfo;
use serde::{Deserialize, Serialize};

/// Layout version of [`GameData`].
pub const GAMEDATA_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameData {
    pub schema: u32,
    /// Build identity of the channel the data came from.
    pub build: Option<BuildInfo>,
    /// Counts from the parsed DataCore, proving it was read.
    pub datacore: Option<DataCoreStats>,
}

impl Default for GameData {
    fn default() -> Self {
        Self {
            schema: GAMEDATA_SCHEMA,
            build: None,
            datacore: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataCoreStats {
    pub records: u64,
    pub struct_defs: u64,
}
