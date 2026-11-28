//! The app's Commodities screen, Mining tab (`mining_list`, `mining_detail`): every mineable
//! resource of the build with its places, quality per place, rock physics and the laser
//! estimate, typed in [`starorg_gamedata::query::mining`]. [`crate::tools::Tools`] builds the
//! index once per build (and again once `game-mining` or `game-weapons` is read, which
//! "Read game data" extracts with [`crate::tools::Tools::warm_mining`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use starorg_gamedata::query::commodities::{CommodityDataState, CommodityRef};
pub use starorg_gamedata::query::mined::MinedSpot;
pub use starorg_gamedata::query::mining::*;
pub use starorg_gamedata::query::quality::{QualityRange, QualityStep};

/// `mining_list`: every mineral, by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiningDirectory {
    pub state: CommodityDataState,
    /// The build the data is of (`LIVE 4.10.201.48085`).
    pub build: Option<String>,
    pub minerals: Vec<MineralSummary>,
    /// `game-mining` (resistance, instability, charge window) was read.
    pub physics: bool,
    /// `game-weapons` (mining laser power) was read.
    pub lasers_read: bool,
    /// The S1 and S2 mining lasers' power range.
    pub lasers: Vec<LaserClass>,
    /// `game-mining-spawns` was read (else no body or belt is known yet).
    pub spawns_read: bool,
    /// `game-item-sources` was read (else no hand-placed spot is known yet).
    pub spots_read: bool,
}

/// `mining_detail`: one mineral, `None` when the id names none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiningInfo {
    pub state: CommodityDataState,
    pub mineral: Option<MineralDetail>,
}

/// The list without a channel or before the game data is read.
pub fn empty_directory(state: CommodityDataState) -> MiningDirectory {
    MiningDirectory {
        state,
        build: None,
        minerals: Vec::new(),
        physics: false,
        lasers_read: false,
        lasers: Vec::new(),
        spawns_read: false,
        spots_read: false,
    }
}

/// The list of a built index.
pub fn directory(ix: &MiningIndex, build: Option<String>) -> MiningDirectory {
    MiningDirectory {
        state: CommodityDataState::Ready,
        build,
        minerals: ix.summaries(),
        physics: ix.physics,
        lasers_read: ix.lasers_read,
        lasers: ix.lasers.clone(),
        spawns_read: ix.spawns_read,
        spots_read: ix.spots_read,
    }
}

/// The rows of `v[key]` that parse; the rest are left out.
pub fn rows<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Vec<T> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}
