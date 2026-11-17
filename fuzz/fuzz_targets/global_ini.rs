//! A `global.ini` (stock or another tool's): parsing never panics, and rendering with no edits
//! gives the input back byte for byte.
#![no_main]
use libfuzzer_sys::fuzz_target;
use starorg_core::ini::{Edits, StockIni};

fuzz_target!(|data: &[u8]| {
    let ini = StockIni::parse(data.to_vec());
    assert_eq!(ini.render(&Edits::new()), data);
    for (i, _, _) in ini.entries() {
        let _ = ini.value_at(i);
    }
});
