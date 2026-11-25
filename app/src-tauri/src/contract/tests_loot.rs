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

/// Ungrouped: one group, every row likeliest first.
fn detail(b: &MockBackend, view: LootView, id: &str) -> LootDetail {
    filtered(b, view, id, None)
}

fn filtered(b: &MockBackend, view: LootView, id: &str, filter: Option<&str>) -> LootDetail {
    ask(b, view, id, filter, Some(LootGroupBy::None), None)
}

fn ask(
    b: &MockBackend,
    view: LootView,
    id: &str,
    filter: Option<&str>,
    group_by: Option<LootGroupBy>,
    groups: Option<&[&str]>,
) -> LootDetail {
    b.loot_detail(LootDetailRequest {
        target: Some(live()),
        view,
        id: id.into(),
        filter: filter.map(str::to_string),
        group_by,
        groups: groups.map(|g| g.iter().map(|k| k.to_string()).collect()),
    })
    .unwrap()
}

fn rows(d: &LootDetail) -> Vec<&LootDrop> {
    d.rows().collect()
}

/// The loot keys of `samples.gen.json` (`samples()` in tests.rs adds them).
pub(super) fn loot_samples() -> Vec<(&'static str, Value)> {
    let b = MockBackend::default();
    let missing = list(&b, Some(live()), LootView::Areas);
    read_live(&b);
    let areas = list(&b, Some(live()), LootView::Areas);
    let items = list(&b, Some(live()), LootView::Items);
    // The view's default grouping and groups (areas by container, items by place).
    let area = ask(&b, LootView::Areas, &areas.rows[0].id, None, None, None);
    let item = ask(&b, LootView::Items, &items.rows[0].id, None, None, None);
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
        assert!(rows(&d).windows(2).all(|w| w[0].odds >= w[1].odds));
        assert_eq!(d.total, gun.count);
        assert!((rows(&d)[0].odds - gun.best).abs() < 1e-12);
    }
    // A rack holds a gun (fixed) and the ration lies loose in caves.
    let racks = detail(&b, LootView::Areas, "lootgeneration_slotpreset_kareah");
    assert_eq!(
        rows(&racks)[0].kind,
        LootDropKind::Fixed,
        "{:?}",
        racks.groups
    );
    let ration = detail(&b, LootView::Items, "syn_food_ration_berry");
    assert!(ration.rows().any(|r| r.kind == LootDropKind::Loose));
    // Unknown ids answer an empty detail.
    let none = detail(&b, LootView::Areas, "nowhere");
    assert!(none.entry.is_none() && none.groups.is_empty());
}

#[test]
fn loot_npc_bodies_are_areas_and_item_sources() {
    use starorg_mcp::loot::{NPC_KIND, is_npc};
    let b = MockBackend::default();
    read_live(&b);
    let areas = list(&b, Some(live()), LootView::Areas);
    let npcs: Vec<String> = areas
        .rows
        .iter()
        .filter(|r| is_npc(&r.id))
        .map(|r| {
            let mut p = r.path.clone();
            p.push(r.name.clone());
            p.join(" > ")
        })
        .collect();
    // The template is no NPC; one-NPC groups sit right under the kind.
    assert_eq!(
        npcs,
        [
            "NPCs (body loot) > Ace pilot",
            "NPCs (body loot) > Mendo Ren",
            "NPCs (body loot) > Ground combat > Grunt",
            "NPCs (body loot) > Ground combat > Soldier",
            "NPCs (body loot) > Ground combat > Soldier (with council scrip)",
        ]
    );
    // By slot: the council-scrip variant's unique item, by class, half the time.
    let d = ask(
        &b,
        LootView::Areas,
        "npc:V3LootTable_Actor_Soldier_CouncilScrip",
        None,
        None,
        None,
    );
    let labels: Vec<&str> = d.groups.iter().map(|g| g.label.as_str()).collect();
    assert_eq!(labels, ["Unique item", "Ammo", "Sidearm"], "{labels:?}");
    let unique = d.groups[0].rows.as_ref().unwrap();
    assert_eq!(unique.len(), 1);
    assert_eq!(unique[0].id, "syn_food_ration_mint");
    assert!((unique[0].odds - 0.5).abs() < 1e-12);
    // The pilot's always-filled slot reads without the archetype words.
    let d = ask(
        &b,
        LootView::Areas,
        "npc:LootTable_Actor_AcePilot",
        None,
        None,
        None,
    );
    assert_eq!(d.groups[0].label, "Rations");
    // By item: the NPCs are drop places of their own kind.
    let d = ask(
        &b,
        LootView::Items,
        "syn_food_ration_mint",
        None,
        None,
        None,
    );
    assert!(
        d.groups.iter().any(|g| g.label == NPC_KIND),
        "{:?}",
        d.groups
    );
}

#[test]
fn loot_rows_carry_tree_paths_and_details_filter() {
    let b = MockBackend::default();
    read_live(&b);
    let areas = list(&b, Some(live()), LootView::Areas);
    let at = |id: &str| areas.rows.iter().find(|r| r.id == id).unwrap();
    let path = |id: &str| {
        let r = at(id);
        let mut p = r.path.clone();
        p.push(r.name.clone());
        p.join(" > ")
    };
    assert_eq!(
        path("caves/loot_caves_occupied_sand_pyro"),
        "Caves > Occupied > Sand Pyro"
    );
    assert_eq!(path("caves/loot_caves_unoccupied"), "Caves > Unoccupied");
    assert_eq!(
        path("distributioncentre_multiconfig_cargowing"),
        "Distribution centres > Cargo wing > General"
    );
    assert_eq!(
        path("v3slotpreset_distributioncentres_highsecurity"),
        "Distribution centres > High security"
    );
    assert_eq!(
        path("lootgeneration_multislotpreset_kareah"),
        "Security Post Kareah > General (multi)"
    );
    // Alone in its place: a top-level row.
    assert_eq!(
        path("lootgeneration_slotpreset_ugfs"),
        "Underground facilities"
    );
    let items = list(&b, Some(live()), LootView::Items);
    assert!(items.rows.iter().all(|r| r.path.len() <= 2));
    assert!(items.rows.iter().any(|r| r.path.len() == 2));

    // The filter: every word, still likeliest first; `all` counts every drop.
    let id = "caves/loot_caves_occupied_rock";
    let full = detail(&b, LootView::Areas, id);
    let f = filtered(&b, LootView::Areas, id, Some("pathfinder"));
    let fr = rows(&f);
    assert!(!fr.is_empty() && fr.len() < rows(&full).len());
    assert!(fr.iter().all(|r| r.name.contains("Pathfinder")));
    assert_eq!((f.total, f.all), (fr.len(), full.total));
    assert!(fr.windows(2).all(|w| w[0].odds >= w[1].odds));
    let n = filtered(&b, LootView::Areas, id, Some("zzz"));
    assert!(n.entry.is_some() && n.groups.is_empty() && n.total == 0);
    // An area drop names where it sits in the tree.
    let gun = &items
        .rows
        .iter()
        .find(|r| r.group == "FPS weapons")
        .unwrap()
        .id;
    let d = detail(&b, LootView::Items, gun);
    assert!(
        d.rows()
            .any(|r| r.group.contains(starorg_mcp::loot::PATH_SEP)),
        "{:?}",
        d.groups
    );
}

#[test]
fn loot_details_answer_group_headers_and_the_rows_asked_for() {
    let b = MockBackend::default();
    read_live(&b);
    let id = "caves/loot_caves_occupied_rock";
    // Areas: by container by default, best odds first; headers cover every drop.
    let d = ask(&b, LootView::Areas, id, None, None, None);
    assert_eq!(d.group_by, LootGroupBy::Container);
    assert!(d.groups.len() >= 2, "{:?}", d.groups);
    assert!(d.groups.windows(2).all(|w| w[0].best >= w[1].best));
    assert_eq!(d.groups.iter().map(|g| g.count).sum::<usize>(), d.all);
    assert!(d.groups[0].rows.is_some());
    // Opening one group answers its rows only.
    let last = d.groups.last().unwrap();
    let one = ask(
        &b,
        LootView::Areas,
        id,
        None,
        None,
        Some(&[last.key.as_str()]),
    );
    assert!(one.groups[0].rows.is_none() || one.groups.len() == 1);
    let got = one.group(&last.key).unwrap().rows.as_ref().unwrap();
    assert_eq!(got.len(), last.count);
    assert!(got.iter().all(|r| r.container == last.label));
    // A filter narrows the headers and opens the matching groups.
    let f = ask(&b, LootView::Areas, id, Some("pathfinder"), None, None);
    assert!(f.groups.iter().all(|g| g.rows.is_some()));
    assert_eq!(f.groups.iter().map(|g| g.count).sum::<usize>(), f.total);
    // Items: by place (the top of the area's tree path) by default.
    let items = list(&b, Some(live()), LootView::Items);
    let gun = items
        .rows
        .iter()
        .find(|r| r.group == "FPS weapons")
        .unwrap();
    let d = ask(&b, LootView::Items, &gun.id, None, None, None);
    assert_eq!(d.group_by, LootGroupBy::Place);
    let areas = list(&b, Some(live()), LootView::Areas);
    let places: std::collections::HashSet<String> = areas
        .rows
        .iter()
        .map(|r| r.path.first().unwrap_or(&r.group).clone())
        .collect();
    assert!(
        d.groups.iter().all(|g| places.contains(&g.label)),
        "{:?}",
        d.groups
    );
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
        (
            "LootGroupBy",
            names(&[
                LootGroupBy::Container,
                LootGroupBy::Place,
                LootGroupBy::None,
            ]),
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
    // With a filter too.
    let d = ipc(
        "loot_detail",
        json!({"req": {"target": null, "view": "areas", "id": "x", "filter": "gun"}}),
    )
    .unwrap();
    assert_eq!(d["all"], 0);
    // Grouping and the groups to open parse (a missing answer is the view's default, empty).
    let d = ipc(
        "loot_detail",
        json!({"req": {"target": null, "view": "items", "id": "x", "group_by": "none", "groups": ["all"]}}),
    )
    .unwrap();
    assert_eq!(d["group_by"], "place");
    assert_eq!(d["groups"], json!([]));
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
