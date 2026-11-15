use super::dataset::*;
use super::diff::*;
use super::group::*;
use serde_json::{Value, json};

/// A weapons-like table: 12 rows, three types, sizes 1..3, two makers, path folders.
fn weapons() -> Table {
    let rows: Vec<Value> = (0..12u32)
        .map(|n| {
            let (ty, sub) = match n % 4 {
                0 | 1 => ("WeaponGun", if n % 2 == 0 { "Gun" } else { "Rocket" }),
                2 => ("WeaponMining", "Gun"),
                _ => ("Missile", "Torpedo"),
            };
            let folder = if ty == "WeaponMining" {
                "entities/scitem/ships/weapons/mining"
            } else {
                "entities/scitem/ships/weapons"
            };
            json!({
                "recordName": format!("W_{n:02}"),
                "id": format!("id-{n}"),
                "path": format!("{folder}/w_{n:02}"),
                "type": ty,
                "subType": sub,
                "size": n % 3 + 1,
                "manufacturer": if n < 6 { "AMRS" } else { "BEHR" },
                "dps": f64::from(n) * 1.5,
            })
        })
        .collect();
    infer("w.json", 0, json!({ "weapons": rows }))
        .tables
        .remove(0)
}

fn by(cols: &[&str]) -> Vec<GroupLevel> {
    cols.iter().map(|c| GroupLevel::col(c)).collect()
}

fn ids(p: &GroupPage) -> Vec<(String, String, u32, u32)> {
    p.nodes
        .iter()
        .map(|n| (n.id.clone(), n.label.clone(), n.depth, n.count))
        .collect()
}

#[test]
fn presets_follow_the_columns_present() {
    let t = weapons();
    let p = presets(&t);
    let got: Vec<&str> = p.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        got,
        [
            "chain",
            "col:manufacturer",
            "folders:path",
            "col:type",
            "col:subType",
            "col:size"
        ]
    );
    assert_eq!(p[0].label, "type › subType › size");
    assert_eq!(p[0].levels, by(&["type", "subType", "size"]));
    // A unique or a one-value column is no group; key/value tables of unique keys have none.
    let kv = infer(
        "s.json",
        0,
        json!({ "strings": { "a": "x", "b": "y", "c": "z" } }),
    );
    assert!(presets(&kv.tables[0]).is_empty());
}

#[test]
fn tree_counts_are_exact_and_open_nodes_list_children() {
    let t = weapons();
    let q = GroupQuery {
        by: by(&["type", "subType", "size"]),
        ..Default::default()
    };
    let p = group_rows(&t, &q, &no_labels);
    assert_eq!((p.total, p.all), (12, 12));
    assert_eq!(
        ids(&p),
        [
            (r#"["Missile"]"#.into(), "Missile".into(), 1, 3),
            (r#"["WeaponGun"]"#.into(), "WeaponGun".into(), 1, 6),
            (r#"["WeaponMining"]"#.into(), "WeaponMining".into(), 1, 3),
        ]
    );
    assert_eq!(p.nodes[1].children, 2);
    let q = GroupQuery {
        open: vec![r#"["WeaponGun"]"#.into(), r#"["WeaponGun","Gun"]"#.into()],
        ..q
    };
    let p = group_rows(&t, &q, &no_labels);
    let labels: Vec<(&str, u32, u32)> = p
        .nodes
        .iter()
        .map(|n| (n.label.as_str(), n.depth, n.count))
        .collect();
    assert_eq!(
        labels,
        [
            ("Missile", 1, 3),
            ("WeaponGun", 1, 6),
            ("Gun", 2, 3),
            ("Size 1", 3, 1),
            ("Size 2", 3, 1),
            ("Size 3", 3, 1),
            ("Rocket", 2, 3),
            ("WeaponMining", 1, 3),
        ]
    );
    assert_eq!(p.nodes[2].parent.as_deref(), Some(r#"["WeaponGun"]"#));
    // Children sum to the parent's count.
    let gun: u32 = p.nodes[3..6].iter().map(|n| n.count).sum();
    assert_eq!(gun, p.nodes[2].count);
}

#[test]
fn counts_follow_the_search_and_filters_but_the_structure_stays() {
    let t = weapons();
    let q = GroupQuery {
        by: by(&["manufacturer"]),
        filters: vec![RowFilter {
            column: "type".into(),
            values: vec!["Missile".into()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let p = group_rows(&t, &q, &no_labels);
    assert_eq!(p.total, 3);
    let c: Vec<(&str, u32)> = p
        .nodes
        .iter()
        .map(|n| (n.label.as_str(), n.count))
        .collect();
    assert_eq!(c, [("AMRS", 1), ("BEHR", 2)]);
    // Readable labels replace the value; the id keeps it.
    let named = |col: &str, v: &str| {
        (col == "manufacturer" && v == "AMRS").then(|| "Aegis Test".to_string())
    };
    let p = group_rows(&t, &q, &named);
    assert_eq!(p.nodes[0].label, "Aegis Test");
    assert_eq!(p.nodes[0].id, r#"["AMRS"]"#);
}

#[test]
fn path_folders_join_single_chains() {
    let t = weapons();
    let q = GroupQuery {
        by: vec![GroupLevel {
            column: "path".into(),
            folders: true,
        }],
        open: vec![r#"["entities","scitem","ships","weapons"]"#.into()],
        ..Default::default()
    };
    let p = group_rows(&t, &q, &no_labels);
    assert_eq!(
        ids(&p),
        [
            (
                r#"["entities","scitem","ships","weapons"]"#.into(),
                "entities/scitem/ships/weapons".into(),
                1,
                12
            ),
            (
                r#"["entities","scitem","ships","weapons","mining"]"#.into(),
                "mining".into(),
                2,
                3
            ),
        ]
    );
}

#[test]
fn a_picked_node_filters_rows() {
    let t = weapons();
    let pick = |node: &str| RowsQuery {
        limit: 50,
        group: Some(GroupPick {
            by: by(&["type", "subType", "size"]),
            node: node.into(),
        }),
        ..Default::default()
    };
    assert_eq!(query_rows(&t, &pick(r#"["WeaponGun"]"#), 500).total, 6);
    assert_eq!(
        query_rows(&t, &pick(r#"["WeaponGun","Gun","1"]"#), 500).total,
        1
    );
    assert_eq!(query_rows(&t, &pick(r#"["Nope"]"#), 500).total, 0);
    // Not a node id: no group filter.
    assert_eq!(query_rows(&t, &pick("garbage"), 500).total, 12);
    // A missing value is its own node.
    let mut rows = t.rows.clone();
    rows.push(json!({ "recordName": "X", "type": null }));
    let t2 = infer("w.json", 0, json!({ "weapons": rows }))
        .tables
        .remove(0);
    let p = group_rows(
        &t2,
        &GroupQuery {
            by: by(&["type"]),
            ..Default::default()
        },
        &no_labels,
    );
    let last = p.nodes.last().unwrap();
    assert_eq!(
        (last.label.as_str(), last.id.as_str(), last.count),
        ("No type", "[null]", 1)
    );
    assert_eq!(
        query_rows(
            &t2,
            &RowsQuery {
                limit: 50,
                group: Some(GroupPick {
                    by: by(&["type"]),
                    node: "[null]".into()
                }),
                ..Default::default()
            },
            500
        )
        .total,
        1
    );
}

#[test]
fn many_children_are_capped() {
    let rows: Vec<Value> = (0..(MAX_CHILDREN as u32 + 20) * 2)
        .map(|n| json!({ "k": format!("k{}", n / 2), "x": n }))
        .collect();
    let t = infer("m.json", 0, json!({ "t": rows })).tables.remove(0);
    let p = group_rows(
        &t,
        &GroupQuery {
            by: vec![GroupLevel {
                column: "k".into(),
                folders: false,
            }],
            ..Default::default()
        },
        &no_labels,
    );
    assert_eq!(p.nodes.len(), MAX_CHILDREN);
}

#[test]
fn diff_tree_groups_changed_records_by_table_then_levels() {
    let a = weapons();
    let mut rows = a.rows.clone();
    rows[0]["dps"] = json!(99.0);
    rows[2]["dps"] = json!(98.0);
    rows.remove(11);
    rows.push(json!({ "recordName": "W_99", "id": "id-99", "path": "entities/scitem/ships/weapons/w_99", "type": "Missile", "subType": "Torpedo", "size": 2, "manufacturer": "AMRS", "dps": 1.0 }));
    let b = infer("w.json", 0, json!({ "weapons": rows }))
        .tables
        .remove(0);
    let d = diff_table("w.json", "weapons", Some(&a), Some(&b), None);
    let same = diff_table("w.json", "other", Some(&a), Some(&a), None);
    let sides = [
        DiffSide {
            diff: &d,
            a: Some(&a),
            b: Some(&b),
        },
        DiffSide {
            diff: &same,
            a: Some(&a),
            b: Some(&a),
        },
    ];
    let p = diff_groups(
        &sides,
        DiffGroupBy::Auto,
        &[r#"["w.json","weapons"]"#.into()],
        &no_labels,
    );
    assert_eq!(p.all, 4);
    let got: Vec<(&str, u32, DiffCounts)> = p
        .nodes
        .iter()
        .map(|n| (n.label.as_str(), n.depth, n.diff.unwrap()))
        .collect();
    let dc = |added, removed, changed| DiffCounts {
        added,
        removed,
        changed,
    };
    // One dataset: tables are the top level; the unchanged table is left out.
    assert_eq!(
        got,
        [
            ("weapons", 1, dc(1, 1, 2)),
            ("Missile", 2, dc(1, 1, 0)),
            ("WeaponGun", 2, dc(0, 0, 1)),
            ("WeaponMining", 2, dc(0, 0, 1)),
        ]
    );
    let (ds, table, rest) = diff_node(&p.nodes[1].id).unwrap();
    assert_eq!((ds.as_str(), table.as_deref()), ("w.json", Some("weapons")));
    let levels = diff_levels(DiffGroupBy::Auto, Some(&a), Some(&b));
    let n = d
        .records
        .iter()
        .filter(|r| record_in(r, Some(&a), Some(&b), &levels, &rest))
        .count();
    assert_eq!(n, 2, "the added and the removed missile");
    // Tables only.
    let p = diff_groups(
        &sides,
        DiffGroupBy::Tables,
        &[r#"["w.json","weapons"]"#.into()],
        &no_labels,
    );
    assert_eq!(p.nodes.len(), 1);
    assert_eq!(p.nodes[0].children, 0);
}
