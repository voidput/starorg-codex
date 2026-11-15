//! Core model, INI handling, discovery and the `GameSource` trait.
//!
//! This crate links no GTK, Tauri or HTTP client (spec 3.1). It reaches StarBreaker only
//! through [`source::GameSource`] implementations.

pub mod appdirs;
pub mod apply;
pub mod backup;
pub mod blueprints;
pub mod build;
pub mod cache;
pub mod config;
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
pub mod state;
pub mod usercfg;
pub mod validate;
