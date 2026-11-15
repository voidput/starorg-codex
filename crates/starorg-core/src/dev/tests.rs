use super::dataset::*;
use super::diff::*;
use super::explorer::*;
use super::record::*;
use serde_json::{Value, json};
use std::path::PathBuf;

fn e(guid: &str, name: &str, ty: &str, path: &str, main: bool) -> Entry {
    Entry {
        guid: guid.into(),
        name: name.into(),
        struct_type: ty.into(),
        path: path.into(),
        main,
        tag: None,
    }
}

fn index() -> ExplorerIndex {
    ExplorerIndex::new(vec![
        e(
            "aaaaaaaa-0000-0000-0000-000000000001",
            "SYN_Cooler_S1",
            "EntityClassDefinition",
            "libs/foundry/records/entities/scitem/ships/cooler/syn_cooler_s1.xml",
            true,
        ),
        e(
            "aaaaaaaa-0000-0000-0000-000000000002",
            "SYN_Cooler_S2",
            "EntityClassDefinition",
            "libs/foundry/records/entities/scitem/ships/cooler/syn_cooler_s2.xml",
            true,
        ),
        e(
            "bbbbbbbb-0000-0000-0000-000000000003",
            "Cooler",
            "Tag",
            "libs/foundry/records/tagdatabase/tagdatabase.xml",
            false,
        ),
        e(
            "bbbbbbbb-0000-0000-0000-000000000004",
            "TagDatabase",
            "TagDatabase",
            "libs/foundry/records/tagdatabase/tagdatabase.xml",
            true,
        ),
        e(
            "cccccccc-0000-0000-0000-000000000005",
            "SYN_Shield_Cooler_Mount",
            "EntityClassDefinition",
            "libs/foundry/records/entities/scitem/ships/shield/syn_shield_cooler_mount.xml",
            true,
        ),
        e(
            "dddddddd-0000-0000-0000-000000000006",
            "Outside",
            "Misc",
            "other/outside.xml",
            true,
        ),
    ])
}

#[test]
fn index_counts_types_and_main_records() {
    let ix = index();
    assert_eq!(ix.len(), 6);
    assert_eq!(ix.main_count(), 5);
    let types = ix.types();
    let ecd = types
        .iter()
        .find(|t| t.name == "EntityClassDefinition")
        .unwrap();
    assert_eq!((ecd.records, ecd.main), (3, 3));
    let tag = types.iter().find(|t| t.name == "Tag").unwrap();
    assert_eq!((tag.records, tag.main), (1, 0));
    // Main only hides types without a main record.
    let root = ix.tree(TreeRoot::Type, "", 0, 500, true);
    assert!(root.nodes.iter().all(|n| n.label != "Tag"));
    let all = ix.tree(TreeRoot::Type, "", 0, 500, false);
    assert!(all.nodes.iter().any(|n| n.label == "Tag" && n.count == 1));
    assert_eq!(all.total as usize, all.nodes.len());
}

#[test]
fn type_tree_pages_records_by_name() {
    let ix = index();
    let p = ix.tree(TreeRoot::Type, "t:EntityClassDefinition", 0, 2, true);
    assert_eq!(p.total, 3);
    assert_eq!(p.nodes.len(), 2);
    assert_eq!(p.nodes[0].label, "SYN_Cooler_S1");
    assert_eq!(p.nodes[0].kind, NodeKind::Record);
    assert_eq!(p.nodes[0].id, "r:aaaaaaaa-0000-0000-0000-000000000001");
    let rest = ix.tree(TreeRoot::Type, "t:EntityClassDefinition", 2, 2, true);
    assert_eq!(rest.nodes.len(), 1);
    assert_eq!(rest.offset, 2);
    assert!(
        ix.tree(TreeRoot::Type, "bogus", 0, 10, true)
            .nodes
            .is_empty()
    );
}

#[test]
fn path_tree_lists_folders_then_records() {
    let ix = index();
    let root = ix.tree(TreeRoot::Path, "", 0, 500, true);
    let labels: Vec<&str> = root.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["entities", "other", "tagdatabase"]);
    assert_eq!(root.nodes[0].count, 3);
    assert_eq!(root.nodes[0].id, "p:entities/");
    let ships = ix.tree(TreeRoot::Path, "p:entities/scitem/ships/", 0, 500, true);
    let labels: Vec<&str> = ships.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["cooler", "shield"]);
    let coolers = ix.tree(
        TreeRoot::Path,
        "p:entities/scitem/ships/cooler/",
        0,
        500,
        true,
    );
    assert!(coolers.nodes.iter().all(|n| n.kind == NodeKind::Record));
    assert_eq!(coolers.total, 2);
    // Sub-records share their main record's file.
    let tags = ix.tree(TreeRoot::Path, "p:tagdatabase/", 0, 500, false);
    assert_eq!(tags.total, 2);
    let tags_main = ix.tree(TreeRoot::Path, "p:tagdatabase/", 0, 500, true);
    assert_eq!(tags_main.total, 1);
}

#[test]
fn search_ranks_and_filters() {
    let ix = index();
    let r = ix.search("cooler", 500);
    let names: Vec<&str> = r.hits.iter().map(|h| h.name.as_str()).collect();
    // Exact name first, then name substrings (shorter first), path matches last.
    assert_eq!(names[0], "Cooler");
    assert_eq!(&names[1..3], ["SYN_Cooler_S1", "SYN_Cooler_S2"]);
    assert_eq!(names[3], "SYN_Shield_Cooler_Mount");
    assert_eq!(r.total, 4);
    // Every token must appear (name or path).
    let r = ix.search("cooler s2", 500);
    assert_eq!(r.total, 1);
    // A path token.
    let r = ix.search("shield mount", 500);
    assert_eq!(r.hits[0].name, "SYN_Shield_Cooler_Mount");
    // type: filter.
    let r = ix.search("type:Tag cooler", 500);
    assert_eq!(r.total, 1);
    assert_eq!(r.hits[0].struct_type, "Tag");
    let r = ix.search("type:entityclassdefinition", 500);
    assert_eq!(r.total, 3);
    // GUID prefix.
    let r = ix.search("bbbbbbbb-0000", 500);
    assert_eq!(r.total, 2);
    assert!(r.hits.iter().all(|h| h.guid.starts_with("bbbbbbbb")));
    // Subsequence.
    let r = ix.search("sncls1", 500);
    assert_eq!(r.hits[0].name, "SYN_Cooler_S1");
    assert_eq!(ix.search("cooler", 2).hits.len(), 2);
    assert_eq!(ix.search("", 500).total, 0);
}

#[test]
fn find_by_guid_path_or_name() {
    let ix = index();
    let n = |i: Option<usize>| ix.entry(i.unwrap()).unwrap().name.clone();
    assert_eq!(
        n(ix.find("AAAAAAAA-0000-0000-0000-000000000002")),
        "SYN_Cooler_S2"
    );
    assert_eq!(
        n(ix.find("entities/scitem/ships/cooler/syn_cooler_s1")),
        "SYN_Cooler_S1"
    );
    assert_eq!(
        n(ix.find("libs/foundry/records/entities/scitem/ships/cooler/SYN_COOLER_S1.xml")),
        "SYN_Cooler_S1"
    );
    // A shared file opens its main record.
    assert_eq!(n(ix.find("tagdatabase/tagdatabase.json")), "TagDatabase");
    assert_eq!(n(ix.find("syn_cooler_s2")), "SYN_Cooler_S2");
    assert!(ix.find("nothing").is_none());
    assert!(is_guid("aaaaaaaa-0000-0000-0000-000000000002"));
    assert!(!is_guid("aaaa"));
    assert!(guid_like("aaaa-00"));
}

#[test]
fn small_records_come_whole_and_large_ones_are_cut() {
    let small = json!({"a": {"b": {"c": {"d": 1}}}});
    let (s, cut_any) = view_json(&small, "", None).unwrap();
    assert!(!cut_any);
    assert_eq!(s, r#"{"a":{"b":{"c":{"d":1}}}}"#);
    // Asked for a depth: cut there.
    let (s, cut_any) = view_json(&small, "", Some(2)).unwrap();
    assert!(cut_any);
    let v: Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["a"]["b"][CUT_KEY]["kind"], "object");
    assert_eq!(v["a"]["b"][CUT_KEY]["len"], 1);
    // A pointer opens a sub-node.
    let (s, _) = view_json(&small, "/a/b", None).unwrap();
    assert_eq!(s, r#"{"c":{"d":1}}"#);
    assert!(view_json(&small, "/x", None).is_err());
    // 1.5 MiB: cut at depth 3 and under the cap.
    let big: Vec<Value> = (0..3000)
        .map(|i| json!({"i": i, "deep": {"text": "x".repeat(500)}}))
        .collect();
    let big = json!({"_RecordValue_": {"items": big}});
    assert!(serde_json::to_string(&big).unwrap().len() > MAX_REPLY);
    let (s, cut_any) = view_json(&big, "", None).unwrap();
    assert!(cut_any);
    assert!(s.len() <= MAX_REPLY, "{}", s.len());
    // References are never cut.
    let r = json!({"x": {"y": {"z": {"_ref": "g", "name": "n", "type": "T", "path": "p"}}}});
    let (s, _) = view_json(&r, "", Some(1)).unwrap();
    assert!(s.contains("\"_cut\""));
    let (c, _) = cut(&r["x"]["y"], 0);
    assert!(c.get(CUT_KEY).is_some());
    let (c, _) = cut(&r["x"]["y"]["z"], 0);
    assert!(is_ref(&c));
}

#[test]
fn loc_keys_are_unique_at_strings() {
    let v = json!({"Name": "@item_NameA", "list": ["@item_NameA", "@ui_B", "@", "plain"]});
    assert_eq!(loc_keys(&v), ["@item_NameA", "@ui_B"]);
    let stock = crate::ini::StockIni::parse(b"item_NameA=Alpha\r\nother=x\r\n".to_vec());
    let r = resolve_loc(&loc_keys(&v), Some(&stock));
    assert_eq!(
        r,
        [LocText {
            key: "@item_NameA".into(),
            text: "Alpha".into()
        }]
    );
    assert!(resolve_loc(&loc_keys(&v), None).is_empty());
}

// ---------------------------------------------------------------------------------------
// Datasets: shape-only copies of the 30 sc-gamedata files (synthetic values)
// ---------------------------------------------------------------------------------------

fn shapes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dev-shapes")
}

fn shape(file: &str) -> Dataset {
    let bytes = std::fs::read(shapes_dir().join(file)).unwrap();
    Dataset::parse(file, &bytes).unwrap()
}

fn kinds(d: &Dataset) -> Vec<(String, TableKind)> {
    d.tables.iter().map(|t| (t.name.clone(), t.kind)).collect()
}

#[test]
fn every_shape_file_has_tables_and_metadata() {
    let mut n = 0;
    for f in std::fs::read_dir(shapes_dir()).unwrap() {
        let f = f.unwrap().path();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let d = Dataset::parse(&name, &std::fs::read(&f).unwrap()).unwrap();
        n += 1;
        assert!(
            d.tables.iter().any(|t| t.kind != TableKind::Tree),
            "{name}: {:?}",
            kinds(&d)
        );
        assert!(
            d.info.meta.iter().any(|m| m.key == "_source"),
            "{name} has _source"
        );
        assert!(
            d.tables
                .iter()
                .all(|t| !t.name.starts_with('_') && t.name != "summary")
        );
        for t in &d.tables {
            assert!(t.rows.iter().all(Value::is_object), "{name}/{}", t.name);
        }
    }
    assert_eq!(n, 30);
}

#[test]
fn shape_rules_match_the_spec_examples() {
    let w = shape("game-weapons.json");
    assert_eq!(
        kinds(&w),
        [
            ("shipWeapons".into(), TableKind::Main),
            ("fpsWeapons".into(), TableKind::Main),
            ("missiles".into(), TableKind::Main),
        ]
    );
    assert!(w.info.facts.len() >= 2, "summary facts");
    assert_eq!(
        w.info
            .meta
            .iter()
            .map(|m| m.key.as_str())
            .collect::<Vec<_>>(),
        ["_source", "_extracted", "_build"]
    );
    // An object of scalars in a file without array tables is the main table.
    let s = shape("game-strings-english.json");
    let strings = s.table("strings").unwrap();
    assert_eq!(strings.kind, TableKind::Main);
    assert_eq!(
        strings
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["key", "value"]
    );
    // ... and an index beside array tables.
    let m = shape("game-manufacturers.json");
    assert_eq!(m.table("byCode").unwrap().kind, TableKind::Index);
    assert_eq!(m.table("manufacturers").unwrap().kind, TableKind::Main);
    let ships = shape("game-ships.json");
    assert_eq!(
        ships.table("byManufacturer").unwrap().kind,
        TableKind::Index
    );
    let ore = shape("game-mining-spawns.json");
    assert_eq!(ore.table("oreLocations").unwrap().kind, TableKind::Index);
    // Arrays of arrays with `columns` use those names.
    let si = shape("game-search-index.json");
    let items = si.table("items").unwrap();
    assert_eq!(items.kind, TableKind::Main);
    assert_eq!(
        items
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["key", "name", "maker", "type", "family", "noise"]
    );
    assert!(si.table("columns").is_none());
    // Real rows are ragged (widths 4, 5 and 6 against 6 names): still one row per entry,
    // the short rows just lack the trailing cells.
    assert_eq!(items.rows.len(), 3);
    assert_eq!(items.rows[1].as_object().unwrap().len(), 4);
    assert!(items.rows[1].get("family").is_none());
    assert_eq!(items.rows[2]["family"], json!(87));
    let ships = si.table("ships").unwrap();
    assert_eq!(ships.kind, TableKind::Main);
    assert_eq!(ships.rows.len(), 3);
    assert_eq!(ships.rows[0]["type"], json!(92));
    assert_eq!(ships.rows[2]["noise"], json!(98));
    // Arrays of arrays without `columns` get columns `0..n` (rows wider than the list too).
    let aoa = Dataset::parse(
        "grid.json",
        br#"{"columns":["a"],"pts":[[1,2],[3]],"top":{"x":1}}"#,
    )
    .unwrap();
    let pts = aoa.table("pts").unwrap();
    assert_eq!(pts.kind, TableKind::Main);
    assert_eq!(
        pts.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "1"]
    );
    let bare = Dataset::parse("grid.json", br#"{"pts":[[1,2,3],[4]]}"#).unwrap();
    assert_eq!(
        bare.table("pts")
            .unwrap()
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["0", "1", "2"]
    );
    let top = Dataset::parse("grid.json", br#"[[1,"a"],[2,"b"]]"#).unwrap();
    assert_eq!(top.table("grid").unwrap().rows[1]["1"], json!("b"));
    // Arrays of scalars are JSON-only.
    assert_eq!(si.table("makers").unwrap().kind, TableKind::Tree);
    // Top-level array files are one table.
    let arr = Dataset::parse("list.json", br#"[{"a":1},{"a":2,"b":"x"}]"#).unwrap();
    assert_eq!(kinds(&arr), [("list".into(), TableKind::Main)]);
    // UEX files are just JSON in the folder (Q-D7).
    let uex = shape("uex-item-prices.json");
    assert_eq!(uex.table("prices").unwrap().kind, TableKind::Main);
    assert!(uex.info.meta.iter().any(|m| m.key == "_fetched"));
}

#[test]
fn columns_are_typed_in_first_seen_order() {
    let rows = vec![
        json!({"name": "a", "n": 1, "flag": true, "obj": {"x": 1}, "list": [1]}),
        json!({"name": "b", "n": 2.5, "extra": "late", "mixed": 1}),
        json!({"name": "c", "n": null, "mixed": "x"}),
    ];
    let cols = columns(&rows);
    let get = |n: &str| cols.iter().find(|c| c.name == n).unwrap();
    assert_eq!(
        cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["name", "n", "flag", "obj", "list", "extra", "mixed"]
    );
    assert_eq!(get("name").kind, ColType::Text);
    assert_eq!(get("n").kind, ColType::Number);
    assert_eq!(get("n").filled, 2);
    assert_eq!((get("n").min, get("n").max), (Some(1.0), Some(2.5)));
    assert_eq!(get("flag").kind, ColType::Bool);
    assert_eq!(get("obj").kind, ColType::Object);
    assert_eq!(get("list").kind, ColType::List);
    assert_eq!(get("mixed").kind, ColType::Mixed);
    assert_eq!(get("name").distinct.as_ref().unwrap().len(), 3);
    // More than 50 distinct values: no chips.
    let many: Vec<Value> = (0..60).map(|i| json!({"t": format!("v{i}")})).collect();
    assert!(columns(&many)[0].distinct.is_none());
    assert_eq!(default_columns(&cols)[0], "name");
    assert!(!default_columns(&cols).contains(&"obj".to_string()));
}

fn sample_table() -> Table {
    let rows: Vec<Value> = (0..30)
        .map(|i| {
            json!({
                "name": format!("Item {i:02}"),
                "size": i % 4,
                "maker": (["SYNA", "SYNB", "SYNC"][i % 3]),
                "desc": if i == 7 { Value::String("y".repeat(300)) } else { Value::Null },
                "ports": [i, i + 1],
            })
        })
        .collect();
    let columns = columns(&rows);
    Table {
        name: "items".into(),
        kind: TableKind::Main,
        rows,
        columns,
    }
}

#[test]
fn rows_search_filter_sort_and_page() {
    let t = sample_table();
    let all = query_rows(
        &t,
        &RowsQuery {
            limit: 10,
            ..Default::default()
        },
        500,
        None,
    );
    assert_eq!((all.total, all.all, all.rows.len()), (30, 30, 10));
    assert_eq!(all.columns, ["name", "size", "maker", "desc"]);
    let q = |q: RowsQuery| query_rows(&t, &q, 500, None);
    let r = q(RowsQuery {
        search: "item 1".into(),
        limit: 50,
        ..Default::default()
    });
    assert_eq!(r.total, 10);
    let r = q(RowsQuery {
        filters: vec![RowFilter {
            column: "maker".into(),
            values: vec!["SYNB".into()],
            ..Default::default()
        }],
        limit: 50,
        ..Default::default()
    });
    assert_eq!(r.total, 10);
    let r = q(RowsQuery {
        filters: vec![RowFilter {
            column: "size".into(),
            min: Some(2.0),
            max: Some(3.0),
            ..Default::default()
        }],
        limit: 50,
        ..Default::default()
    });
    assert_eq!(r.total, 14);
    let r = q(RowsQuery {
        filters: vec![RowFilter {
            column: "desc".into(),
            has: Some(true),
            ..Default::default()
        }],
        limit: 50,
        ..Default::default()
    });
    assert_eq!(r.total, 1);
    let cell = r.rows[0].cells[3].as_ref().unwrap();
    assert_eq!(cell.chars().count(), MAX_CELL + 1, "cut with an ellipsis");
    let r = q(RowsQuery {
        sort: Some(RowSort {
            column: "size".into(),
            desc: true,
        }),
        columns: vec!["name".into(), "size".into(), "ports".into()],
        limit: 3,
        offset: 1,
        ..Default::default()
    });
    assert_eq!(r.rows[0].cells[1].as_deref(), Some("3"));
    assert_eq!(r.rows[0].cells[2].as_deref(), Some("[2]"));
    assert_eq!(r.offset, 1);
    // Pages are capped.
    let r = query_rows(
        &t,
        &RowsQuery {
            limit: 9999,
            ..Default::default()
        },
        20,
        None,
    );
    assert_eq!(r.rows.len(), 20);
}

#[test]
fn identity_is_the_first_unique_column() {
    let a = vec![
        json!({"id": "1", "name": "x"}),
        json!({"id": "1", "name": "y"}),
    ];
    assert_eq!(identity_column(&a, None).as_deref(), Some("name"));
    let b = vec![json!({"code": "A"}), json!({"code": "B"})];
    assert_eq!(identity_column(&b, None).as_deref(), Some("code"));
    assert_eq!(identity_column(&[json!({"z": 1})], None), None);
    assert_eq!(pick_identity(&a, &b, None), None);
    assert_eq!(pick_identity(&b, &b, Some("code")).as_deref(), Some("code"));
    assert_eq!(pick_identity(&b, &b, Some("nope")), None);
}

fn t(rows: Vec<Value>) -> Table {
    let columns = columns(&rows);
    Table {
        name: "t".into(),
        kind: TableKind::Main,
        rows,
        columns,
    }
}

#[test]
fn diff_counts_and_orders_records() {
    let a = t(vec![
        json!({"recordName": "a", "dps": 100, "ports": [{"name": "p1", "size": 1}, {"name": "p2", "size": 2}]}),
        json!({"recordName": "b", "dps": 5}),
        json!({"recordName": "c", "dps": 1}),
        json!({"recordName": "gone"}),
    ]);
    let b = t(vec![
        // Reordered ports, one size changed.
        json!({"recordName": "a", "dps": 110, "ports": [{"name": "p2", "size": 3}, {"name": "p1", "size": 1}]}),
        json!({"recordName": "b", "dps": 5}),
        json!({"recordName": "c", "dps": 2}),
        json!({"recordName": "new", "dps": 0}),
    ]);
    let d = diff_table("game-x.json", "t", Some(&a), Some(&b), None);
    let s = &d.summary;
    assert_eq!(s.identity.as_deref(), Some("recordName"));
    assert_eq!(
        (s.added, s.removed, s.changed, s.same),
        (1, 1, 2, 1),
        "{s:?}"
    );
    assert_eq!(s.presence, Presence::Both);
    assert_eq!(d.records[0].id, "a", "most changed fields first");
    assert_eq!(d.records[0].fields, ["/dps", "/ports/[name=p2]/size"]);
    assert_eq!(d.records[1].id, "c");
    assert_eq!(d.records[2].state, RecordState::Added);
    assert_eq!(d.records[3].state, RecordState::Removed);
    let f = field_changes(&a.rows[0], &b.rows[0]);
    assert_eq!(f[0].path, "/dps");
    assert_eq!(f[0].delta, Some(10.0));
    assert_eq!(f[0].percent, Some(10.0));
    assert_eq!(f[1].old, Some(json!(2)));
    assert_eq!(f[1].new, Some(json!(3)));
    // By position when no identity exists; a table on one side only.
    let pa = t(vec![json!({"v": 1}), json!({"v": 2})]);
    let pb = t(vec![json!({"v": 1}), json!({"v": 3}), json!({"v": 4})]);
    let d = diff_table("f", "t", Some(&pa), Some(&pb), None);
    assert_eq!(d.summary.identity, None);
    assert_eq!(
        (d.summary.added, d.summary.changed, d.summary.same),
        (1, 1, 1)
    );
    assert_eq!(d.records[0].id, "#1");
    let only = diff_table("f", "t", Some(&pa), None, None);
    assert_eq!(only.summary.presence, Presence::OnlyA);
    assert_eq!(only.summary.removed, 2);
}

#[test]
fn source_diff_sorts_tables_by_weight() {
    let a = Dataset::parse(
        "game-a.json",
        br#"{"_source":"x","_extracted":"1","items":[{"id":"1","v":1},{"id":"2","v":2}],"same":[{"id":"z"}]}"#,
    )
    .unwrap();
    let b = Dataset::parse(
        "game-a.json",
        br#"{"_source":"y","_extracted":"2","items":[{"id":"1","v":9},{"id":"3","v":2}],"same":[{"id":"z"}],"extra":[{"id":"q"}]}"#,
    )
    .unwrap();
    let c = Dataset::parse("game-only-a.json", br#"{"t":[{"id":"1"}]}"#).unwrap();
    let calls = std::sync::Mutex::new(0);
    let out = diff_sources(
        &[("game-a.json".into(), &a), ("game-only-a.json".into(), &c)],
        &[("game-a.json".into(), &b)],
        &|_, _| *calls.lock().unwrap() += 1,
    );
    let rows: Vec<(String, u32)> = out
        .iter()
        .map(|d| (d.summary.table.clone(), d.summary.weight()))
        .collect();
    assert_eq!(rows[0], ("items".into(), 3));
    assert!(rows.contains(&("same".into(), 0)));
    assert!(
        out.iter()
            .any(|d| d.summary.table == "extra" && d.summary.presence == Presence::OnlyB)
    );
    assert!(
        out.iter()
            .any(|d| d.summary.dataset == "game-only-a.json"
                && d.summary.presence == Presence::OnlyA)
    );
    assert!(*calls.lock().unwrap() >= 2);
}

#[test]
fn oversized_and_malformed_files_are_refused() {
    assert!(Dataset::parse("x.json", b"{not json").is_err());
    // Nesting deeper than serde_json's limit of 128.
    let deep = "[".repeat(200) + &"]".repeat(200);
    assert!(Dataset::parse("deep.json", deep.as_bytes()).is_err());
}

// ---------------------------------------------------------------------------------------
// Readable labels (addendum 7.12)
// ---------------------------------------------------------------------------------------

mod readable_labels {
    use super::super::readable::*;
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    const COOLER: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const TAG: &str = "aaaaaaaa-0000-4000-8000-0000000000a1";
    const NAMELESS: &str = "aaaaaaaa-0000-4000-8000-000000000002";
    const UNKNOWN: &str = "aaaaaaaa-0000-4000-8000-0000000000ff";

    fn text(pairs: &[(&str, &str)]) -> TextFn {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_string()))
            .collect();
        Arc::new(move |k: &str| m.get(&k.to_lowercase()).cloned())
    }

    fn lookup() -> MapLookup {
        let mut m = MapLookup::new();
        m.add(KnownRecord {
            guid: Some(COOLER.into()),
            name: "EntityClassDefinition.COOL_SYN_S01_Bracer".into(),
            kind: LabelKind::Record,
            display: Some("@item_NameCOOL_SYN_S01_Bracer".into()),
        });
        m.add(KnownRecord {
            guid: Some(NAMELESS.into()),
            name: "SEntityDensityClass.VehicleItems".into(),
            kind: LabelKind::Record,
            display: None,
        });
        m.add(KnownRecord {
            guid: Some(TAG.into()),
            name: format!("Tag.{TAG}"),
            kind: LabelKind::Tag,
            display: Some("Cooler".into()),
        });
        m.with_stock(text(&[
            ("item_NameCOOL_SYN_S01_Bracer", "Bracer"),
            ("item_DescCOOL_SYN_S01_Bracer", "A cooler."),
            ("manufacturer_NameSYNA", "Alpha Synthetics"),
            ("LOC_EMPTY", ""),
        ]))
        .with_enhanced(text(&[(
            "item_DescCOOL_SYN_S01_Bracer",
            "A cooler.\\nGrade: C",
        )]))
    }

    #[test]
    fn guids_read_as_display_names_with_the_short_name() {
        let l = lookup();
        let r = label(&l, COOLER, None).unwrap();
        assert_eq!(r.kind, LabelKind::Record);
        assert_eq!(r.text, "Bracer");
        assert_eq!(r.detail.as_deref(), Some("COOL_SYN_S01_Bracer"));
        // Upper case GUIDs too.
        assert_eq!(
            label(&l, &COOLER.to_uppercase(), None).unwrap().text,
            "Bracer"
        );
        // No display name: the short record name.
        let r = label(&l, NAMELESS, None).unwrap();
        assert_eq!((r.text.as_str(), r.detail), ("VehicleItems", None));
        // Names, full or short, case-insensitive.
        assert_eq!(
            label(&l, "cool_syn_s01_bracer", Some("item")).unwrap().text,
            "Bracer"
        );
    }

    #[test]
    fn loc_keys_read_as_stock_text_and_mark_enhanced_text() {
        let l = lookup();
        let r = label(&l, "@item_NameCOOL_SYN_S01_Bracer", None).unwrap();
        assert_eq!(
            (r.kind, r.text.as_str(), r.enhanced),
            (LabelKind::Loc, "Bracer", None)
        );
        let r = label(&l, "@item_DescCOOL_SYN_S01_Bracer", None).unwrap();
        assert_eq!(r.text, "A cooler.");
        assert_eq!(r.enhanced.as_deref(), Some("A cooler.\\nGrade: C"));
        // Empty stock text stays raw (no blank label).
        assert_eq!(label(&l, "@LOC_EMPTY", None), None);
        // Bare keys only where the lookup allows them (datasets).
        assert_eq!(label(&l, "item_NameCOOL_SYN_S01_Bracer", None), None);
        let bare = lookup().with_bare_keys(true);
        assert_eq!(
            label(&bare, "item_NameCOOL_SYN_S01_Bracer", None)
                .unwrap()
                .text,
            "Bracer"
        );
    }

    #[test]
    fn tags_and_manufacturers() {
        let l = lookup();
        let r = label(&l, TAG, None).unwrap();
        assert_eq!(
            (r.kind, r.text.as_str(), r.detail),
            (LabelKind::Tag, "Cooler", None)
        );
        let r = label(&l, "SYNA", Some("manufacturer")).unwrap();
        assert_eq!(
            (r.kind, r.text.as_str()),
            (LabelKind::Manufacturer, "Alpha Synthetics")
        );
        // Lower case codes (ships), and only under a manufacturer key.
        assert_eq!(
            label(&l, "syna", Some("manufacturer")).unwrap().text,
            "Alpha Synthetics"
        );
        assert_eq!(label(&l, "SYNA", Some("grade")), None);
    }

    #[test]
    fn unknown_values_stay_raw() {
        let l = lookup();
        for (raw, key) in [
            (UNKNOWN, None),
            ("@item_NameNOPE", None),
            ("SYNZ", Some("manufacturer")),
            ("Some prose with spaces", None),
            ("", None),
        ] {
            assert_eq!(label(&l, raw, key), None, "{raw}");
        }
        assert_eq!(label(&NoLookup, COOLER, None), None);
    }

    #[test]
    fn record_json_labels_refs_guids_and_keys() {
        let l = lookup();
        let v = json!({
            "_RecordId_": COOLER,
            "_RecordValue_": {
                "Localization": { "Name": "@item_NameCOOL_SYN_S01_Bracer", "ShortName": "@LOC_EMPTY" },
                "density": { "_ref": NAMELESS, "name": "SEntityDensityClass.VehicleItems", "type": "SEntityDensityClass", "path": "x" },
                "tags": [{ "_RecordId_": TAG, "_RecordName_": format!("Tag.{TAG}") }, { "_RecordId_": UNKNOWN }],
                "Manufacturer": "SYNA",
            },
        });
        let m = labels_in(&Strict(&l), &v);
        assert_eq!(m[COOLER].text, "Bracer");
        assert_eq!(m[NAMELESS].text, "VehicleItems");
        assert_eq!(m[TAG].text, "Cooler");
        assert_eq!(m["@item_NameCOOL_SYN_S01_Bracer"].text, "Bracer");
        assert_eq!(m["SYNA"].kind, LabelKind::Manufacturer);
        assert!(!m.contains_key(UNKNOWN));
        assert!(!m.contains_key("@LOC_EMPTY"));
        assert!(
            !m.contains_key("SEntityDensityClass.VehicleItems"),
            "inside a ref only the GUID"
        );
        assert_eq!(
            display_of(&v),
            Some((LabelKind::Record, "@item_NameCOOL_SYN_S01_Bracer".into()))
        );
        let tag = json!({ "_RecordValue_": { "_Type_": "Tag", "tagName": "Cooler" } });
        assert_eq!(display_of(&tag), Some((LabelKind::Tag, "Cooler".into())));
        assert_eq!(display_of(&json!({ "_RecordValue_": { "Size": 1 } })), None);
    }

    #[test]
    fn rows_titles_paths_units_and_label_search() {
        let l = lookup().with_bare_keys(true);
        let row = json!({ "recordName": "EntityClassDefinition.COOL_SYN_S01_Bracer", "loc": { "name": "@item_NameCOOL_SYN_S01_Bracer" }, "manufacturer": "SYNA", "health": 500 });
        assert_eq!(
            row_title(&l, &row, Some("recordName")).as_deref(),
            Some("Bracer")
        );
        let by_id = json!({ "className": "COOL_SYN_S01_Bracer" });
        assert_eq!(
            row_title(&l, &by_id, Some("className")).as_deref(),
            Some("Bracer")
        );
        let dump = json!({ "className": "SYN_Unknown_01", "name": "Plain Name" });
        assert_eq!(
            row_title(&l, &dump, Some("className")).as_deref(),
            Some("Plain Name")
        );
        assert_eq!(row_title(&l, &json!({ "id": UNKNOWN }), Some("id")), None);
        assert_eq!(
            path_label(&l, "/loadout/[className=COOL_SYN_S01_Bracer]/size").as_deref(),
            Some("loadout › Bracer › size")
        );
        assert_eq!(path_label(&l, "/stats/health"), None);
        assert_eq!(
            group_of(&l, "/loadout/[className=COOL_SYN_S01_Bracer]/size").as_deref(),
            Some("loadout › Bracer")
        );
        assert_eq!(group_of(&l, "/stats/health").as_deref(), Some("stats"));
        assert_eq!(group_of(&l, "/health"), None);
        assert_eq!(unit_of("/flight/scmSpeed"), Some("m/s"));
        assert_eq!(unit_of("/weapons/[name=x]/dps"), Some("dmg/s"));
        assert_eq!(unit_of("/size"), None);
        // Search finds a row by what its cells read as; the page carries labels and titles.
        let rows = vec![
            row.clone(),
            json!({ "recordName": "Other.Thing_01", "manufacturer": "SYNB", "health": 1 }),
        ];
        let cols = columns(&rows);
        let t = Table {
            name: "c".into(),
            kind: TableKind::Main,
            rows,
            columns: cols,
        };
        let q = RowsQuery {
            search: "alpha synth".into(),
            limit: 10,
            ..Default::default()
        };
        assert_eq!(query_rows(&t, &q, 500, None).total, 0);
        let p = query_rows(&t, &q, 500, Some(&l));
        assert_eq!(p.total, 1);
        let r = &p.rows[0];
        assert_eq!(r.title.as_deref(), Some("Bracer"));
        let at = p.columns.iter().position(|c| c == "manufacturer").unwrap();
        assert_eq!(r.labels[at].as_ref().unwrap().text, "Alpha Synthetics");
        assert_eq!(r.cells[at].as_deref(), Some("SYNA"), "the raw cell stays");
        let q = RowsQuery {
            search: "bracer".into(),
            limit: 10,
            ..Default::default()
        };
        assert_eq!(query_rows(&t, &q, 500, Some(&l)).total, 1);
    }

    #[test]
    fn field_changes_read_on_their_own_side() {
        let a = lookup();
        let mut b = MapLookup::new();
        b.add(KnownRecord {
            guid: Some(COOLER.into()),
            name: "EntityClassDefinition.COOL_SYN_S01_Bracer".into(),
            kind: LabelKind::Record,
            display: Some("@item_NameCOOL_SYN_S01_Bracer".into()),
        });
        let b = b.with_stock(text(&[("item_NameCOOL_SYN_S01_Bracer", "Bracer Mk II")]));
        let mut c = field_changes(
            &json!({ "cooler": COOLER, "health": 100, "mass": 2 }),
            &json!({ "cooler": COOLER.to_uppercase(), "health": 120, "mass": 2 }),
        );
        annotate_changes(&mut c, &a, &b);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].path, "/cooler");
        assert_eq!(c[0].old_label.as_ref().unwrap().text, "Bracer");
        assert_eq!(c[0].new_label.as_ref().unwrap().text, "Bracer Mk II");
        assert_eq!(c[1].unit.as_deref(), Some("HP"));
        assert_eq!(c[1].old_label, None);
    }
}

/// Real data, read only (addendum 7.12): two builds' GameData (`gamedata.json`) and stock
/// `global.ini` in `STARORG_READABLE_A` and `STARORG_READABLE_B` (`DIR/gamedata.json`,
/// `DIR/base.ini`). Diffs them as the app does and prints titled records, readable field
/// changes and dataset cells, with timings and lookup sizes. `--ignored --nocapture`.
#[test]
#[ignore = "needs real GameData in STARORG_READABLE_A and STARORG_READABLE_B"]
fn real_builds_read_as_names() {
    use super::readable::{self, MapLookup};
    use crate::ini::StockIni;
    use crate::model::GameData;
    use std::sync::Arc;
    let side = |var: &str| {
        let dir = PathBuf::from(std::env::var(var).expect(var));
        let stock = Arc::new(StockIni::parse(
            std::fs::read(dir.join("base.ini")).unwrap(),
        ));
        let gd: GameData =
            serde_json::from_slice(&std::fs::read(dir.join("gamedata.json")).unwrap()).unwrap();
        let t = std::time::Instant::now();
        let s = stock.clone();
        let l = MapLookup::from_gamedata(&gd)
            .with_stock(Arc::new(move |k: &str| s.value(k).map(str::to_string)))
            .with_bare_keys(true);
        eprintln!("{var}: lookup of {} records in {:?}", l.len(), t.elapsed());
        let mut v = serde_json::to_value(&gd).unwrap();
        for k in ["schema", "build", "datacore"] {
            let x = v.as_object_mut().unwrap().shift_remove(k);
            if let Some(x) = x {
                v[format!("_{k}")] = x;
            }
        }
        (infer("gamedata.json", 0, v), l)
    };
    let (da, la) = side("STARORG_READABLE_A");
    let (db, lb) = side("STARORG_READABLE_B");
    let t = std::time::Instant::now();
    let tables = diff_sources(
        &[("gamedata.json".into(), &da)],
        &[("gamedata.json".into(), &db)],
        &|_, _| {},
    );
    eprintln!("diff in {:?}", t.elapsed());
    let mut titled = 0;
    let mut all = 0;
    for td in tables.iter().filter(|t| t.summary.weight() > 0) {
        let (ta, tb) = (da.table(&td.summary.table), db.table(&td.summary.table));
        let id = td.summary.identity.as_deref();
        eprintln!(
            "== {} +{} -{} ~{} (by {:?})",
            td.summary.table, td.summary.added, td.summary.removed, td.summary.changed, id
        );
        for (k, r) in td.records.iter().enumerate() {
            fn row(t: Option<&Table>, i: Option<u32>) -> Option<&Value> {
                t?.rows.get(i? as usize)
            }
            let title = row(tb, r.b)
                .and_then(|x| readable::row_title(&lb, x, id))
                .or_else(|| row(ta, r.a).and_then(|x| readable::row_title(&la, x, id)));
            all += 1;
            titled += usize::from(title.is_some());
            if k < 4 {
                eprintln!(
                    "  {:?} {} ({})",
                    r.state,
                    title.as_deref().unwrap_or("-"),
                    r.id
                );
                if r.state == RecordState::Changed {
                    let mut c = field_changes(
                        row(ta, r.a).unwrap_or(&Value::Null),
                        row(tb, r.b).unwrap_or(&Value::Null),
                    );
                    annotate_changes(&mut c, &la, &lb);
                    for c in c.iter().take(4) {
                        let side = |v: &Option<Value>, l: &Option<readable::Readable>| {
                            l.as_ref().map(|l| l.text.clone()).unwrap_or_else(|| {
                                v.as_ref().map(|v| v.to_string()).unwrap_or("-".into())
                            })
                        };
                        eprintln!(
                            "      {}: {} -> {} {}",
                            c.path_label.as_deref().unwrap_or(&c.path),
                            side(&c.old, &c.old_label),
                            side(&c.new, &c.new_label),
                            c.unit.as_deref().unwrap_or("")
                        );
                    }
                }
            }
        }
    }
    eprintln!("titled {titled} of {all} changed/added/removed records");
    assert!(
        all > 0 && titled * 20 >= all * 19,
        "most records read as a name"
    );
    let comps = db.table("components").unwrap();
    let q = RowsQuery {
        columns: vec!["recordName".into(), "manufacturer".into(), "type".into()],
        search: "bracer".into(),
        limit: 3,
        ..Default::default()
    };
    let t = std::time::Instant::now();
    let p = query_rows(comps, &q, 500, Some(&lb));
    eprintln!("search by name: {} rows in {:?}", p.total, t.elapsed());
    for r in &p.rows {
        let cells: Vec<String> = r
            .cells
            .iter()
            .zip(&r.labels)
            .map(|(c, l)| match (c, l) {
                (Some(c), Some(l)) => format!("{} [{c}]", l.text),
                (Some(c), None) => c.clone(),
                _ => "-".into(),
            })
            .collect();
        eprintln!("  {:?}: {}", r.title, cells.join(" | "));
    }
    assert!(p.total > 0 && p.rows[0].title.is_some());
}
