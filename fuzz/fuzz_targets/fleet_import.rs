//! A fleet export (HangarXPLOR, FleetYards, FleetView, hangar.link, text): never panics.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = starorg_core::fleet::import::parse(data, None);
});
