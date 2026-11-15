//! Core model, INI handling, discovery and the `GameSource` trait.
//!
//! This crate links no GTK, Tauri or HTTP client (spec 3.1). It reaches StarBreaker only
//! through [`source::GameSource`] implementations.

pub mod build;
pub mod discovery;
pub mod enhance;
pub mod ini;
pub mod model;
pub mod paths;
pub mod source;
