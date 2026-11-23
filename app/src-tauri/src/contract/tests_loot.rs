//! Loot screen contract tests: samples for `loot_list` and `loot_detail`, the TS mirror's
//! unions, the mock's states, IPC, and the mock's data file for the TS mock
//! (`mock-loot.gen.json`).

use super::Backend;
use super::mock::MockBackend;
use super::mock_loot;
use super::tests::{app_src, ipc, names, ts_union};
use super::types::*;
use serde_json::{Value, json};

fn live() -> ChannelRef {
    ChannelRef {
        install: "inst-7f3a".into(),
        channel: "LIVE".into(),
    }
}

/// LIVE set up and its game data read (the mock's loot is `ready` then).
fn read_live(b: &MockBackend) {
    b.setup(SetupRequest {
        install: "inst-7f3a".into(),
        channels: vec![ChannelConsent {
            channel: "LIVE".into(),
            consent: true,
            switch_language: false,
        }],
        preset: Preset::Recommended,
        fix_known_bugs: true,
    })
    .unwrap();
    b.read_game_data(ReadGameDataRequest { target: live() })
        .unwrap();
}

fn list(b: &MockBackend, target: Option<ChannelRef>, view: LootView) -> LootList {
    b.loot_list(LootListRequest { target, view }).unwrap()
}

fn detail(b: &MockBackend, view: LootView, id: &str) -> LootDetail {
    b.loot_detail(LootDetailRequest {
        target: Some(live()),
        view,
        id: id.into(),
    })
    .unwrap()
}

/// The loot keys of `samples.gen.json` (`samples()` in tests.rs adds them).
pub(super) fn loot_samples() -> Vec<(&'static str, Value)> {
    let b = MockBackend::default();
    let missing = list(&b, Some(live()), LootView::Areas);
    read_live(&b);
    let areas = list(&b, Some(live()), LootView::Areas);
    let items = list(&b, Some(live()), LootView::Items);
    let area = detail(&b, LootView::Areas, &areas.rows[0].id);
    let item = detail(&b, LootView::Items, &items.rows[0].id);
    vec![
        ("loot_list_missing", json!(missing)),
        ("loot_list", json!(areas)),
        ("loot_detail", json!(area)),
        ("loot_detail_item", json!(item)),
    ]
}

#[test]
fn loot_on_the_mock_is_missing_until_the_game_data_is_read() {
    let b = MockBackend::default();
    assert_eq!(
        list(&b, Some(live()), LootView::Items).state,
        LootDataState::Missing
    );
    assert_eq!(
        detail(&b, LootView::Areas, "caves/loot_caves_unoccupied").state,
        LootDataState::Missing
    );
    read_live(&b);
    assert_eq!(
        list(&b, None, LootView::Areas).state,
        LootDataState::Missing
    );
    let areas = list(&b, Some(live()), LootView::Areas);
    assert_eq!(areas.state, LootDataState::Ready);
    assert_eq!(areas.build.as_deref(), Some(mock_loot::MOCK_LOOT_BUILD));
    // The test preset is not listed; generic loot is last, under Other.
    assert!(areas.rows.iter().all(|r| !r.id.starts_with("test_")));
    assert_eq!(areas.rows.last().unwrap().group, "Other");
    assert!(areas.rows.len() >= 10);
    let items = list(&b, Some(live()), LootView::Items);
    assert!(items.rows.len() >= 40, "{}", items.rows.len());
    // The placeholder-named piece reads from its class name.
    assert!(
        items.rows.iter().any(|r| r.name == "SYN Armour Legs Storm"),
        "{:?}",
        items.rows.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
    // A catalogue item links to its record; its areas are likeliest first.
    let guns: Vec<&LootSummary> = items
        .rows
        .iter()
        .filter(|r| r.group == "FPS weapons")
        .collect();
    assert!(guns.len() >= 2);
    for gun in guns {
        let d = detail(&b, LootView::Items, &gun.id);
        assert!(d.record.as_ref().unwrap().guid.starts_with("54000000-"));
        assert!(d.rows.windows(2).all(|w| w[0].odds >= w[1].odds));
        assert_eq!(d.total, gun.count);
        assert!((d.rows[0].odds - gun.best).abs() < 1e-12);
    }
    // A rack holds a gun (fixed) and the ration lies loose in caves.
    let racks = detail(&b, LootView::Areas, "lootgeneration_slotpreset_kareah");
    assert_eq!(racks.rows[0].kind, LootDropKind::Fixed, "{:?}", racks.rows);
    let ration = detail(&b, LootView::Items, "syn_food_ration_berry");
    assert!(ration.rows.iter().any(|r| r.kind == LootDropKind::Loose));
    // Unknown ids answer an empty detail.
    let none = detail(&b, LootView::Areas, "nowhere");
    assert!(none.entry.is_none() && none.rows.is_empty());
}

#[test]
fn loot_unions_match_types_ts() {
    let ts = std::fs::read_to_string(app_src().join("types.ts")).unwrap();
    for (name, want) in [
        (
            "LootDataState",
            names(&[LootDataState::Ready, LootDataState::Missing]),
        ),
        ("LootView", names(&[LootView::Areas, LootView::Items])),
        (
            "LootDropKind",
            names(&[LootDropKind::Loot, LootDropKind::Fixed, LootDropKind::Loose]),
        ),
    ] {
        assert_eq!(ts_union(&ts, name), want, "{name}");
    }
}

#[test]
fn loot_commands_answer_over_ipc() {
    let r = ipc(
        "loot_list",
        json!({"req": {"target": {"install": "inst-7f3a", "channel": "LIVE"}, "view": "items"}}),
    )
    .unwrap();
    assert_eq!(r["state"], "missing");
    assert_eq!(r["view"], "items");
    let d = ipc(
        "loot_detail",
        json!({"req": {"target": null, "view": "areas", "id": "x"}}),
    )
    .unwrap();
    assert_eq!(d["state"], "missing");
}

/// `mock-loot.gen.json`: every list and detail of the mock, read by the TS mock.
/// Regenerated with the samples (`UPDATE_CONTRACT_SAMPLES=1`).
#[test]
fn mock_loot_data_matches_the_committed_file() {
    let path = app_src().join("mock-loot.gen.json");
    let text = serde_json::to_string(&super::tests::sorted(mock_loot::gen_file())).unwrap() + "\n";
    if std::env::var_os("UPDATE_CONTRACT_SAMPLES").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        committed == text,
        "{} is out of date: run UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-codex-app",
        path.display()
    );
}
