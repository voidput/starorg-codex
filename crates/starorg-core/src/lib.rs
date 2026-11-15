//! Core model, INI handling, discovery and the `GameSource` trait.
//!
//! This crate links no GTK, Tauri or HTTP client (spec 3.1). It reaches StarBreaker only
//! through [`source::GameSource`] implementations.

pub mod appdirs;
pub mod apply;
pub mod assetcache;
pub mod backup;
pub mod blueprints;
pub mod build;
pub mod cache;
pub mod canon;
pub mod config;
pub mod dev;
pub mod discovery;
pub mod enhance;
pub mod fleet;
pub mod fsutil;
pub mod groups;
pub mod hooks;
pub mod ini;
pub mod lock;
pub mod merge;
pub mod model;
pub mod patch;
pub mod paths;
pub mod running;
pub mod source;
pub mod starorg;
pub mod state;
pub mod usercfg;
pub mod validate;

/// Version of the generated text. `applied.json` records it; status reports
/// `Stale (enhancer update)` when the recorded one differs, so users re-apply after an
/// update that changes what is written.
///
/// **Bump only when the generated text changes for the same inputs** (same game data,
/// settings, patches, overrides, owned blueprints, ship names): a new or changed enhancement,
/// a changed default of a setting, a fix in the text or the merge. Do not bump for app
/// releases that leave the output as it was; the app version is recorded for information
/// only. Manifests written before this field existed (0.1.0) read as version 1.
pub const OUTPUT_VERSION: u32 = 2;
