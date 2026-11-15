//! Rust port of `sc-gamedata`: Star Citizen game data domains read from `Data.p4k` and
//! written as the same JSON datasets (`game-<domain>.json`) the published dumps carry.
//!
//! Layers, each a port of one `sc-gamedata` module:
//!
//! - [`records`] (`records.ts`): records, record sets, reference following, accessors;
//! - [`entity`] (`entity.ts`): components of `EntityClassDefinition` records;
//! - [`localization`] (`localization.ts`): the string table, `localize`, overlays, and the
//!   published-dump redaction (off by default);
//! - [`cryxml`] (`cryxml.ts`): CryXmlB documents in raw archive files;
//! - [`glob`] (`glob.ts`), [`collate`] (`Intl.Collator`/`localeCompare` orders);
//! - [`output`]: `JSON.stringify(_, null, 2)`-exact writing and the dataset header;
//! - [`domain`] + [`domains`]: the `Domain` trait and the registry;
//! - [`pipeline`]: extraction scoping and running a domain;
//! - [`compare`]: the semantic diff the `gamedata-port` harness uses against golden dumps;
//! - [`source`]: the real archive via `starorg-data`'s StarBreaker adapter;
//! - [`fixtures`]: in-memory sources for tests.

pub mod collate;
pub mod compare;
pub mod cryxml;
pub mod domain;
pub mod domains;
pub mod entity;
pub mod fixtures;
pub mod glob;
pub mod helpers_items;
pub mod helpers_ships;
pub mod helpers_world;
pub mod localization;
pub mod output;
pub mod pipeline;
pub mod records;
pub mod source;
pub mod strings;
