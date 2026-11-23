//! Timings of the read paths the Strings, Patches and Home screens hit, on the user's real
//! install (read only: overview, Strings pages, the patch list; the cache entry must exist).
//! Run with `cargo test --release -p starorg-codex-app --test real_perf -- --ignored
//! --nocapture`.

use starorg_codex_app::contract::Backend;
use starorg_codex_app::contract::real::{RealBackend, RealConfig};
use starorg_codex_app::contract::types::*;
use std::time::{Duration, Instant};

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let v = f();
    (v, t.elapsed())
}

fn avg(n: u32, mut f: impl FnMut(u32)) -> Duration {
    let t = Instant::now();
    for i in 0..n {
        f(i);
    }
    t.elapsed() / n
}

#[test]
#[ignore]
fn measure_read_paths_on_real_live() {
    let b = RealBackend::new(RealConfig::from_env().unwrap());
    let (o, overview_cold) = time(|| b.overview().unwrap());
    let overview_warm = avg(5, |_| {
        b.overview().unwrap();
    });
    // The first set-up channel whose game data is already cached (nothing is extracted).
    println!("overview cold {overview_cold:?} warm {overview_warm:?}");
    let Some((install, channel)) = o.installs.iter().find_map(|i| {
        i.channels
            .iter()
            .find(|c| c.set_up && c.cache == CacheState::Fresh)
            .map(|c| (i.id.clone(), c.channel.clone()))
    }) else {
        println!("no set-up channel has cached game data: Strings and Patches not measured");
        return;
    };
    println!("channel {channel}");
    let target = ChannelRef { install, channel };
    let q = |search: &str, sort: StringSort, offset: u32| StringsQuery {
        target: target.clone(),
        search: search.into(),
        sources: vec![],
        categories: vec![],
        changed_only: false,
        sort,
        offset,
        limit: 200,
        groups: vec![],
        manufacturers: vec![],
        sizes: vec![],
        group_by: GroupBy::Type,
    };
    let (first, cold) = time(|| b.query_strings(q("", StringSort::Group, 0)).unwrap());
    let page0 = avg(5, |_| {
        b.query_strings(q("", StringSort::Group, 0)).unwrap();
    });
    let pages = avg(20, |i| {
        b.query_strings(q("", StringSort::Group, 200 * (i + 1)))
            .unwrap();
    });
    let source_sort = avg(3, |i| {
        b.query_strings(q("", StringSort::Source, 200 * i)).unwrap();
    });
    let search = avg(3, |i| {
        b.query_strings(q("gladius", StringSort::Key, 200 * i))
            .unwrap();
    });
    let patches = avg(5, |_| {
        b.list_patches(target.clone()).unwrap();
    });
    println!(
        "keys {}: strings cold {cold:?}, page 0 {page0:?}, later pages {pages:?}, source sort {source_sort:?}, search {search:?} | patch list {patches:?}",
        first.all
    );
}
