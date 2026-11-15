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
        sections: None,
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

/// Sections and quick filters (addendum 7.17) on the real archive: every record in one
/// section, the counts, the cost of the archive-name sets and of the index, and a few list
/// queries. Read only.
#[test]
#[ignore = "needs a game install"]
fn live_sections_index_counts_and_cost() {
    use starorg_core::dev::sections::{Filters, ListGroup, ListQuery, SortKey};
    let want = std::env::var("STARORG_CHANNEL").unwrap_or_else(|_| "LIVE".into());
    let ch = starorg_core::discovery::list_channels(&game_dir())
        .into_iter()
        .find(|c| c.name == want)
        .unwrap_or_else(|| panic!("no {want}"));
    let t = Instant::now();
    let (archive, _) = starorg_data::extract::open_checked(&ch.p4k).unwrap();
    let t_open = t.elapsed();
    let t = Instant::now();
    let media = devworker::media_names(&archive);
    let t_media = t.elapsed();
    drop(archive);
    let loaded = devworker::load(&DevInput::Dcb {
        dcb: PathBuf::from("/nonexistent"),
        ini: None,
    });
    assert!(loaded.is_err());
    let loaded = devworker::load(&DevInput::P4k(ch.p4k.clone())).unwrap();
    let db = DevDatabase::parse(&loaded.dcb).unwrap();
    let info = WorkerInfo {
        records: db.index().len() as u32,
        main: db.index().main_count() as u32,
        types: 0,
        struct_defs: 0,
        dcb_bytes: 0,
        start_ms: 0,
        rss: None,
        source: String::new(),
        sections: None,
    };
    let mut s = Session::new(&db, loaded.ini.clone(), info);
    let rss0 = devworker::rss_bytes().unwrap_or(0);
    let stats = s.index_sections(Some(&media));
    let rss1 = devworker::rss_bytes().unwrap_or(0);
    println!(
        "open {t_open:?}; media names {t_media:?} ({} geometry files, {} triggers, ~{} KB)",
        media.geometry.len(),
        media.triggers.len(),
        media.bytes() / 1024
    );
    println!(
        "sections: scan {} ms, build {} ms, ~{} KB held; RSS {} -> {} MB; {} entities, {} with a model, {} with sounds",
        stats.scan_ms,
        stats.build_ms,
        stats.bytes / 1024,
        rss0 / 1_000_000,
        rss1 / 1_000_000,
        stats.entities,
        stats.models,
        stats.sounds
    );
    let ix = db.index();
    let counts = s.sections.counts();
    let total: u32 = counts.iter().map(|c| c.1).sum();
    assert_eq!(
        total as usize,
        ix.len(),
        "every record in exactly one section"
    );
    let mains = s.sections.tree(ix, "", 0, 500, true, false);
    let players = s.sections.tree(ix, "", 0, 500, true, true);
    for n in &mains.nodes {
        let p = players
            .nodes
            .iter()
            .find(|x| x.id == n.id)
            .map_or(0, |x| x.count);
        println!(
            "  {:<28} {:>7} main  {:>7} player-usable",
            n.label, n.count, p
        );
    }
    for (id, c) in &counts {
        println!("  all records {id:<14} {c:>7}");
    }
    let q = |query: &str, f: Filters, group: ListGroup| ListQuery {
        query: query.into(),
        filters: f,
        group,
        limit: 500,
        main_only: true,
        sort: SortKey::Name,
        ..Default::default()
    };
    for (label, lq) in [
        (
            "all, grouped by section",
            q("", Filters::default(), ListGroup::Section),
        ),
        ("gladius", q("gladius", Filters::default(), ListGroup::None)),
        (
            "type:cooler size:1 maker:aegis",
            q(
                "type:cooler size:1 maker:aegis",
                Filters::default(),
                ListGroup::None,
            ),
        ),
        (
            "quantum (fuzzy, grouped by kind)",
            q("quantum", Filters::default(), ListGroup::Kind),
        ),
        (
            "has:model player only",
            q(
                "has:model",
                Filters {
                    player_only: true,
                    ..Default::default()
                },
                ListGroup::None,
            ),
        ),
    ] {
        let t = Instant::now();
        let p = s.list(&lq);
        println!(
            "list {label}: {} rows, {} sections, first {:?}, {:?}",
            p.total,
            p.facets.sections.len(),
            p.items.first().map(|i| (&i.label, &i.detail)),
            t.elapsed()
        );
    }
    let gl = s.list(&q("gladius", Filters::default(), ListGroup::None));
    for it in gl.items.iter().take(8) {
        println!(
            "  gladius hit: {} | {:?} | {} | model {} sounds {} | {:?} {:?}",
            it.label, it.detail, it.section, it.model, it.sounds, it.grade, it.class
        );
    }
    // The Pirate Gladius is a player ship (`_PIR` is no variant token).
    let pir = s.list(&q("gladius pirate", Filters::default(), ListGroup::None));
    let p0 = pir
        .items
        .iter()
        .find(|i| i.name == "EntityClassDefinition.AEGS_Gladius_PIR")
        .expect("Gladius Pirate");
    assert!(!p0.variant, "{p0:?}");
    let cool = s.list(&q("bracer", Filters::default(), ListGroup::None));
    println!("  bracer: {:?}", cool.items.first());
    let ship = gl
        .items
        .iter()
        .find(|i| i.name == "EntityClassDefinition.AEGS_Gladius")
        .expect("the Gladius");
    assert!(ship.model && ship.sounds, "{ship:?}");
    assert_eq!(gl.items[0].name, ship.name, "the ship ranks first");
    let gun = s.list(&q("type:weapongun", Filters::default(), ListGroup::None));
    assert!(gun.items.iter().filter(|i| i.sounds).count() > 50);
    // Search quality on real names: total, the first hits and their sections.
    for query in [
        "bracer",
        "mole",
        "ATLS",
        "arrow",
        "panther",
        "medpen",
        "glds",
        "galdius",
        "cutlass black",
        "quantum drive",
        "qauntum drive",
    ] {
        let t = Instant::now();
        let p = s.list(&q(query, Filters::default(), ListGroup::None));
        let top: Vec<String> = p
            .items
            .iter()
            .take(5)
            .map(|i| format!("{} [{}]", i.label, i.section))
            .collect();
        println!(
            "search {query:?}: {} rows ({:?}) in {:?}; sections {:?}; top {top:?}",
            p.total,
            p.matching,
            t.elapsed(),
            p.facets
                .sections
                .iter()
                .map(|v| (v.value.as_str(), v.count))
                .collect::<Vec<_>>()
        );
    }
    // "Changed vs <channel>" (optional): GameData of the other build (A) and of this one (B)
    // as the app reads them, `STARORG_GD_A` / `STARORG_GD_B` (JSON files).
    if let (Some(a), Some(b)) = (
        std::env::var_os("STARORG_GD_A"),
        std::env::var_os("STARORG_GD_B"),
    ) {
        use starorg_core::dev::dataset::{Dataset, infer};
        use starorg_core::dev::sections::{Change, compare_marks};
        let read = |p: &std::ffi::OsStr| -> Dataset {
            let gd: starorg_core::model::GameData =
                serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
            infer("gamedata.json", 0, serde_json::to_value(&gd).unwrap())
        };
        let t = Instant::now();
        let (da, db) = (read(&a), read(&b));
        let t_read = t.elapsed();
        let t = Instant::now();
        let tables = starorg_core::dev::diff::diff_sources(
            &[("gamedata.json".into(), &da)],
            &[("gamedata.json".into(), &db)],
            &|_, _| {},
        );
        let (marks, removed) = compare_marks(&tables, &[("gamedata.json".into(), &db)]);
        let t_diff = t.elapsed();
        let t = Instant::now();
        let found: Vec<(usize, Change)> = marks
            .iter()
            .filter_map(|(g, c)| ix.by_guid(g).map(|i| (i, *c)))
            .collect();
        let n = found.len();
        s.sections.set_changes(found);
        let t_mark = t.elapsed();
        let added = marks.iter().filter(|m| m.1 == Change::Added).count();
        let t = Instant::now();
        let changed = s.list(&q(
            "",
            Filters {
                changes: vec!["changed".into(), "added".into()],
                ..Default::default()
            },
            ListGroup::Section,
        ));
        println!(
            "compare: read GameData {t_read:?}, diff + marks {t_diff:?}, {} marks ({added} added), {removed} only in A, {n} found in the index in {t_mark:?}; list of changes {} rows in {:?}, by section {:?}",
            marks.len(),
            changed.total,
            t.elapsed(),
            changed
                .groups
                .iter()
                .map(|g| (g.label.as_str(), g.count))
                .collect::<Vec<_>>()
        );
        assert!(n > 0);
    }
    println!(
        "RSS after queries {:?} MB, peak {:?} MB",
        devworker::rss_bytes().map(|b| b / 1_000_000),
        peak_rss().map(|b| b / 1_000_000)
    );
}
