//! Smart Citizen's `owned-blueprints.json`: never panics.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = starorg_core::blueprints::import::parse_export(data);
});
