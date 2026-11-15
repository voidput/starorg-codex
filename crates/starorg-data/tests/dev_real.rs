//! Developer mode against a real install (addendum 7.11), read only. Ignored by default:
//!
//! ```sh
//! STARORG_GAME_DIR="/path/to/StarCitizen" STARORG_CHANNEL=LIVE \
//!   cargo test --release -p starorg-data --test dev_real -- --ignored --nocapture
//! ```
//!
//! Opens `Data.p4k` read only (shared; the game may run), indexes every record, searches,
//! opens records and exports every main record into memory (nothing is written). Prints
//! timings and the peak RSS.

use starorg_core::dev::explorer::TreeRoot;
use starorg_data::devworker::{self, DevInput, Session, WorkerInfo};
use starorg_data::starbreaker::DevDatabase;
use std::path::PathBuf;
use std::time::Instant;

fn game_dir() -> PathBuf {
    std::env::var_os("STARORG_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
            )
        })
}

fn peak_rss() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let l = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    Some(l.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
}

#[test]
#[ignore = "needs a game install"]
fn live_opens_indexes_and_every_main_record_exports() {
    let want = std::env::var("STARORG_CHANNEL").unwrap_or_else(|_| "LIVE".into());
    let ch = starorg_core::discovery::list_channels(&game_dir())
        .into_iter()
        .find(|c| c.name == want)
        .unwrap_or_else(|| panic!("no {want} under {}", game_dir().display()));
    let t = Instant::now();
    let loaded = devworker::load(&DevInput::P4k(ch.p4k.clone())).unwrap();
    let t_load = t.elapsed();
    let t = Instant::now();
    let db = DevDatabase::parse(&loaded.dcb).unwrap();
    let t_index = t.elapsed();
    let ix = db.index();
    println!(
        "{want}: DCB {} MB, {} records ({} main), {} types; load {t_load:?}, parse+index {t_index:?}, RSS now {:?} MB, peak {:?} MB",
        loaded.dcb.len() / 1_000_000,
        ix.len(),
        ix.main_count(),
        ix.types().len(),
        devworker::rss_bytes().map(|b| b / 1_000_000),
        peak_rss().map(|b| b / 1_000_000)
    );
    assert!(ix.len() >= 100_000, "{} records", ix.len());
    let t = Instant::now();
    let root = ix.tree(TreeRoot::Path, "", 0, 500, true);
    let types = ix.tree(TreeRoot::Type, "", 0, 500, true);
    let hits = ix.search("type:EntityClassDefinition behr", 500);
    println!(
        "path root {} folders, type root {} types, search {} hits ({:?} for all three)",
        root.total,
        types.total,
        hits.total,
        t.elapsed()
    );
    assert!(root.total > 100);
    let info = WorkerInfo {
        records: ix.len() as u32,
        main: ix.main_count() as u32,
        types: types.total,
        struct_defs: db.summary().struct_defs as u32,
        dcb_bytes: loaded.dcb.len() as u64,
        start_ms: 0,
        rss: None,
        source: loaded.source.clone(),
    };
    let mut s = Session::new(&db, loaded.ini.clone(), info);
    let first = hits.hits.first().map(|h| h.guid.clone()).unwrap();
    let t = Instant::now();
    let r = s.record(&first, "", None).unwrap();
    println!(
        "record {} ({} bytes JSON, {} loc keys) in {:?}",
        r.header.name,
        r.header.bytes,
        r.loc.len(),
        t.elapsed()
    );
    // Every main record exports (StarBreaker's own form, into memory).
    let t = Instant::now();
    let (mut n, mut bytes, mut failed) = (0usize, 0usize, Vec::new());
    for (i, e) in ix.entries().iter().enumerate() {
        if !e.main {
            continue;
        }
        let mut buf = Vec::new();
        match db.write_export(i, &mut buf, false) {
            Ok(()) => {
                n += 1;
                bytes += buf.len();
            }
            Err(err) => failed.push(format!("{}: {err}", e.name)),
        }
    }
    println!(
        "{n} main records exported, {} MB, in {:?}; peak RSS {:?} MB",
        bytes / 1_000_000,
        t.elapsed(),
        peak_rss().map(|b| b / 1_000_000)
    );
    assert!(
        failed.is_empty(),
        "{} failed: {:?}",
        failed.len(),
        &failed[..failed.len().min(5)]
    );
}
