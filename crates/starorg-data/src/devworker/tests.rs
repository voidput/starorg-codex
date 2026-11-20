use super::*;
use crate::starbreaker::synthetic::{self, ITEM_TYPE, MAKER_TYPE, TAG_FILE, TAG_TYPE};
use starorg_core::dev::explorer::NodeKind;

fn info() -> WorkerInfo {
    WorkerInfo {
        records: 0,
        main: 0,
        types: 0,
        struct_defs: 3,
        dcb_bytes: 0,
        start_ms: 0,
        rss: None,
        source: "test".into(),
        sections: None,
    }
}

const INI: &[u8] = b"item_NameSYN_Item_001=Synthetic Item One\r\n";

#[test]
fn index_counts_main_records_types_and_paths() {
    let bytes = synthetic::build(3, 10, 4);
    let db = DevDatabase::parse(&bytes).unwrap();
    let ix = db.index();
    assert_eq!(ix.len(), 17);
    // Tags share one file: only the last is a main record.
    assert_eq!(ix.main_count(), 14);
    let types = ix.types();
    let names: Vec<&str> = types.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, [ITEM_TYPE, MAKER_TYPE, TAG_TYPE]);
    let tags = types.iter().find(|t| t.name == TAG_TYPE).unwrap();
    assert_eq!((tags.records, tags.main), (4, 1));
    let items = ix.tree(TreeRoot::Type, &format!("t:{ITEM_TYPE}"), 0, 500, true);
    assert_eq!(items.total, 10);
    assert_eq!(items.nodes[0].label, "SYN_Item_000");
    let root = ix.tree(TreeRoot::Path, "", 0, 500, true);
    let labels: Vec<&str> = root.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["entities", "scitemmanufacturer", "tagdatabase"]);
    assert!(root.nodes.iter().all(|n| n.kind == NodeKind::Folder));
    let tags = ix.tree(TreeRoot::Path, "p:tagdatabase/", 0, 500, false);
    assert_eq!(tags.total, 4);
    let e = ix
        .entry(ix.find(&synthetic::guid_text(1, 2)).unwrap())
        .unwrap();
    assert_eq!(e.name, "SYN_Item_002");
    assert_eq!(e.path, synthetic::item_path(2));
    assert_eq!(
        ix.entry(ix.find(TAG_FILE).unwrap()).unwrap().name,
        "SynTag3"
    );
}

#[test]
fn references_are_objects_with_name_type_and_path() {
    let bytes = synthetic::build(3, 10, 4);
    let db = DevDatabase::parse(&bytes).unwrap();
    let i = db.index().find("SYN_Item_004").unwrap();
    let v = db.record_value(i).unwrap();
    let rv = &v["_RecordValue_"];
    assert_eq!(v["_RecordName_"], "SYN_Item_004");
    assert_eq!(rv["Name"], "@item_NameSYN_Item_004");
    assert_eq!(rv["Size"], 1);
    // The f32's shortest text, as the JSON export writes it.
    assert_eq!(rv["Mass"].to_string(), "2.1");
    assert_eq!(rv["Active"], true);
    let r = &rv["Maker"];
    assert_eq!(r["_ref"], synthetic::guid_text(0, 1));
    assert_eq!(r["name"], "SYN_Maker_1");
    assert_eq!(r["type"], MAKER_TYPE);
    assert_eq!(r["path"], synthetic::maker_path(1));
    // StarBreaker's own form keeps the relative path.
    let mut buf = Vec::new();
    db.write_export(i, &mut buf, false).unwrap();
    let sb: Value = serde_json::from_slice(&buf).unwrap();
    let path = sb["_RecordValue_"]["Maker"].as_str().unwrap();
    assert!(path.starts_with("file://./"), "{path}");
    assert!(path.ends_with("syn_maker_1.json"), "{path}");
}

#[test]
fn record_replies_cut_by_pointer_and_depth_and_resolve_loc() {
    let bytes = synthetic::build(3, 10, 4);
    let db = DevDatabase::parse(&bytes).unwrap();
    let mut s = Session::new(&db, Some(INI.to_vec()), info());
    let r = s.record("SYN_Item_001", "", None).unwrap();
    assert!(!r.truncated);
    assert_eq!(r.header.struct_type, ITEM_TYPE);
    assert!(r.header.main);
    assert_eq!(r.header.bytes as usize, r.json.len());
    assert_eq!(
        r.loc,
        [record::LocText {
            key: "@item_NameSYN_Item_001".into(),
            text: "Synthetic Item One".into()
        }]
    );
    let cut = s.record(&synthetic::guid_text(1, 1), "", Some(1)).unwrap();
    assert!(cut.truncated);
    let v: Value = serde_json::from_str(&cut.json).unwrap();
    assert_eq!(v["_RecordValue_"]["_cut"]["kind"], "object");
    let sub = s
        .record("SYN_Item_001", "/_RecordValue_/Maker", None)
        .unwrap();
    assert_eq!(sub.pointer, "/_RecordValue_/Maker");
    assert!(sub.json.contains("\"_ref\""));
    assert_eq!(s.record("nope", "", None).unwrap_err().code, "not-found");
    assert_eq!(
        s.record("SYN_Item_001", "/x", None).unwrap_err().code,
        "not-found"
    );
}

#[test]
fn type_export_streams_an_array_equal_to_per_record_exports() {
    let bytes = synthetic::build(3, 150, 4);
    let db = DevDatabase::parse(&bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("items.json");
    let records = db.index().of_type(ITEM_TYPE, true);
    let mut x = TypeExport::start(records.clone(), &out, false).unwrap();
    assert!(dir.path().join("items.json.tmp").exists());
    while !x.step(&db, 64).unwrap() {}
    let done = x.finish().unwrap();
    assert_eq!(done.records, 150);
    assert!(!dir.path().join("items.json.tmp").exists());
    let text = std::fs::read(&out).unwrap();
    let arr: Vec<Value> = serde_json::from_slice(&text).unwrap();
    assert_eq!(arr.len(), 150);
    // Byte-equal: the file is `[`, each record's own export, `,` ..., `]`.
    let mut want = b"[".to_vec();
    for (k, &i) in records.iter().enumerate() {
        want.extend_from_slice(if k == 0 { b"\n" } else { b",\n" });
        db.write_export(i, &mut want, true).unwrap();
    }
    want.extend_from_slice(b"\n]\n");
    assert_eq!(text, want);
    // NDJSON: one compact record per line.
    let nd = dir.path().join("items.ndjson");
    let mut x = TypeExport::start(records, &nd, true).unwrap();
    while !x.step(&db, 1000).unwrap() {}
    x.finish().unwrap();
    let lines = std::fs::read_to_string(&nd).unwrap();
    assert_eq!(lines.lines().count(), 150);
    assert!(
        lines
            .lines()
            .all(|l| serde_json::from_str::<Value>(l).is_ok())
    );
    // Dropped (cancelled) exports leave nothing.
    let gone = dir.path().join("gone.json");
    let x = TypeExport::start(vec![0], &gone, false).unwrap();
    drop(x);
    assert!(!dir.path().join("gone.json.tmp").exists() && !gone.exists());
}

/// Input that hands out the lines, then blocks until the writer has seen `want` final
/// replies (so a running export finishes before EOF).
struct Slow(
    std::collections::VecDeque<String>,
    std::sync::mpsc::Receiver<()>,
);

impl std::io::Read for Slow {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(l) = self.0.pop_front() else {
            let _ = self.1.recv();
            return Ok(0);
        };
        let b = format!("{l}\n").into_bytes();
        buf[..b.len()].copy_from_slice(&b);
        Ok(b.len())
    }
}

struct Count<'a> {
    buf: &'a mut Vec<u8>,
    want: usize,
    seen: usize,
    tx: Option<std::sync::mpsc::Sender<()>>,
}

impl Write for Count<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(b);
        // Complete lines only (a reply may arrive in several writes).
        let text = String::from_utf8_lossy(self.buf);
        let done = text.rfind('\n').map_or("", |i| &text[..i]);
        self.seen = done.lines().filter(|l| !l.contains("\"progress\"")).count();
        if self.seen >= self.want {
            self.tx.take();
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn run_lines(db: &DevDatabase, lines: &[String], hooks: Hooks) -> Vec<Value> {
    let mut s = Session::new(db, Some(INI.to_vec()), info());
    s.hooks = hooks;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let input = std::io::BufReader::new(Slow(lines.iter().cloned().collect(), rx));
    let mut buf = Vec::new();
    let mut out = Count {
        buf: &mut buf,
        want: lines.len(),
        seen: 0,
        tx: Some(tx),
    };
    serve(&mut s, input, &mut out).unwrap();
    String::from_utf8(buf)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn protocol_answers_each_line_and_exits_on_eof() {
    let bytes = synthetic::build(3, 10, 4);
    let db = DevDatabase::parse(&bytes).unwrap();
    let lines: Vec<String> = [
        r#"{"id":1,"op":"status"}"#,
        r#"{"id":2,"op":"tree","root":"type","node":"","limit":10,"main_only":true}"#,
        r#"{"id":3,"op":"search","query":"item 00","limit":5}"#,
        r#"{"id":4,"op":"record","what":"SYN_Item_001"}"#,
        r#"{"id":5,"op":"bogus"}"#,
        r#"not json"#,
        r#"{"id":6,"op":"test-panic"}"#,
        r#"{"id":7,"op":"types"}"#,
        r#"{"id":8,"op":"resolve","values":["SYN_Item_002","nothing"]}"#,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let replies = run_lines(&db, &lines, Hooks::default());
    assert_eq!(replies.len(), 9, "{replies:?}");
    assert_eq!(replies[8]["result"][0], synthetic::guid_text(1, 2));
    assert!(replies[8]["result"][1].is_null());
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[0]["result"]["struct_defs"], 3);
    assert_eq!(replies[1]["result"]["total"], 3);
    assert_eq!(replies[2]["result"]["total"], 10);
    assert_eq!(replies[2]["result"]["hits"].as_array().unwrap().len(), 5);
    assert_eq!(replies[3]["result"]["header"]["name"], "SYN_Item_001");
    assert_eq!(replies[4]["ok"], false);
    assert_eq!(replies[4]["error"]["code"], "invalid");
    assert_eq!(replies[4]["id"], 5);
    assert_eq!(replies[5]["id"], 0);
    // Without the hook, the panic op is unknown.
    assert_eq!(replies[6]["error"]["code"], "invalid");
    assert_eq!(replies[7]["result"].as_array().unwrap().len(), 3);
}

#[test]
fn a_panic_becomes_an_error_reply_and_the_worker_keeps_serving() {
    let bytes = synthetic::build(1, 2, 1);
    let db = DevDatabase::parse(&bytes).unwrap();
    let lines = vec![
        r#"{"id":1,"op":"test-panic"}"#.to_string(),
        r#"{"id":2,"op":"status"}"#.to_string(),
    ];
    let replies = run_lines(&db, &lines, Hooks { panic_op: true });
    assert_eq!(replies[0]["ok"], false);
    assert_eq!(replies[0]["error"]["code"], "internal");
    assert!(
        replies[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("test panic")
    );
    assert_eq!(replies[1]["ok"], true);
}

#[test]
fn exports_over_the_protocol_report_progress_and_can_be_refused() {
    let bytes = synthetic::build(2, 300, 1);
    let db = DevDatabase::parse(&bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("all.json");
    let one = dir.path().join("one.json");
    let q = |v: &Path| serde_json::to_string(&v.display().to_string()).unwrap();
    let lines = vec![
        format!(
            r#"{{"id":5,"op":"export-type","name":"Nope","out":{}}}"#,
            q(&out)
        ),
        format!(
            r#"{{"id":1,"op":"export-type","name":"{ITEM_TYPE}","out":{}}}"#,
            q(&out)
        ),
        format!(
            r#"{{"id":2,"op":"export-type","name":"{ITEM_TYPE}","out":{}}}"#,
            q(&out)
        ),
        r#"{"id":3,"op":"status"}"#.to_string(),
        format!(
            r#"{{"id":4,"op":"export-record","what":"SYN_Item_000","out":{}}}"#,
            q(&one)
        ),
    ];
    let replies = run_lines(&db, &lines, Hooks::default());
    let of = |id: u64| -> Vec<&Value> { replies.iter().filter(|r| r["id"] == id).collect() };
    let first = of(1);
    assert_eq!(first[0]["progress"]["done"], 0);
    assert_eq!(first[0]["progress"]["total"], 300);
    let last = first.last().unwrap();
    assert_eq!(last["result"]["records"], 300, "{first:?}");
    assert_eq!(of(2)[0]["error"]["code"], "busy");
    assert_eq!(of(3)[0]["ok"], true);
    assert_eq!(of(4)[0]["result"]["records"], 1);
    assert_eq!(of(5)[0]["error"]["code"], "not-found");
    let arr: Vec<Value> = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(arr.len(), 300);
    let rec: Value = serde_json::from_slice(&std::fs::read(&one).unwrap()).unwrap();
    assert_eq!(rec["_RecordName_"], "SYN_Item_000");
}

#[test]
fn cancel_stops_an_export_and_removes_its_file() {
    let bytes = synthetic::build(2, 300, 1);
    let db = DevDatabase::parse(&bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("all.json");
    let q = serde_json::to_string(&out.display().to_string()).unwrap();
    let lines = vec![
        format!(r#"{{"id":1,"op":"export-type","name":"{ITEM_TYPE}","out":{q}}}"#),
        r#"{"id":2,"op":"cancel","job":1}"#.to_string(),
        r#"{"id":3,"op":"cancel","job":99}"#.to_string(),
    ];
    // Cancel normally arrives before the second step: the reader thread is ahead.
    let replies = run_lines(&db, &lines, Hooks::default());
    let finished = replies
        .iter()
        .any(|r| r["id"] == 1 && r["result"]["records"] == 300);
    if !finished {
        assert!(
            replies
                .iter()
                .any(|r| r["id"] == 1 && r["error"]["code"] == "cancelled"),
            "{replies:?}"
        );
        assert!(replies.iter().any(|r| r["id"] == 2 && r["result"] == true));
        assert!(!out.exists());
    }
    assert!(replies.iter().any(|r| r["id"] == 3 && r["result"] == false));
    assert!(!dir.path().join("all.json.tmp").exists());
}

#[test]
fn exports_never_go_into_the_game_folder() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("LIVE");
    std::fs::create_dir_all(&game).unwrap();
    assert_eq!(
        check_out(&game.join("x.json"), Some(&game))
            .unwrap_err()
            .code,
        "invalid"
    );
    assert!(check_out(&dir.path().join("x.json"), Some(&game)).is_ok());
    assert!(check_out(dir.path(), None).is_err());
}

#[test]
fn args_name_one_input() {
    let a = |v: &[&str]| -> Vec<std::ffi::OsString> { v.iter().map(Into::into).collect() };
    assert_eq!(
        parse_args(&a(&["--p4k", "/g/Data.p4k"])).unwrap(),
        DevInput::P4k("/g/Data.p4k".into())
    );
    assert_eq!(
        parse_args(&a(&["--dcb", "x.dcb", "--ini", "g.ini"])).unwrap(),
        DevInput::Dcb {
            dcb: "x.dcb".into(),
            ini: Some("g.ini".into())
        }
    );
    assert!(parse_args(&a(&[])).is_err());
    assert!(parse_args(&a(&["--p4k", "a", "--dcb", "b"])).is_err());
    assert!(parse_args(&a(&["--what"])).is_err());
}

#[test]
fn load_reads_a_dcb_file_and_refuses_a_missing_archive() {
    let dir = tempfile::tempdir().unwrap();
    let dcb = dir.path().join("Game2.dcb");
    std::fs::write(&dcb, synthetic::build(1, 1, 1)).unwrap();
    let l = load(&DevInput::Dcb {
        dcb: dcb.clone(),
        ini: None,
    })
    .unwrap();
    assert!(l.ini.is_none());
    assert!(DevDatabase::parse(&l.dcb).is_ok());
    assert!(load(&DevInput::P4k(dir.path().join("Data.p4k"))).is_err());
}

#[test]
fn records_carry_labels_for_refs_and_loc_keys() {
    let bytes = synthetic::build(3, 10, 4);
    let db = DevDatabase::parse(&bytes).unwrap();
    let mut s = Session::new(&db, Some(INI.to_vec()), info());
    let v = s.record("SYN_Item_001", "", None).unwrap();
    // The item's own GUID reads as its display name; the key as its stock text.
    let own = &v.labels[&synthetic::guid_text(1, 1)];
    assert_eq!(own.text, "Synthetic Item One");
    assert_eq!(own.detail.as_deref(), Some("SYN_Item_001"));
    assert_eq!(
        v.labels["@item_NameSYN_Item_001"].text,
        "Synthetic Item One"
    );
    // The maker has no `@key`: its short record name.
    let maker = &v.labels[&synthetic::guid_text(0, 1)];
    assert_eq!(
        (maker.text.as_str(), maker.detail.as_ref()),
        ("SYN_Maker_1", None)
    );
    // A key the stock file lacks stays raw.
    let other = s.record("SYN_Item_002", "", None).unwrap();
    assert!(!other.labels.contains_key("@item_NameSYN_Item_002"));
    assert!(s.names_cached() >= 2);
    // The labels op, as the app asks for dataset cells; unknown GUIDs are null.
    let r = s
        .handle(&Request::Labels {
            values: vec![
                synthetic::guid_text(1, 1),
                "@item_NameSYN_Item_001".into(),
                "aaaaaaaa-0000-4000-8000-0000000000ff".into(),
            ],
        })
        .unwrap();
    assert_eq!(r[0]["text"], "Synthetic Item One");
    assert_eq!(r[0]["kind"], "record");
    assert_eq!(r[1]["kind"], "loc");
    assert!(r[2].is_null());
    // Record names too (dataset values GameData does not list), case-insensitive; an
    // unknown name stays raw.
    let r = s
        .handle(&Request::Labels {
            values: vec!["syn_item_001".into(), "SYN_Item_999".into()],
        })
        .unwrap();
    assert_eq!(r[0]["text"], "Synthetic Item One");
    assert_eq!(r[0]["detail"], "SYN_Item_001");
    assert!(r[1].is_null());
}

#[test]
fn sections_tree_list_marks_and_owner_over_the_protocol() {
    let bytes = synthetic::build(3, 10, 4);
    let db = DevDatabase::parse(&bytes).unwrap();
    let mut s = Session::new(&db, Some(INI.to_vec()), info());
    let stats = s.index_sections(None);
    assert!(!stats.verified);
    assert_eq!(stats.entities, 0, "the synthetic DCB has no entities");
    // Every record in exactly one section.
    let total: u32 = s.sections.counts().iter().map(|c| c.1).sum();
    assert_eq!(total as usize, db.index().len());
    let root: Value = s
        .handle(&Request::Tree {
            root: TreeRoot::Section,
            node: String::new(),
            offset: 0,
            limit: 500,
            main_only: true,
            player_only: true,
            fold_variants: false,
        })
        .unwrap();
    let nodes = root["nodes"].as_array().unwrap();
    assert!(!nodes.is_empty());
    assert!(nodes.iter().all(|n| n["kind"] == "section"));
    let other = nodes.iter().find(|n| n["label"] == "Other").expect("Other");
    // Other > SynItem type > folder > records.
    let l1 = s.sections.tree(
        db.index(),
        other["id"].as_str().unwrap(),
        0,
        500,
        true,
        false,
    );
    assert!(l1.nodes.iter().any(|n| n.label == ITEM_TYPE));
    let q = sections::ListQuery {
        query: "type:synitem syn_item_00".into(),
        limit: 500,
        ..Default::default()
    };
    let page: Value = s.handle(&Request::List(Box::new(q))).unwrap();
    assert_eq!(page["total"], 10);
    assert_eq!(page["understood"][0], "type:synitem");
    // Lazily read display names: SYN_Item_001 reads as its stock text.
    let one = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"].as_str().unwrap().ends_with("SYN_Item_001"))
        .unwrap();
    assert_eq!(one["label"], "Synthetic Item One");
    assert_eq!(one["detail"], "SYN_Item_001");
    let marked: Value = s
        .handle(&Request::Marks {
            marks: vec![
                (synthetic::guid_text(1, 1), sections::Change::Changed),
                (
                    "ffffffff-0000-0000-0000-000000000000".into(),
                    sections::Change::Added,
                ),
            ],
        })
        .unwrap();
    assert_eq!(marked, 1);
    let q = sections::ListQuery {
        filters: sections::Filters {
            changes: vec!["changed".into()],
            ..Default::default()
        },
        limit: 500,
        ..Default::default()
    };
    let page: Value = s.handle(&Request::List(Box::new(q))).unwrap();
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["change"], "changed");
    let owner: Value = s
        .handle(&Request::Owner {
            path: r"Data\Objects\nothing.cga".into(),
        })
        .unwrap();
    assert_eq!(owner, serde_json::json!([]));
    // The protocol parses the new ops (kebab-case, list flattened).
    let lines = vec![
        r#"{"id":1,"op":"list","node":"","query":"syn","limit":5}"#.to_string(),
        r#"{"id":2,"op":"tree","root":"section","node":"","limit":5,"main_only":true}"#.to_string(),
        r#"{"id":3,"op":"owner","path":"x.cga"}"#.to_string(),
    ];
    let out = run_lines(&db, &lines, Hooks::default());
    assert!(out.iter().all(|r| r["ok"] == true), "{out:?}");
    assert_eq!(out[0]["result"]["items"].as_array().unwrap().len(), 5);
}

#[test]
fn facet_sink_reads_attach_def_geometry_triggers_and_vehicle() {
    use crate::starbreaker::scan_events_for_test as scan;
    let f = scan();
    assert_eq!(f.item_type.as_deref(), Some("Cooler"));
    assert_eq!(f.sub_type.as_deref(), Some("UNDEFINED"));
    assert_eq!(f.size, Some(1));
    assert_eq!(f.manufacturer.as_deref(), Some("AEGS"));
    assert_eq!(f.display.as_deref(), Some("@item_NameX"));
    assert_eq!(f.description.as_deref(), Some("@item_DescX"));
    assert_eq!(f.geometry, ["objects/x/x.cga"]);
    assert_eq!(f.triggers, ["Play_x"]);
    assert!(f.vehicle);
}
