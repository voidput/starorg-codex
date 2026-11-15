//! Developer mode contract tests (addendum 7.11): samples for every dev command, the TS
//! mirror's unions, `dev-disabled` while the setting is off, and the mock's synthetic data
//! file for the TS mock.

use super::Backend;
use super::dev::*;
use super::mock::MockBackend;
use super::mock_dev;
use super::tests::{app_src, ipc, names, sorted, ts_union};
use super::types::*;
use serde_json::{Value, json};

fn live() -> ChannelRef {
    ChannelRef {
        install: "inst-7f3a".into(),
        channel: "LIVE".into(),
    }
}

fn noop(_: JobProgress) {}

fn dev_mock() -> MockBackend {
    let b = MockBackend::default();
    let mut s = b.get_settings().unwrap();
    s.advanced.developer_mode = true;
    b.save_settings(s).unwrap();
    b
}

fn tree(root: TreeRoot, node: &str, limit: u32) -> DevTreeRequest {
    DevTreeRequest {
        target: live(),
        root,
        node: node.into(),
        offset: 0,
        limit,
        main_only: true,
    }
}

fn rows(source: &str, dataset: &str, table: &str) -> DevRowsRequest {
    DevRowsRequest {
        target: live(),
        source: source.into(),
        dataset: dataset.into(),
        table: table.into(),
        columns: vec![],
        search: String::new(),
        filters: vec![],
        sort: None,
        offset: 0,
        limit: 5,
        group: None,
    }
}

fn records_req(dataset: &str, table: &str) -> DevDiffRecordsRequest {
    DevDiffRecordsRequest {
        target: live(),
        dataset: dataset.into(),
        table: table.into(),
        state: None,
        field: None,
        identity: None,
        query: None,
        offset: 0,
        limit: 5,
        group: None,
    }
}

fn groups_req(source: &str, dataset: &str, table: &str, by: &[&str]) -> DevGroupsRequest {
    DevGroupsRequest {
        target: live(),
        source: source.into(),
        dataset: dataset.into(),
        table: table.into(),
        by: by
            .iter()
            .map(|c| GroupLevel {
                column: c.to_string(),
                folders: false,
            })
            .collect(),
        open: vec![],
        columns: vec![],
        search: String::new(),
        filters: vec![],
    }
}

fn weapon_chain() -> [&'static str; 3] {
    ["type", "subType", "size"]
}

/// One sample per dev command (keys are command names plus a variant), for samples.gen.json.
pub(super) fn dev_samples() -> Vec<(&'static str, Value)> {
    let b = dev_mock();
    let stopped = b.dev_status(live()).unwrap();
    let open = b.dev_open(live(), &noop).unwrap();
    let search = |q: &str, n: u32| {
        b.dev_search(DevSearchRequest {
            target: live(),
            query: q.into(),
            limit: n,
        })
        .unwrap()
    };
    let record = |what: String, pointer: &str, depth: Option<u32>| {
        b.dev_record(DevRecordRequest {
            target: live(),
            what,
            pointer: pointer.into(),
            depth,
        })
        .unwrap()
    };
    let sources = b.dev_sources(live()).unwrap();
    let added = b.dev_add_source(live()).unwrap();
    let mut sorted_rows = rows("build:LIVE", "gamedata.json", "components");
    sorted_rows.search = "cooler".into();
    sorted_rows.sort = Some(RowSort {
        column: "health".into(),
        desc: true,
    });
    sorted_rows.limit = 3;
    let mut filtered = rows("build:LIVE", "gamedata.json", "components");
    filtered.filters = vec![
        RowFilter {
            column: "manufacturer".into(),
            values: vec!["SYNB".into()],
            ..Default::default()
        },
        RowFilter {
            column: "size".into(),
            min: Some(2.0),
            max: None,
            ..Default::default()
        },
    ];
    filtered.columns = vec!["name".into(), "size".into(), "manufacturer".into()];
    let diff = b
        .dev_diff(
            DevDiffRequest {
                target: live(),
                a: "build:LIVE".into(),
                b: "build:PTU".into(),
            },
            &noop,
        )
        .unwrap();
    let recs = b
        .dev_diff_records(records_req("gamedata.json", "components"))
        .unwrap();
    let diff_groups = b
        .dev_diff_groups(DevDiffGroupsRequest {
            target: live(),
            by: DiffGroupBy::Auto,
            open: vec![],
        })
        .unwrap();
    let diff_groups_open = b
        .dev_diff_groups(DevDiffGroupsRequest {
            target: live(),
            by: DiffGroupBy::Auto,
            open: vec![r#"["gamedata.json","weapons"]"#.into()],
        })
        .unwrap();
    let recs_group = b
        .dev_diff_records(DevDiffRecordsRequest {
            group: Some(DiffGroupPick {
                by: DiffGroupBy::Auto,
                node: r#"["gamedata.json","weapons","WeaponMining"]"#.into(),
            }),
            ..records_req("gamedata.json", "weapons")
        })
        .unwrap();
    let table_weapons = b
        .dev_table(DevTableRef {
            target: live(),
            source: "build:LIVE".into(),
            dataset: "gamedata.json".into(),
            table: "weapons".into(),
        })
        .unwrap();
    let groups = b
        .dev_groups(groups_req(
            "build:LIVE",
            "gamedata.json",
            "weapons",
            &weapon_chain(),
        ))
        .unwrap();
    let mut open_req = groups_req("build:LIVE", "gamedata.json", "weapons", &weapon_chain());
    open_req.open = vec![r#"["WeaponGun"]"#.into(), r#"["WeaponGun","Gun"]"#.into()];
    open_req.search = "repeater".into();
    let groups_open = b.dev_groups(open_req).unwrap();
    let mut rows_group = rows("build:LIVE", "gamedata.json", "weapons");
    rows_group.group = Some(GroupPick {
        by: groups_req("", "", "", &weapon_chain()).by,
        node: r#"["WeaponGun","Gun"]"#.into(),
    });
    rows_group.limit = 10;
    let first = recs.records[0].id.clone();
    let field = b
        .dev_diff_record(DevDiffRecordRequest {
            target: live(),
            dataset: "gamedata.json".into(),
            table: "components".into(),
            id: first,
            identity: None,
        })
        .unwrap();
    // Readable labels (7.14): a search that only matches what a code reads as, a record
    // query by display name, a field diff with a unit.
    let mut readable_rows = rows("build:LIVE", "gamedata.json", "components");
    readable_rows.search = "alpha synth".into();
    readable_rows.limit = 3;
    let readable_rows = b.dev_rows(readable_rows).unwrap();
    let by_name = b
        .dev_diff_records(DevDiffRecordsRequest {
            query: Some("sample cooler 4".into()),
            ..records_req("gamedata.json", "components")
        })
        .unwrap();
    let ship = b
        .dev_diff_record(DevDiffRecordRequest {
            target: live(),
            dataset: "gamedata.json".into(),
            table: "ships".into(),
            id: "SYN_Ship_003".into(),
            identity: None,
        })
        .unwrap();
    let table_ref = |source: &str, dataset: &str, table: &str| DevTableRef {
        target: live(),
        source: source.into(),
        dataset: dataset.into(),
        table: table.into(),
    };
    vec![
        ("dev_status", json!(stopped)),
        ("dev_open", json!(open)),
        ("dev_types", json!(b.dev_types(live()).unwrap())),
        (
            "dev_tree",
            json!(b.dev_tree(tree(TreeRoot::Type, "", 50)).unwrap()),
        ),
        (
            "dev_tree_type",
            json!(
                b.dev_tree(tree(TreeRoot::Type, "t:SynComponent", 5))
                    .unwrap()
            ),
        ),
        (
            "dev_tree_path",
            json!(b.dev_tree(tree(TreeRoot::Path, "", 50)).unwrap()),
        ),
        (
            "dev_tree_folder",
            json!(
                b.dev_tree(tree(TreeRoot::Path, "p:entities/scitem/ships/cooler/", 3))
                    .unwrap()
            ),
        ),
        ("dev_search", json!(search("cooler 01", 5))),
        ("dev_search_guid", json!(search("5100", 3))),
        ("dev_search_type", json!(search("type:SynTag 5", 3))),
        (
            "dev_record",
            json!(record(mock_dev::ship_guid(1), "", None)),
        ),
        (
            "dev_record_cut",
            json!(record(
                mock_dev::ship_guid(mock_dev::HUGE_SHIP),
                "",
                Some(2)
            )),
        ),
        (
            "dev_record_pointer",
            json!(record("SYN_cooler_004".into(), "/_RecordValue_/Ship", None)),
        ),
        (
            "dev_export_record",
            json!(
                b.dev_export_record(DevExportRecordRequest {
                    target: live(),
                    what: "SYN_Ship_001".into(),
                })
                .unwrap()
            ),
        ),
        (
            "dev_export_type",
            json!(
                b.dev_export_type(
                    DevExportTypeRequest {
                        target: live(),
                        type_name: "SynComponent".into(),
                        main_only: true,
                    },
                    &noop
                )
                .unwrap()
            ),
        ),
        ("dev_sources", json!(sources)),
        ("dev_add_source", json!(added)),
        (
            "dev_dataset",
            json!(
                b.dev_dataset(DevDatasetRef {
                    target: live(),
                    source: "folder:0".into(),
                    dataset: "game-weapons.json".into(),
                })
                .unwrap()
            ),
        ),
        (
            "dev_table",
            json!(
                b.dev_table(table_ref("build:LIVE", "gamedata.json", "components"))
                    .unwrap()
            ),
        ),
        (
            "dev_table_kv",
            json!(
                b.dev_table(table_ref(
                    "folder:0",
                    "game-strings-english.json",
                    "strings"
                ))
                .unwrap()
            ),
        ),
        ("dev_rows", json!(b.dev_rows(sorted_rows).unwrap())),
        ("dev_rows_filtered", json!(b.dev_rows(filtered).unwrap())),
        ("dev_rows_group", json!(b.dev_rows(rows_group).unwrap())),
        ("dev_table_weapons", json!(table_weapons)),
        ("dev_groups", json!(groups)),
        ("dev_groups_open", json!(groups_open)),
        (
            "dev_row",
            json!(
                b.dev_row(DevRowRequest {
                    target: live(),
                    source: "folder:0".into(),
                    dataset: "game-weapons.json".into(),
                    table: "shipWeapons".into(),
                    row: 1,
                })
                .unwrap()
            ),
        ),
        (
            "dev_row_loc",
            json!(
                b.dev_row(DevRowRequest {
                    target: live(),
                    source: "build:LIVE".into(),
                    dataset: "gamedata.json".into(),
                    table: "components".into(),
                    row: 0,
                })
                .unwrap()
            ),
        ),
        ("dev_diff", json!(diff)),
        (
            "dev_diff_summary",
            json!(b.dev_diff_summary(live()).unwrap()),
        ),
        ("dev_diff_records", json!(recs)),
        (
            "dev_diff_records_added",
            json!(
                b.dev_diff_records(DevDiffRecordsRequest {
                    state: Some(RecordState::Added),
                    ..records_req("gamedata.json", "components")
                })
                .unwrap()
            ),
        ),
        ("dev_diff_records_group", json!(recs_group)),
        ("dev_diff_groups", json!(diff_groups)),
        ("dev_diff_groups_open", json!(diff_groups_open)),
        ("dev_diff_record", json!(field)),
        ("dev_rows_readable", json!(readable_rows)),
        ("dev_diff_records_query", json!(by_name)),
        ("dev_diff_record_ship", json!(ship)),
        ("dev_close", json!(b.dev_close(live()).unwrap())),
    ]
}

fn j<T: serde::Serialize>(x: Result<T, AppError>) -> Result<Value, AppError> {
    x.map(|v| json!(v))
}

#[test]
fn every_dev_command_is_refused_while_the_setting_is_off() {
    let b = MockBackend::default();
    assert!(!b.get_settings().unwrap().advanced.developer_mode);
    let code = |r: Result<Value, AppError>| r.unwrap_err().code;
    let t = tree(TreeRoot::Type, "", 10);
    let tr = DevTableRef {
        target: live(),
        source: "build:LIVE".into(),
        dataset: "gamedata.json".into(),
        table: "components".into(),
    };
    let calls: Vec<(&str, Result<Value, AppError>)> = vec![
        ("dev_status", j(b.dev_status(live()))),
        ("dev_open", j(b.dev_open(live(), &noop))),
        ("dev_close", j(b.dev_close(live()))),
        ("dev_types", j(b.dev_types(live()))),
        ("dev_tree", j(b.dev_tree(t))),
        (
            "dev_search",
            j(b.dev_search(DevSearchRequest {
                target: live(),
                query: "x".into(),
                limit: 5,
            })),
        ),
        (
            "dev_record",
            j(b.dev_record(DevRecordRequest {
                target: live(),
                what: "x".into(),
                pointer: String::new(),
                depth: None,
            })),
        ),
        (
            "dev_export_record",
            j(b.dev_export_record(DevExportRecordRequest {
                target: live(),
                what: "x".into(),
            })),
        ),
        (
            "dev_export_type",
            j(b.dev_export_type(
                DevExportTypeRequest {
                    target: live(),
                    type_name: "x".into(),
                    main_only: true,
                },
                &noop,
            )),
        ),
        ("dev_sources", j(b.dev_sources(live()))),
        ("dev_add_source", j(b.dev_add_source(live()))),
        (
            "dev_remove_source",
            j(b.dev_remove_source(DevSourceRef {
                target: live(),
                source: "folder:0".into(),
            })),
        ),
        ("dev_download_dump", j(b.dev_download_dump(live(), &noop))),
        (
            "dev_dataset",
            j(b.dev_dataset(DevDatasetRef {
                target: live(),
                source: "build:LIVE".into(),
                dataset: "gamedata.json".into(),
            })),
        ),
        ("dev_table", j(b.dev_table(tr.clone()))),
        (
            "dev_rows",
            j(b.dev_rows(rows("build:LIVE", "gamedata.json", "components"))),
        ),
        (
            "dev_row",
            j(b.dev_row(DevRowRequest {
                target: live(),
                source: tr.source.clone(),
                dataset: tr.dataset.clone(),
                table: tr.table.clone(),
                row: 0,
            })),
        ),
        (
            "dev_groups",
            j(b.dev_groups(groups_req(
                "build:LIVE",
                "gamedata.json",
                "components",
                &["kind"],
            ))),
        ),
        (
            "dev_diff",
            j(b.dev_diff(
                DevDiffRequest {
                    target: live(),
                    a: "build:LIVE".into(),
                    b: "build:PTU".into(),
                },
                &noop,
            )),
        ),
        ("dev_diff_summary", j(b.dev_diff_summary(live()))),
        (
            "dev_diff_records",
            j(b.dev_diff_records(records_req("gamedata.json", "components"))),
        ),
        (
            "dev_diff_groups",
            j(b.dev_diff_groups(DevDiffGroupsRequest {
                target: live(),
                by: DiffGroupBy::Tables,
                open: vec![],
            })),
        ),
        (
            "dev_diff_record",
            j(b.dev_diff_record(DevDiffRecordRequest {
                target: live(),
                dataset: "gamedata.json".into(),
                table: "components".into(),
                id: "x".into(),
                identity: None,
            })),
        ),
    ];
    let dev_names: Vec<&str> = super::COMMAND_NAMES
        .iter()
        .copied()
        .filter(|n| n.starts_with("dev_"))
        .collect();
    let called: Vec<&str> = calls.iter().map(|c| c.0).collect();
    assert_eq!(called, dev_names, "every dev command is checked");
    for (name, r) in calls {
        assert_eq!(code(r), ErrorCode::DevDisabled, "{name}");
    }
}

#[test]
fn dev_disabled_crosses_ipc_as_its_own_code() {
    let e = ipc(
        "dev_status",
        json!({ "req": { "install": "inst-7f3a", "channel": "LIVE" } }),
    )
    .unwrap_err();
    assert_eq!(e["code"], "dev-disabled");
    let e = ipc(
        "dev_open",
        json!({ "req": { "install": "inst-7f3a", "channel": "LIVE" } }),
    )
    .unwrap_err();
    assert_eq!(e["code"], "dev-disabled");
}

#[test]
fn the_mock_explorer_has_300_records_and_a_huge_one_is_cut() {
    let b = dev_mock();
    let c = mock_dev::core();
    assert_eq!(c.index.len(), 300);
    assert_eq!(c.index.types().len(), 3);
    let huge = b
        .dev_record(DevRecordRequest {
            target: live(),
            what: mock_dev::ship_guid(mock_dev::HUGE_SHIP),
            pointer: String::new(),
            depth: None,
        })
        .unwrap();
    assert!(huge.header.bytes > 1_500_000, "{}", huge.header.bytes);
    assert!(huge.truncated);
    assert!(huge.json.len() <= starorg_core::dev::record::MAX_REPLY);
    // Deeper nodes load on expand.
    let port = b
        .dev_record(DevRecordRequest {
            target: live(),
            what: mock_dev::ship_guid(mock_dev::HUGE_SHIP),
            pointer: "/_RecordValue_/Loadout/7".into(),
            depth: None,
        })
        .unwrap();
    assert!(!port.truncated);
    assert!(port.json.contains("hardpoint_syn_00007"));
    // References carry name, type and path; loc keys resolve against the mock strings.
    let ship = b
        .dev_record(DevRecordRequest {
            target: live(),
            what: "SYN_Ship_002".into(),
            pointer: String::new(),
            depth: None,
        })
        .unwrap();
    let v: Value = serde_json::from_str(&ship.json).unwrap();
    assert_eq!(v["_RecordValue_"]["Components"][0]["type"], "SynComponent");
    assert_eq!(ship.loc.len(), 1);
    assert!(ship.loc[0].text.starts_with("Synthetic hull 16."));
}

#[test]
fn dumps_need_the_switch_and_are_a_follow_up() {
    let b = dev_mock();
    assert_eq!(
        b.dev_download_dump(live(), &noop).unwrap_err().code,
        ErrorCode::NetworkOff
    );
    let mut s = b.get_settings().unwrap();
    s.network.published_dumps = true;
    b.save_settings(s).unwrap();
    assert_eq!(
        b.dev_download_dump(live(), &noop).unwrap_err().code,
        ErrorCode::NotImplemented
    );
    let src = b.dev_sources(live()).unwrap();
    let dump = src.iter().find(|s| s.kind == SourceKind::Dump).unwrap();
    assert_eq!(dump.state, SourceState::Off);
}

#[test]
fn diff_identity_override_and_filters() {
    let b = dev_mock();
    b.dev_diff(
        DevDiffRequest {
            target: live(),
            a: "build:LIVE".into(),
            b: "build:PTU".into(),
        },
        &noop,
    )
    .unwrap();
    let r = b
        .dev_diff_records(records_req("gamedata.json", "components"))
        .unwrap();
    assert_eq!(r.summary.identity.as_deref(), Some("recordName"));
    assert_eq!((r.summary.added, r.summary.removed), (1, 1));
    assert!(r.summary.changed >= 5);
    assert!(r.fields.iter().any(|f| f.value == "health"));
    let only = b
        .dev_diff_records(DevDiffRecordsRequest {
            field: Some("health".into()),
            limit: 100,
            ..records_req("gamedata.json", "components")
        })
        .unwrap();
    assert!(
        only.records
            .iter()
            .all(|x| x.fields.iter().any(|f| f == "/health"))
    );
    // By an identity the user picks.
    let by = b
        .dev_diff_records(DevDiffRecordsRequest {
            identity: Some("name".into()),
            query: None,
            ..records_req("gamedata.json", "components")
        })
        .unwrap();
    assert_eq!(by.summary.identity.as_deref(), Some("name"));
    assert!(by.identity_options.contains(&"name".to_string()));
}

#[test]
fn dev_unions_in_dev_ts_list_every_variant() {
    let ts = std::fs::read_to_string(app_src().join("dev.ts")).unwrap();
    let cases: Vec<(&str, Vec<String>)> = vec![
        (
            "WorkerState",
            names(&[
                WorkerState::Stopped,
                WorkerState::Starting,
                WorkerState::Ready,
                WorkerState::Failed,
            ]),
        ),
        ("TreeRoot", names(&[TreeRoot::Type, TreeRoot::Path])),
        (
            "NodeKind",
            names(&[NodeKind::Type, NodeKind::Folder, NodeKind::Record]),
        ),
        (
            "ColType",
            names(&[
                ColType::Text,
                ColType::Number,
                ColType::Bool,
                ColType::Object,
                ColType::List,
                ColType::Mixed,
                ColType::Empty,
            ]),
        ),
        (
            "TableKind",
            names(&[TableKind::Main, TableKind::Index, TableKind::Tree]),
        ),
        (
            "SourceKind",
            names(&[SourceKind::Build, SourceKind::Folder, SourceKind::Dump]),
        ),
        (
            "SourceState",
            names(&[
                SourceState::Ready,
                SourceState::Off,
                SourceState::Missing,
                SourceState::Error,
            ]),
        ),
        (
            "Presence",
            names(&[Presence::Both, Presence::OnlyA, Presence::OnlyB]),
        ),
        (
            "DiffGroupBy",
            names(&[DiffGroupBy::Tables, DiffGroupBy::Auto, DiffGroupBy::Folders]),
        ),
        (
            "RecordState",
            names(&[
                RecordState::Added,
                RecordState::Removed,
                RecordState::Changed,
            ]),
        ),
        (
            "LabelKind",
            names(&[
                LabelKind::Record,
                LabelKind::Loc,
                LabelKind::Tag,
                LabelKind::Manufacturer,
            ]),
        ),
    ];
    for (name, want) in cases {
        assert_eq!(ts_union(&ts, name), want, "dev.ts union {name}");
    }
}

/// `mock-dev.gen.json`: the mock's synthetic DataCore and datasets, read by the TS mock.
/// Regenerated with the samples (`UPDATE_CONTRACT_SAMPLES=1`). Keys keep their order: the
/// TS mock writes record JSON from it and must match the Rust mock byte for byte.
#[test]
fn mock_dev_data_matches_the_committed_file() {
    let path = app_src().join("mock-dev.gen.json");
    let text = serde_json::to_string(&mock_dev::gen_file()).unwrap() + "\n";
    if std::env::var_os("UPDATE_CONTRACT_SAMPLES").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        committed == text,
        "{} is out of date: run UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app",
        path.display()
    );
    // The samples file stays key-sorted; this one keeps order (no `sorted`).
    let _ = sorted;
}

#[test]
fn links_and_loc_in_rows() {
    let row = json!({
        "id": mock_dev::ship_guid(3),
        "recordName": "SYN_Ship_004",
        "nested": { "className": "nope", "ref": mock_dev::component_guid(1) },
        "loc": { "name": "@ui_SYN_Label_00007", "plain": "ui_SYN_Label_00015", "text": "two words" },
    });
    let ix = &mock_dev::core().index;
    let links = explorer_links(&row, &|s| {
        ix.by_guid(s)
            .or_else(|| ix.find_name(s))
            .map(|i| ix.entry(i).unwrap().guid.clone())
    });
    let ptrs: Vec<&str> = links.iter().map(|l| l.pointer.as_str()).collect();
    assert_eq!(ptrs, ["/id", "/recordName", "/nested/ref"]);
    assert_eq!(links[1].what, mock_dev::ship_guid(4));
    let b = dev_mock();
    let loc = row_loc(&row, &|k| b.stock_text(k));
    let keys: Vec<&str> = loc.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, ["@ui_SYN_Label_00007", "ui_SYN_Label_00015"]);
}

#[test]
fn group_trees_count_rows_and_filter_rows_and_records() {
    let b = dev_mock();
    let t = b
        .dev_table(DevTableRef {
            target: live(),
            source: "build:LIVE".into(),
            dataset: "gamedata.json".into(),
            table: "weapons".into(),
        })
        .unwrap();
    assert_eq!(t.group_presets[0].label, "type › subType › size");
    let g = b
        .dev_groups(groups_req(
            "build:LIVE",
            "gamedata.json",
            "weapons",
            &weapon_chain(),
        ))
        .unwrap();
    let top: Vec<(&str, u32)> = g
        .nodes
        .iter()
        .map(|n| (n.label.as_str(), n.count))
        .collect();
    assert_eq!(top, [("WeaponGun", 8), ("WeaponMining", 2)]);
    assert_eq!(g.total, 10);
    // Picking a node filters the rows to the same count.
    let mut r = rows("build:LIVE", "gamedata.json", "weapons");
    r.group = Some(GroupPick {
        by: groups_req("", "", "", &weapon_chain()).by,
        node: g.nodes[1].id.clone(),
    });
    assert_eq!(b.dev_rows(r).unwrap().total, 2);
    // Readable labels (7.14): manufacturer codes read as names, ids keep the code.
    let m = b
        .dev_groups(groups_req(
            "build:LIVE",
            "gamedata.json",
            "weapons",
            &["manufacturer"],
        ))
        .unwrap();
    let names: Vec<(&str, &str)> = m
        .nodes
        .iter()
        .map(|n| (n.label.as_str(), n.id.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("Alpha Synthetics", r#"["SYNA"]"#),
            ("Beta Works", r#"["SYNB"]"#)
        ]
    );
    // Diff tree: tables at the top (one dataset), records counted by state.
    b.dev_diff(
        DevDiffRequest {
            target: live(),
            a: "build:LIVE".into(),
            b: "build:PTU".into(),
        },
        &noop,
    )
    .unwrap();
    let d = b
        .dev_diff_groups(DevDiffGroupsRequest {
            target: live(),
            by: DiffGroupBy::Auto,
            open: vec![r#"["gamedata.json","weapons"]"#.into()],
        })
        .unwrap();
    let w = d.nodes.iter().find(|n| n.label == "weapons").unwrap();
    let c = w.diff.unwrap();
    assert_eq!((c.added, c.removed, c.changed), (1, 1, 2));
    let mining = d.nodes.iter().find(|n| n.label == "WeaponMining").unwrap();
    let recs = b
        .dev_diff_records(DevDiffRecordsRequest {
            group: Some(DiffGroupPick {
                by: DiffGroupBy::Auto,
                node: mining.id.clone(),
            }),
            limit: 100,
            ..records_req("gamedata.json", "weapons")
        })
        .unwrap();
    assert_eq!(recs.total, mining.count);
    // A node of another table holds none of this one's records.
    let other = b
        .dev_diff_records(DevDiffRecordsRequest {
            group: Some(DiffGroupPick {
                by: DiffGroupBy::Auto,
                node: r#"["gamedata.json","ships"]"#.into(),
            }),
            ..records_req("gamedata.json", "weapons")
        })
        .unwrap();
    assert_eq!(other.total, 0);
}

/// Records that read the same (AI variants of one ship) get their short id appended, in the
/// list and when searched; a unique title stays as it is.
#[test]
fn diff_records_tell_shared_titles_apart() {
    use starorg_core::dev::dataset::{Table, TableKind, columns};
    use starorg_core::dev::diff::diff_table;
    let rows = |hp: u32| -> Vec<Value> {
        [
            "AEGS_Javelin_AI_Escort",
            "AEGS_Javelin_AI_Boss",
            "AEGS_Gladius",
        ]
        .iter()
        .map(|n| json!({ "recordName": format!("EntityClassDefinition.{n}"), "hp": hp }))
        .collect()
    };
    let table = |hp| {
        let rows = rows(hp);
        Table {
            name: "ships".into(),
            kind: TableKind::Main,
            columns: columns(&rows),
            rows,
        }
    };
    let (a, b) = (table(1), table(2));
    let d = diff_table("gamedata.json", "ships", Some(&a), Some(&b), None);
    let title = |r: &starorg_core::dev::diff::DiffRecord| {
        Some(
            if r.id.contains("Javelin") {
                "Aegis Javelin"
            } else {
                "Aegis Gladius"
            }
            .to_string(),
        )
    };
    let mut req = records_req("gamedata.json", "ships");
    req.limit = 10;
    let page = page_records(&d, &req, vec![], &|_| true, &title);
    let mut labels: Vec<String> = page
        .records
        .iter()
        .filter_map(|r| r.label.clone())
        .collect();
    labels.sort();
    assert_eq!(
        labels,
        [
            "Aegis Gladius",
            "Aegis Javelin (AEGS_Javelin_AI_Boss)",
            "Aegis Javelin (AEGS_Javelin_AI_Escort)"
        ]
    );
    req.query = Some("boss".into());
    let page = page_records(&d, &req, vec![], &|_| true, &title);
    assert_eq!(page.records.len(), 1);
    assert_eq!(
        page.records[0].label.as_deref(),
        Some("Aegis Javelin"),
        "unique within the result"
    );
}
