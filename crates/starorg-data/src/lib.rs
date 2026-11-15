//! Game data extraction from `Data.p4k`. The only crate that depends on StarBreaker.
//!
//! - [`starbreaker`]: the adapter; the only module that names StarBreaker types, apart from
//!   [`assets`] (Developer mode asset export, which drives StarBreaker's model, texture and
//!   audio crates directly).
//! - [`eocd`]: our own reader of the declared entry count (spec 3.3 step 2).
//! - [`extract`]: the extraction steps.
//! - [`gamedata`]: DataCore records -> `GameData`.
//! - [`worker`]: the `extract-worker` process body.
//! - [`devworker`]: the `dev-worker` process body (Developer mode explorer, addendum 7).
//! - [`assetworker`]: the `asset-worker` process body (Developer mode assets, addendum 7.16).

pub mod assets;
pub mod assetworker;
pub mod devworker;
pub mod eocd;
pub mod extract;
pub mod gamedata;
pub mod keyrefs;
pub mod starbreaker;
pub mod worker;
