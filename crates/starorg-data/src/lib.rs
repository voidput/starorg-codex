//! Game data extraction from `Data.p4k`. The only crate that depends on StarBreaker.
//!
//! - [`starbreaker`]: the adapter; the only module that names StarBreaker types.
//! - [`eocd`]: our own reader of the declared entry count (spec 3.3 step 2).
//! - [`extract`]: the extraction steps.
//! - [`gamedata`]: DataCore records -> `GameData`.
//! - [`worker`]: the `extract-worker` process body.

pub mod eocd;
pub mod extract;
pub mod gamedata;
pub mod starbreaker;
pub mod worker;
