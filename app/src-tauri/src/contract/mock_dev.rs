//! Developer mode in the mock backend (addendum 7.7): a synthetic 300-record DataCore (three
//! types, cross references, one 1.5 MiB record), synthetic datasets per channel build and a
//! sample folder of three files, all OS-independent. The explorer, rows and diff go through
//! the same core functions as the real backend. `mock-dev.gen.json` (written by the contract
//! tests) carries this data to the TS mock (`app/src/lib/contract/mock-dev.ts`).

use super::Progress;
use super::dev::*;
use super::mock::MockBackend;
use super::types::*;
use serde_json::{Value, json};
use starorg_core::dev::dataset::{self, Dataset, RowsQuery};
use starorg_core::dev::diff::{self, TableDiff};
use starorg_core::dev::explorer::{Entry, ExplorerIndex, MAX_PAGE};
use starorg_core::dev::readable::{self, KnownRecord, LabelKind, MapLookup};
use starorg_core::dev::record;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const SHIPS: u32 = 40;
pub const COMPONENTS: u32 = 200;
pub const TAGS: u32 = 60;
/// Ship 39 carries a long loadout: its JSON is about 1.5 MiB.
pub const HUGE_SHIP: u32 = 39;
pub const HUGE_PORTS: u32 = 3_400;
const KINDS: [&str; 4] = ["cooler", "shield", "powerplant", "quantumdrive"];
pub const FOLDER_LABEL: &str = "sample-data";
pub const FOLDER_PATH: &str = "/synthetic/sample-data";
pub const GAMEDATA: &str = "gamedata.json";

pub fn ship_guid(n: u32) -> String {
    format!("51000000-0000-4000-8000-{n:012x}")
}
pub fn component_guid(n: u32) -> String {
    format!("52000000-0000-4000-8000-{n:012x}")
}
pub fn tag_guid(n: u32) -> String {
    format!("53000000-0000-4000-8000-{n:012x}")
}

fn ship_path(n: u32) -> String {
    format!("libs/foundry/records/entities/spaceships/syn_ship_{n:03}.xml")
}
fn component_path(n: u32) -> String {
    format!(
        "libs/foundry/records/entities/scitem/ships/{}/syn_comp_{n:03}.xml",
        KINDS[(n % 4) as usize]
    )
}
const TAG_PATH: &str = "libs/foundry/records/tagdatabase/syn_tags.xml";

/// Mock string keys (see `synthetic_strings` in mock.rs): ships are rows 8k, coolers 8k+1.
fn ship_key(n: u32) -> String {
    format!("@vehicle_DescSYN_Ship_{:05}", n * 8)
}
fn component_key(n: u32) -> String {
    let i = n * 8 + 1;
    format!("@item_NameSYN_Cooler_S{}_{i:05}", i % 4 + 1)
}

fn reference(guid: String, name: String, ty: &str, path: String) -> Value {
    json!({ record::REF_KEY: guid, "name": name, "type": ty, "path": path })
}

/// The synthetic DataCore: index entries and record values (same order).
pub struct MockCore {
    pub index: ExplorerIndex,
    pub values: Vec<Value>,
}

fn build_core() -> MockCore {
    let mut entries = Vec::new();
    let mut values = Vec::new();
    let ship_name = |n: u32| format!("SYN_Ship_{n:03}");
    let comp_name = |n: u32| format!("SYN_{}_{n:03}", KINDS[(n % 4) as usize]);
    for n in 0..SHIPS {
        entries.push(Entry {
            guid: ship_guid(n),
            name: ship_name(n),
            struct_type: "SynShip".into(),
            path: ship_path(n),
            main: true,
            tag: None,
        });
        let comps: Vec<Value> = (0..3)
            .map(|k| {
                let c = (n * 5 + k) % COMPONENTS;
                reference(
                    component_guid(c),
                    comp_name(c),
                    "SynComponent",
                    component_path(c),
                )
            })
            .collect();
        let mut v = json!({
            "_Type_": "SynShip",
            "DisplayName": format!("Synthetic hull {n}"),
            "Description": ship_key(n),
            "Crew": n % 4 + 1,
            "Mass": 10_000 + n * 250,
            "Speed": { "Scm": 150 + n, "Max": 900 + n * 7, "Boost": { "Forward": 1.5, "Back": 0.75 } },
            "Components": comps,
        });
        if n == HUGE_SHIP {
            v["Loadout"] = Value::Array(
                (0..HUGE_PORTS)
                    .map(|i| {
                        json!({
                            "Port": format!("hardpoint_syn_{i:05}"),
                            "Size": i % 5 + 1,
                            "Note": "synthetic loadout padding ".repeat(16),
                        })
                    })
                    .collect(),
            );
        }
        values.push(json!({
            "_RecordName_": ship_name(n),
            "_RecordId_": ship_guid(n),
            "_RecordValue_": v,
        }));
    }
    for n in 0..COMPONENTS {
        entries.push(Entry {
            guid: component_guid(n),
            name: comp_name(n),
            struct_type: "SynComponent".into(),
            path: component_path(n),
            main: true,
            tag: None,
        });
        let s = n % SHIPS;
        values.push(json!({
            "_RecordName_": comp_name(n),
            "_RecordId_": component_guid(n),
            "_RecordValue_": {
                "_Type_": "SynComponent",
                "Name": component_key(n),
                "Size": n % 4 + 1,
                "Grade": (["A", "B", "C", "D"][(n % 4) as usize]),
                "Health": 500 + n * 3,
                "Ship": reference(ship_guid(s), ship_name(s), "SynShip", ship_path(s)),
            },
        }));
    }
    for n in 0..TAGS {
        // One file: only the last record is a main record.
        entries.push(Entry {
            guid: tag_guid(n),
            name: format!("SynTag{n:02}"),
            struct_type: "SynTag".into(),
            path: TAG_PATH.into(),
            main: n == TAGS - 1,
            tag: None,
        });
        values.push(json!({
            "_RecordName_": format!("SynTag{n:02}"),
            "_RecordId_": tag_guid(n),
            "_RecordValue_": { "_Type_": "SynTag", "Label": format!("tag {n}") },
        }));
    }
    MockCore {
        index: ExplorerIndex::new(entries),
        values,
    }
}

pub fn core() -> &'static MockCore {
    static CORE: OnceLock<MockCore> = OnceLock::new();
    CORE.get_or_init(build_core)
}

/// Synthetic manufacturer names (`manufacturer_Name<code>`) of the mock's codes.
pub const MAKERS: [(&str, &str); 3] = [
    ("SYNA", "Alpha Synthetics"),
    ("SYNB", "Beta Works"),
    ("SYNC", "Gamma Test Yards"),
];

/// The records the mock's labels know (7.14): every synthetic record, components with their
/// `Name` key, ships with their plain `DisplayName`, tags with their label.
pub fn known() -> Vec<KnownRecord> {
    let c = core();
    c.index
        .entries()
        .iter()
        .zip(&c.values)
        .map(|(e, v)| {
            let tag = e.struct_type == "SynTag";
            let field = match e.struct_type.as_str() {
                "SynTag" => "Label",
                // Ships carry a plain display name; their key is a description.
                "SynShip" => "DisplayName",
                _ => "Name",
            };
            KnownRecord {
                guid: Some(e.guid.clone()),
                name: e.name.clone(),
                kind: if tag {
                    LabelKind::Tag
                } else {
                    LabelKind::Record
                },
                display: v["_RecordValue_"][field].as_str().map(str::to_string),
            }
        })
        .collect()
}

/// Synthetic strings by lower-case key: (stock, result).
fn texts() -> &'static HashMap<String, (String, String)> {
    static T: OnceLock<HashMap<String, (String, String)>> = OnceLock::new();
    T.get_or_init(|| {
        super::mock::synthetic_strings()
            .into_iter()
            .map(|r| (r.key.to_lowercase(), (r.original, r.result)))
            .collect()
    })
}

/// Enhanced text of the keys the known records name, where it differs from stock (lower-case
/// key -> text); the TS mock gets the same map.
pub fn enhanced() -> &'static HashMap<String, String> {
    static E: OnceLock<HashMap<String, String>> = OnceLock::new();
    E.get_or_init(|| {
        known()
            .iter()
            .filter_map(|r| {
                let k = r.display.as_deref()?.trim_start_matches('@').to_lowercase();
                let (o, res) = texts().get(&k)?;
                (o != res).then(|| (k, res.clone()))
            })
            .collect()
    })
}

/// The mock's lookup: known records, synthetic stock text and manufacturer names, enhanced
/// text. `bare` for dataset rows.
pub fn lookup(bare: bool) -> MapLookup {
    let mut m = MapLookup::new();
    for r in known() {
        m.add(r);
    }
    m.with_stock(Arc::new(|k: &str| {
        let lk = k.to_lowercase();
        if let Some(code) = lk.strip_prefix("manufacturer_name") {
            return MAKERS
                .iter()
                .find(|x| x.0.eq_ignore_ascii_case(code))
                .map(|x| x.1.to_string());
        }
        texts().get(&lk).map(|x| x.0.clone())
    }))
    .with_enhanced(Arc::new(|k: &str| {
        enhanced().get(&k.to_lowercase()).cloned()
    }))
    .with_bare_keys(bare)
}

/// Synthetic GameData-like dataset of a channel's build. PTU differs from LIVE (changed,
/// added and removed rows); HOTFIX equals LIVE except one value.
pub fn build_value(channel: &str) -> Value {
    let ptu = channel == "PTU";
    let hot = channel == "HOTFIX";
    let components: Vec<Value> = (0..24u32)
        .filter(|n| !(ptu && *n == 5))
        .map(|n| {
            let mut health = 500 + n * 3;
            if ptu && n % 4 == 1 {
                health += 25;
            }
            if hot && n == 2 {
                health += 1;
            }
            json!({
                "name": format!("Sample {} {n:02}", ["Cooler", "Shield", "Power Plant", "Quantum Drive"][(n % 4) as usize]),
                "recordName": format!("SYN_{}_{n:03}", KINDS[(n % 4) as usize]),
                "kind": KINDS[(n % 4) as usize],
                "size": n % 4 + 1,
                "grade": (["A", "B", "C", "D"][(n % 4) as usize]),
                "manufacturer": (["SYNA", "SYNB", "SYNC"][(n % 3) as usize]),
                "health": health,
                "loc": { "name": format!("item_NameSYN_Cooler_S{}_{:05}", (n * 8 + 1) % 4 + 1, n * 8 + 1) },
            })
        })
        .chain(ptu.then(|| json!({
            "name": "Sample Cooler 99",
            "recordName": "SYN_cooler_099",
            "kind": "cooler",
            "size": 2,
            "grade": "A",
            "manufacturer": "SYNA",
            "health": 777,
            "loc": { "name": null },
        })))
        .collect();
    let ships: Vec<Value> = (0..12u32)
        .map(|n| {
            json!({
                "name": format!("Synthetic hull {n}"),
                "recordName": format!("SYN_Ship_{n:03}"),
                "crew": n % 4 + 1,
                "scm": if ptu && n == 3 { 200 } else { 150 + n },
                "roles": ["Test", if n % 2 == 0 { "Fighter" } else { "Cargo" }],
            })
        })
        .collect();
    let weapons: Vec<Value> = (0..10u32)
        .map(|n| {
            json!({
                "name": format!("Test Repeater {n}"),
                "className": format!("SYN_Laser_S{}_{n:02}", n % 6 + 1),
                "size": n % 6 + 1,
                "dps": 100.25 + f64::from(n) * 12.5 + if ptu && n == 0 { 10.0 } else { 0.0 },
                "ammo": { "speed": 1800 + n * 10, "range": 2000 },
            })
        })
        .collect();
    json!({
        "_schema": 1,
        "_build": { "channel": channel, "version": "4.10.0-mock" },
        "components": components,
        "ships": ships,
        "weapons": weapons,
    })
}

/// The sample folder's three files (shapes of the sc-gamedata files, synthetic values).
pub fn folder_files() -> Vec<(&'static str, Value)> {
    let weapons = json!({
        "_source": "synthetic",
        "_extracted": "2026-11-14T12:00:00Z",
        "_build": "4.10.0-mock",
        "shipWeapons": (0..30u32).map(|n| json!({
            "name": format!("Test Repeater {n}"),
            "recordName": format!("SYN_Laser_S{}_{n:02}", n % 6 + 1),
            "id": ship_guid(n % SHIPS),
            "size": n % 6 + 1,
            "type": (["Gun", "Cannon", "Repeater"][(n % 3) as usize]),
            "dps": 100.25 + f64::from(n) * 12.5,
            "modes": [{ "name": "Auto", "rpm": 600 + n }],
        })).collect::<Vec<_>>(),
        "missiles": (0..8u32).map(|n| json!({
            "name": format!("Sample Missile {n}"),
            "recordName": format!("SYN_Missile_S{}_{n:02}", n % 3 + 1),
            "size": n % 3 + 1,
            "damage": 1000 + n * 250,
        })).collect::<Vec<_>>(),
        "summary": { "shipWeapons": 30, "missiles": 8 },
    });
    let ships = json!({
        "_source": "synthetic",
        "_extracted": "2026-11-14T12:00:00Z",
        "_build": "4.10.0-mock",
        "vehicles": (0..12u32).map(|n| json!({
            "name": format!("Synthetic hull {n}"),
            "className": format!("SYN_Ship_{n:03}"),
            "manufacturer": (["Alpha Synthetics", "Beta Works", "Gamma Test Yards"][(n % 3) as usize]),
            "crew": n % 4 + 1,
            "cargo": n * 8,
        })).collect::<Vec<_>>(),
        "byManufacturer": { "Alpha Synthetics": 4, "Beta Works": 4, "Gamma Test Yards": 4 },
        "summary": { "vehicles": 12 },
    });
    let strings: serde_json::Map<String, Value> = (0..40u32)
        .map(|n| {
            let i = n * 8;
            (
                format!("vehicle_DescSYN_Ship_{i:05}"),
                Value::String(format!(
                    "Synthetic hull {i}. A test ship description.\\nSecond line."
                )),
            )
        })
        .collect();
    let strings = json!({
        "_source": "synthetic",
        "_extracted": "2026-11-14T12:00:00Z",
        "_build": "4.10.0-mock",
        "language": "english",
        "strings": strings,
        "summary": { "strings": 40 },
    });
    vec![
        ("game-ships.json", ships),
        ("game-strings-english.json", strings),
        ("game-weapons.json", weapons),
    ]
}

fn to_dataset(file: &str, v: Value) -> Dataset {
    let bytes = serde_json::to_string(&v)
        .map(|s| s.len() as u64)
        .unwrap_or(0);
    dataset::infer(file, bytes, v)
}

/// Summary, per-table results and both sides' datasets of the last comparison.
type MockDiff = (
    DevDiffSummary,
    Vec<TableDiff>,
    Vec<(String, Arc<Dataset>)>,
    Vec<(String, Arc<Dataset>)>,
);

/// The mock's Developer mode state.
#[derive(Default)]
pub struct DevMock {
    /// The channel whose explorer is open (`dev_open`).
    open: Option<(String, String)>,
    folder: bool,
    diff: Option<MockDiff>,
}

fn not_found(m: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::NotFound, m)
}

impl MockBackend {
    fn dev_on(&self, r: &ChannelRef) -> CmdResult<()> {
        if !self.dev_mode_on() {
            return Err(disabled());
        }
        if !self.has_channel(r) {
            return Err(not_found(format!(
                "No channel {} in install {}",
                r.channel, r.install
            )));
        }
        Ok(())
    }

    /// Requests start the explorer when it is not open, as the real worker does.
    fn explorer(&self, r: &ChannelRef) -> CmdResult<&'static MockCore> {
        self.dev_on(r)?;
        self.dev_state().open = Some((r.install.clone(), r.channel.clone()));
        Ok(core())
    }

    fn status(&self, r: &ChannelRef) -> DevStatus {
        let open = self.dev_state().open.clone() == Some((r.install.clone(), r.channel.clone()));
        let c = core();
        DevStatus {
            state: if open {
                WorkerState::Ready
            } else {
                WorkerState::Stopped
            },
            channel: Some(r.channel.clone()),
            build: Some("4.10.0-mock".into()),
            records: if open { c.index.len() as u32 } else { 0 },
            main: if open { c.index.main_count() as u32 } else { 0 },
            types: if open {
                c.index.types().len() as u32
            } else {
                0
            },
            started: open.then(|| "2026-11-14T21:30:05Z".into()),
            start_ms: open.then_some(840),
            memory: open.then_some(412 * 1024 * 1024),
            error: None,
        }
    }

    fn sources_of(&self, r: &ChannelRef) -> Vec<DevSource> {
        let mut chans = self.channel_names(&r.install);
        chans.sort_by_key(|c| *c != r.channel);
        let mut out: Vec<DevSource> = chans
            .iter()
            .map(|c| {
                let d = to_dataset(GAMEDATA, build_value(c));
                DevSource {
                    id: format!("build:{c}"),
                    kind: SourceKind::Build,
                    label: format!("{c} build"),
                    build: Some("4.10.0-mock".into()),
                    state: SourceState::Ready,
                    detail: None,
                    datasets: vec![DevDatasetSummary {
                        file: GAMEDATA.into(),
                        bytes: d.info.bytes,
                        tables: d.tables.len() as u32,
                        rows: d.tables.iter().map(|t| t.rows.len() as u32).sum(),
                    }],
                }
            })
            .collect();
        if self.dev_state().folder {
            out.push(DevSource {
                id: "folder:0".into(),
                kind: SourceKind::Folder,
                label: FOLDER_LABEL.into(),
                build: None,
                state: SourceState::Ready,
                detail: Some(FOLDER_PATH.into()),
                datasets: folder_files()
                    .into_iter()
                    .map(|(f, v)| {
                        let d = to_dataset(f, v);
                        DevDatasetSummary {
                            file: f.into(),
                            bytes: d.info.bytes,
                            tables: d.tables.len() as u32,
                            rows: d.tables.iter().map(|t| t.rows.len() as u32).sum(),
                        }
                    })
                    .collect(),
            });
        }
        out.push(DevSource {
            id: format!("dump:{}", r.channel),
            kind: SourceKind::Dump,
            label: "Published dump".into(),
            build: None,
            state: SourceState::Off,
            detail: Some(if self.dumps_on() {
                "Follow-up: not in this build".into()
            } else {
                "Off (Settings > Network)".into()
            }),
            datasets: vec![],
        });
        out
    }

    fn datasets_of(&self, r: &ChannelRef, source: &str) -> CmdResult<Vec<(String, Arc<Dataset>)>> {
        if let Some(c) = source.strip_prefix("build:") {
            if !self.channel_names(&r.install).iter().any(|x| x == c) {
                return Err(not_found(format!("no source {source}")));
            }
            return Ok(vec![(
                GAMEDATA.into(),
                Arc::new(to_dataset(GAMEDATA, build_value(c))),
            )]);
        }
        if source == "folder:0" && self.dev_state().folder {
            return Ok(folder_files()
                .into_iter()
                .map(|(f, v)| (f.to_string(), Arc::new(to_dataset(f, v))))
                .collect());
        }
        if source.starts_with("dump:") {
            return Err(dump_refusal(false));
        }
        Err(not_found(format!("no source {source}")))
    }

    fn dataset_of(&self, r: &ChannelRef, source: &str, name: &str) -> CmdResult<Arc<Dataset>> {
        self.dev_on(r)?;
        self.datasets_of(r, source)?
            .into_iter()
            .find(|d| d.0 == name)
            .map(|d| d.1)
            .ok_or_else(|| not_found(format!("no dataset {name}")))
    }

    fn progress_job(
        &self,
        label: &str,
        steps: u32,
        channel: &str,
        progress: Progress<'_>,
    ) -> JobProgress {
        let job = self.next_job_id();
        let p = JobProgress {
            job,
            kind: JobKind::Dev,
            channel: Some(channel.into()),
            phase: JobPhase::Running,
            step: 1,
            steps,
            label: label.into(),
            cancellable: false,
        };
        progress(p.clone());
        p
    }

    fn wait(&self) {
        let d = self.job_delay();
        if !d.is_zero() {
            std::thread::sleep(d);
        }
    }
}

fn identity_options(a: Option<&dataset::Table>, b: Option<&dataset::Table>) -> Vec<String> {
    let cols = |t: Option<&dataset::Table>| -> Vec<String> {
        t.map(|t| {
            t.columns
                .iter()
                .filter(|c| c.kind.scalar())
                .map(|c| c.name.clone())
                .collect()
        })
        .unwrap_or_default()
    };
    let (ca, cb) = (cols(a), cols(b));
    match (a, b) {
        (Some(_), Some(_)) => ca.into_iter().filter(|c| cb.contains(c)).collect(),
        (Some(_), None) => ca,
        _ => cb,
    }
}

impl DevBackend for MockBackend {
    fn dev_status(&self, req: ChannelRef) -> CmdResult<DevStatus> {
        self.dev_on(&req)?;
        Ok(self.status(&req))
    }

    fn dev_open(&self, req: ChannelRef, progress: Progress<'_>) -> CmdResult<DevStatus> {
        self.dev_on(&req)?;
        let mut p = self.progress_job("Opening the game data", 2, &req.channel, progress);
        self.wait();
        p.step = 2;
        p.label = "Indexing records".into();
        progress(p.clone());
        self.wait();
        p.phase = JobPhase::Done;
        p.label = "Explorer ready".into();
        progress(p);
        self.explorer(&req)?;
        Ok(self.status(&req))
    }

    fn dev_close(&self, req: ChannelRef) -> CmdResult<DevStatus> {
        self.dev_on(&req)?;
        self.dev_state().open = None;
        Ok(self.status(&req))
    }

    fn dev_types(&self, req: ChannelRef) -> CmdResult<Vec<TypeCount>> {
        Ok(self.explorer(&req)?.index.types())
    }

    fn dev_tree(&self, req: DevTreeRequest) -> CmdResult<TreePage> {
        let c = self.explorer(&req.target)?;
        Ok(c.index.tree(
            req.root,
            &req.node,
            req.offset as usize,
            (req.limit as usize).min(MAX_PAGE),
            req.main_only,
        ))
    }

    fn dev_search(&self, req: DevSearchRequest) -> CmdResult<SearchPage> {
        let c = self.explorer(&req.target)?;
        Ok(c.index.search(&req.query, req.limit as usize))
    }

    fn dev_record(&self, req: DevRecordRequest) -> CmdResult<RecordView> {
        let c = self.explorer(&req.target)?;
        let i = c
            .index
            .find(&req.what)
            .ok_or_else(|| not_found(format!("no record {:?}", req.what)))?;
        let v = &c.values[i];
        let (json, truncated) =
            record::view_json(v, &req.pointer, req.depth.map(|d| d as usize)).map_err(not_found)?;
        let node = v.pointer(&req.pointer).unwrap_or(v);
        let loc = record::loc_keys(node)
            .into_iter()
            .filter_map(|k| {
                let text = self.stock_text(k.trim_start_matches('@'))?;
                Some(LocText { key: k, text })
            })
            .collect();
        let sent: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
        let labels = readable::labels_in(&lookup(false), &sent);
        let e = c.index.entry(i).expect("found");
        Ok(RecordView {
            header: RecordHeader {
                guid: e.guid.clone(),
                name: e.name.clone(),
                struct_type: e.struct_type.clone(),
                path: e.path.clone(),
                tag: e.tag.clone(),
                main: e.main,
                bytes: serde_json::to_string(v)
                    .map(|s| s.len() as u64)
                    .unwrap_or(0),
            },
            json,
            pointer: req.pointer,
            truncated,
            loc,
            labels,
        })
    }

    /// No native dialog in the mock: always "cancelled", nothing written.
    fn dev_export_record(&self, req: DevExportRecordRequest) -> CmdResult<DevExportResult> {
        let c = self.explorer(&req.target)?;
        c.index
            .find(&req.what)
            .ok_or_else(|| not_found(format!("no record {:?}", req.what)))?;
        Ok(DevExportResult {
            cancelled: true,
            path: None,
            records: 0,
            bytes: 0,
        })
    }

    fn dev_export_type(
        &self,
        req: DevExportTypeRequest,
        _progress: Progress<'_>,
    ) -> CmdResult<DevExportResult> {
        let c = self.explorer(&req.target)?;
        if c.index.of_type(&req.type_name, req.main_only).is_empty() {
            return Err(not_found(format!("no records of type {}", req.type_name)));
        }
        Ok(DevExportResult {
            cancelled: true,
            path: None,
            records: 0,
            bytes: 0,
        })
    }

    fn dev_sources(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req)?;
        Ok(self.sources_of(&req))
    }

    /// The mock's "folder dialog" always picks the synthetic sample folder.
    fn dev_add_source(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req)?;
        self.dev_state().folder = true;
        Ok(self.sources_of(&req))
    }

    fn dev_remove_source(&self, req: DevSourceRef) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req.target)?;
        if req.source != "folder:0" || !self.dev_state().folder {
            return Err(not_found(format!("no source {}", req.source)));
        }
        let mut s = self.dev_state();
        s.folder = false;
        s.diff = None;
        drop(s);
        Ok(self.sources_of(&req.target))
    }

    fn dev_download_dump(
        &self,
        req: ChannelRef,
        _progress: Progress<'_>,
    ) -> CmdResult<Vec<DevSource>> {
        self.dev_on(&req)?;
        Err(dump_refusal(self.dumps_on()))
    }

    fn dev_dataset(&self, req: DevDatasetRef) -> CmdResult<DatasetInfo> {
        Ok(self
            .dataset_of(&req.target, &req.source, &req.dataset)?
            .info
            .clone())
    }

    fn dev_table(&self, req: DevTableRef) -> CmdResult<TableInfo> {
        let d = self.dataset_of(&req.target, &req.source, &req.dataset)?;
        d.table(&req.table)
            .map(|t| t.info())
            .ok_or_else(|| not_found(format!("no table {}", req.table)))
    }

    fn dev_rows(&self, req: DevRowsRequest) -> CmdResult<RowsPage> {
        let d = self.dataset_of(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        Ok(dataset::query_rows(
            t,
            &RowsQuery {
                columns: req.columns,
                search: req.search,
                filters: req.filters,
                sort: req.sort,
                offset: req.offset,
                limit: req.limit,
            },
            MAX_PAGE,
            Some(&lookup(true)),
        ))
    }

    fn dev_row(&self, req: DevRowRequest) -> CmdResult<DevRowDetail> {
        let d = self.dataset_of(&req.target, &req.source, &req.dataset)?;
        let t = d
            .table(&req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        let row = t
            .rows
            .get(req.row as usize)
            .ok_or_else(|| not_found(format!("no row {}", req.row)))?;
        let (json, truncated) = record::view_json(row, "", None).map_err(not_found)?;
        let loc = row_loc(row, &|k| self.stock_text(k));
        let open = self.dev_state().open.is_some();
        let ix = &core().index;
        let links = if open {
            explorer_links(row, &|s| {
                ix.by_guid(s)
                    .or_else(|| ix.find_name(s))
                    .and_then(|i| ix.entry(i))
                    .map(|e| e.guid.clone())
            })
        } else {
            vec![]
        };
        Ok(DevRowDetail {
            row: req.row,
            json,
            truncated,
            loc,
            links,
            labels: readable::labels_in(&lookup(true), row),
        })
    }

    fn dev_diff(&self, req: DevDiffRequest, progress: Progress<'_>) -> CmdResult<DevDiffSummary> {
        self.dev_on(&req.target)?;
        let mut p = self.progress_job("Reading datasets", 2, &req.target.channel, progress);
        let a = self.datasets_of(&req.target, &req.a)?;
        let b = self.datasets_of(&req.target, &req.b)?;
        self.wait();
        p.step = 2;
        p.label = "Comparing".into();
        progress(p.clone());
        let ra: Vec<(String, &Dataset)> = a.iter().map(|x| (x.0.clone(), &*x.1)).collect();
        let rb: Vec<(String, &Dataset)> = b.iter().map(|x| (x.0.clone(), &*x.1)).collect();
        let tables = diff::diff_sources(&ra, &rb, &|_, _| {});
        let summary = DevDiffSummary {
            a: req.a,
            b: req.b,
            tables: tables.iter().map(|t| t.summary.clone()).collect(),
            ms: 12,
        };
        self.dev_state().diff = Some((summary.clone(), tables, a, b));
        p.phase = JobPhase::Done;
        p.label = "Compared".into();
        progress(p);
        Ok(summary)
    }

    fn dev_diff_summary(&self, req: ChannelRef) -> CmdResult<DevDiffSummary> {
        self.dev_on(&req)?;
        self.dev_state()
            .diff
            .as_ref()
            .map(|d| d.0.clone())
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))
    }

    fn dev_diff_records(&self, req: DevDiffRecordsRequest) -> CmdResult<DevDiffRecords> {
        self.dev_on(&req.target)?;
        let mut g = self.dev_state();
        let (_, tables, a, b) = g
            .diff
            .as_mut()
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))?;
        let find = |side: &[(String, Arc<Dataset>)]| {
            side.iter()
                .find(|x| x.0 == req.dataset)
                .and_then(|x| x.1.table(&req.table).cloned())
        };
        let (ta, tb) = (find(a), find(b));
        let pos = tables
            .iter()
            .position(|t| t.summary.dataset == req.dataset && t.summary.table == req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        if req.identity.is_some() && tables[pos].summary.identity != req.identity {
            tables[pos] = diff::diff_table(
                &req.dataset,
                &req.table,
                ta.as_ref(),
                tb.as_ref(),
                req.identity.as_deref(),
            );
        }
        let l = lookup(true);
        let id = tables[pos].summary.identity.clone();
        Ok(page_records(
            &tables[pos],
            &req,
            identity_options(ta.as_ref(), tb.as_ref()),
            &|r| record_title(r, id.as_deref(), ta.as_ref(), tb.as_ref(), &l, &l),
        ))
    }

    fn dev_diff_record(&self, req: DevDiffRecordRequest) -> CmdResult<DevFieldDiff> {
        self.dev_on(&req.target)?;
        let g = self.dev_state();
        let (_, tables, a, b) = g
            .diff
            .as_ref()
            .ok_or_else(|| not_found("No comparison yet: pick A and B."))?;
        let td = tables
            .iter()
            .find(|t| t.summary.dataset == req.dataset && t.summary.table == req.table)
            .ok_or_else(|| not_found(format!("no table {}", req.table)))?;
        let rec = td
            .records
            .iter()
            .find(|r| r.id == req.id)
            .ok_or_else(|| not_found(format!("no record {}", req.id)))?;
        let row = |side: &[(String, Arc<Dataset>)], i: Option<u32>| -> Value {
            i.and_then(|i| {
                side.iter()
                    .find(|x| x.0 == req.dataset)
                    .and_then(|x| x.1.table(&req.table))
                    .and_then(|t| t.rows.get(i as usize).cloned())
            })
            .unwrap_or_else(|| Value::Object(Default::default()))
        };
        let l = lookup(true);
        let (ra, rb) = (row(a, rec.a), row(b, rec.b));
        let mut changes = diff::field_changes(&ra, &rb);
        diff::annotate_changes(&mut changes, &l, &l);
        diff::name_positions(&mut changes, &ra, &rb, &l, &l);
        let label = record_title(
            rec,
            td.summary.identity.as_deref(),
            side_table(a, &req.dataset, &req.table),
            side_table(b, &req.dataset, &req.table),
            &l,
            &l,
        );
        Ok(DevFieldDiff {
            id: rec.id.clone(),
            label,
            state: rec.state,
            changes,
        })
    }
}

/// What the TS mock needs to behave like this one (`mock-dev.gen.json`): the index entries,
/// the record values (the huge ship's loadout left out and rebuilt in TS by the same rule),
/// the build datasets and the sample folder, raw.
pub fn gen_file() -> Value {
    let c = core();
    let values: Vec<Value> = c
        .values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if c.index
                .entry(i)
                .is_some_and(|e| e.guid == ship_guid(HUGE_SHIP))
            {
                let mut v = v.clone();
                v["_RecordValue_"]
                    .as_object_mut()
                    .expect("object")
                    .shift_remove("Loadout");
                v
            } else {
                v.clone()
            }
        })
        .collect();
    json!({
        "entries": c.index.entries(),
        "values": values,
        "huge": { "guid": ship_guid(HUGE_SHIP), "ports": HUGE_PORTS },
        "builds": (["LIVE", "PTU", "HOTFIX"].iter().map(|c| (c.to_string(), build_value(c))).collect::<serde_json::Map<_, _>>()),
        "known": known(),
        "makers": (MAKERS.iter().map(|(c, n)| (c.to_string(), Value::from(*n))).collect::<serde_json::Map<_, _>>()),
        "enhanced": (enhanced().iter().map(|(k, v)| (k.clone(), Value::from(v.clone()))).collect::<std::collections::BTreeMap<_, _>>()),
        "folder": { "label": FOLDER_LABEL, "path": FOLDER_PATH, "files": (folder_files().into_iter().map(|(f, v)| json!({ "file": f, "value": v })).collect::<Vec<_>>()) },
    })
}
