//! Core model, INI handling, discovery and the `GameSource` trait.
//!
//! This crate links no GTK, Tauri or HTTP client (spec 3.1). It reaches StarBreaker only
//! through [`source::GameSource`] implementations.

pub mod appdirs;
pub mod apply;
pub mod assetcache;
pub mod background;
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
pub mod gz;
pub mod hooks;
pub mod ini;
pub mod legacy;
pub mod lock;
pub mod memlimit;
pub mod merge;
pub mod model;
pub mod patch;
pub mod paths;
pub mod running;
pub mod selfexe;
pub mod source;
pub mod starorg;
pub mod state;
pub mod storage;
pub mod telemetry;
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
///
/// History: 1 = 0.1.0; 2 = no stats in shared `LOC_*` sentinels or placeholder text;
/// 3 = same-named blueprints: ownership per blueprint, no false `[Owned]`/`[BP Owned]`;
/// 4 = `[Owned]` coloured by default (EM4, the game's blue);
/// 5 = quest items: `Used in:` hints from real mission references and the built-in
/// `quest-items` patch (Vanduul-Tech Smugglers hints);
/// 6 = obtain hints: WHERE TO GET blocks (on in Recommended), extending stat blocks;
/// 7 = missing names: added keys for entity names stock lacks (curated names from the
/// built-in `missing-names` patch, a tidier readable fallback), and generated text never
/// brings a raw `@key` or a placeholder;
/// 8 = commodity sell prices: the WHERE TO GET `sell` line (on in Everything);
/// 9 = missing descriptions: a sibling's stock description (colour paragraph cut) for
/// description keys stock lacks or holds as a placeholder, with stats and WHERE TO GET on it;
/// 10 = ore quality in WHERE TO GET for Aluminum and Stileron ore (the spawn data spells them
/// `aluminium_ore` and `sileron_ore`, which matched no resource before);
/// 11 = per-variant MISSION DETAILS: values per difficulty/rank/standing/system when a shared
/// description's variants differ, the reputation faction, `Systems:` and `Shared text:` (shown
/// on N contracts, labelled by the kind that split a line).
/// 12 = SPAWNS in mission descriptions: the ships a contract spawns (salvage targets,
/// hostiles, ships to protect), from its spawn groups and the vehicles their tags match.
pub const OUTPUT_VERSION: u32 = 12;
