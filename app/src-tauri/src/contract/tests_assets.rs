//! Developer mode assets contract tests (addendum 7.16): samples for every asset command,
//! the TS mirror's unions and command list, `dev-disabled` while the setting is off, IPC, and
//! the mock's data file for the TS mock.

use super::Backend;
use super::assets::*;
use super::mock::MockBackend;
use super::mock_assets;
use super::mock_dev::{component_guid, ship_guid, tag_guid};
use super::tests::{app_src, ipc, names, ts_union};
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

fn list(dir: &str, offset: u32, limit: u32) -> AssetListRequest {
    AssetListRequest {
        target: live(),
        dir: dir.into(),
        offset,
        limit,
    }
}

fn path(p: &str) -> AssetPathRequest {
    AssetPathRequest {
        target: live(),
        path: p.into(),
    }
}

fn rec(what: String) -> AssetRecordRequest {
    AssetRecordRequest {
        target: live(),
        what,
    }
}

pub const TEXTURE: &str = "Data\\Textures\\ships\\syn\\syn_ship_001_diff.dds";

/// One sample per asset command (plus variants), in an order the TS replay follows.
pub(super) fn asset_samples() -> Vec<(&'static str, Value)> {
    let b = dev_mock();
    let mut out: Vec<(&'static str, Value)> = Vec::new();
    let mut put = |k: &'static str, v: Value| out.push((k, v));
    put("asset_cache_status", json!(b.asset_cache_status().unwrap()));
    put("asset_status", json!(b.asset_status(live()).unwrap()));
    put("asset_list", json!(b.asset_list(list("", 0, 50)).unwrap()));
    put(
        "asset_list_data",
        json!(b.asset_list(list("Data", 0, 50)).unwrap()),
    );
    put(
        "asset_list_page",
        json!(b.asset_list(list("Data/Textures/ships/syn", 2, 3)).unwrap()),
    );
    put(
        "asset_search",
        json!(
            b.asset_search(AssetSearchRequest {
                target: live(),
                query: "syn_ship_001 dds".into(),
                limit: 2,
            })
            .unwrap()
        ),
    );
    put(
        "asset_preview",
        json!(b.asset_preview(path(TEXTURE)).unwrap()),
    );
    put(
        "asset_preview_cached",
        json!(b.asset_preview(path(&TEXTURE.replace('\\', "/"))).unwrap()),
    );
    put(
        "asset_preview_text",
        json!(
            b.asset_preview(path("Data\\Libs\\Config\\defaultProfile.xml"))
                .unwrap()
        ),
    );
    put(
        "asset_preview_cryxml",
        json!(
            b.asset_preview(path(
                "Data\\Objects\\Spaceships\\Ships\\SYN\\syn_ship_002\\syn_ship_002.mtl"
            ))
            .unwrap()
        ),
    );
    put(
        "asset_preview_audio",
        json!(
            b.asset_preview(path("Data\\Sounds\\wwise\\Media\\700001.wem"))
                .unwrap()
        ),
    );
    put(
        "asset_preview_none",
        json!(b.asset_preview(path(&format!("{TEXTURE}.1"))).unwrap()),
    );
    put(
        "asset_extract",
        json!(
            b.asset_extract(
                AssetExtractRequest {
                    target: live(),
                    paths: vec![
                        "Data\\Textures".into(),
                        "Data/Libs/Config/defaultProfile.xml".into()
                    ],
                    convert: true,
                },
                &noop,
            )
            .unwrap()
        ),
    );
    let ship = b.asset_sounds(rec(ship_guid(1))).unwrap();
    put("asset_sounds", json!(ship));
    put(
        "asset_sounds_item",
        json!(b.asset_sounds(rec(component_guid(4))).unwrap()),
    );
    put(
        "asset_sounds_none",
        json!(b.asset_sounds(rec(tag_guid(2))).unwrap()),
    );
    let found = b
        .asset_sound_search(AssetSoundSearchRequest {
            target: live(),
            query: "ship_001 engine".into(),
            limit: 10,
        })
        .unwrap();
    assert_eq!(found.hits[0].name, "Play_SYN_Ship_001_Engine_Loop");
    put("asset_sound_search", json!(found));
    put(
        "asset_sound_search_words",
        json!(
            b.asset_sound_search(AssetSoundSearchRequest {
                target: live(),
                query: "door".into(),
                limit: 5,
            })
            .unwrap()
        ),
    );
    put(
        "asset_trigger",
        json!(
            b.asset_trigger(AssetTriggerRequest {
                target: live(),
                name: "Play_SYN_Ship_001_Engine_Loop".into(),
            })
            .unwrap()
        ),
    );
    let first = ship.triggers[0].sounds[0].sound.clone();
    put(
        "asset_sound",
        json!(
            b.asset_sound(AssetSoundRequest {
                target: live(),
                sound: first.clone(),
            })
            .unwrap()
        ),
    );
    put(
        "asset_sound_cached",
        json!(
            b.asset_sound(AssetSoundRequest {
                target: live(),
                sound: first,
            })
            .unwrap()
        ),
    );
    put(
        "asset_export_sounds",
        json!(
            b.asset_export_sounds(
                AssetExportSoundsRequest {
                    target: live(),
                    what: ship_guid(1),
                    sounds: vec![],
                    wem: false,
                },
                &noop,
            )
            .unwrap()
        ),
    );
    let model = |preset, lod, stl| AssetExportModelRequest {
        target: live(),
        what: ship_guid(2),
        name: "SYN_Ship_002".into(),
        preset,
        lod,
        stl,
        force: false,
    };
    put(
        "asset_export_model",
        json!(
            b.asset_export_model(model(ModelPreset::PrintShell, 0, true), &noop)
                .unwrap()
        ),
    );
    put(
        "asset_export_model_cached",
        json!(
            b.asset_export_model(model(ModelPreset::PrintShell, 0, false), &noop)
                .unwrap()
        ),
    );
    put(
        "asset_export_model_full",
        json!(
            b.asset_export_model(model(ModelPreset::Full, 1, false), &noop)
                .unwrap()
        ),
    );
    let info = |what: String| AssetModelInfoRequest {
        target: live(),
        what,
        name: String::new(),
    };
    let view = |what: String, lod| AssetModelViewRequest {
        target: live(),
        what,
        name: String::new(),
        lod,
        force: false,
    };
    put(
        "asset_model_info",
        json!(b.asset_model_info(info(ship_guid(4))).unwrap()),
    );
    // LOD 4 of a mesh with LODs 0-3: LOD 3 is exported.
    put(
        "asset_model_view",
        json!(b.asset_model_view(view(ship_guid(4), 4), &noop).unwrap()),
    );
    put(
        "asset_model_view_cached",
        json!(b.asset_model_view(view(ship_guid(4), 3), &noop).unwrap()),
    );
    put(
        "asset_model_info_known",
        json!(b.asset_model_info(info(ship_guid(4))).unwrap()),
    );
    put(
        "asset_model_view_item",
        json!(
            b.asset_model_view(view(component_guid(8), 3), &noop)
                .unwrap()
        ),
    );
    put("asset_status_open", json!(b.asset_status(live()).unwrap()));
    put("asset_cache_pick", json!(b.asset_cache_pick().unwrap()));
    put("asset_cache_move", json!(b.asset_cache_move().unwrap()));
    put("asset_cache_clear", json!(b.asset_cache_clear().unwrap()));
    put("asset_cache_reset", json!(b.asset_cache_reset().unwrap()));
    put("asset_close", json!(b.asset_close(live()).unwrap()));
    out
}

#[test]
fn asset_unions_in_assets_ts_list_every_variant() {
    let ts = std::fs::read_to_string(app_src().join("assets.ts")).unwrap();
    let cases: Vec<(&str, Vec<String>)> = vec![
        (
            "PreviewKind",
            names(&[
                PreviewKind::Image,
                PreviewKind::Text,
                PreviewKind::Audio,
                PreviewKind::None,
            ]),
        ),
        (
            "AudioSource",
            names(&[
                SourceKind::Embedded,
                SourceKind::MediaFile,
                SourceKind::External,
                SourceKind::Unavailable,
            ]),
        ),
        (
            "ModelPreset",
            names(&[ModelPreset::Full, ModelPreset::PrintShell]),
        ),
        (
            "MemoryFit",
            names(&[MemoryFit::Ok, MemoryFit::OverLimit, MemoryFit::LowFree]),
        ),
    ];
    for (name, want) in cases {
        assert_eq!(ts_union(&ts, name), want, "assets.ts union {name}");
    }
    let start = ts.find("export const ASSET_COMMANDS = [").unwrap();
    let rest = &ts[start..];
    let body = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
    let listed: Vec<&str> = body
        .split(',')
        .map(|s| s.trim().trim_matches('\''))
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(listed, ASSET_COMMAND_NAMES);
    let all: Vec<&str> = super::COMMAND_NAMES
        .iter()
        .copied()
        .filter(|n| n.starts_with("asset_"))
        .collect();
    assert_eq!(all, ASSET_COMMAND_NAMES);
    assert!(ts.contains(PERSONAL_USE), "the same personal-use note");
}

#[test]
fn every_asset_command_but_the_cache_refuses_while_developer_mode_is_off() {
    let b = MockBackend::default();
    let code = |r: Result<Value, AppError>| r.unwrap_err().code;
    fn to<T: serde::Serialize>(x: T) -> Value {
        serde_json::to_value(x).unwrap()
    }
    fn j(r: CmdResult<Value>) -> CmdResult<Value> {
        r
    }
    let calls: Vec<(&str, Result<Value, AppError>)> = vec![
        ("asset_status", j(b.asset_status(live()).map(to))),
        ("asset_close", j(b.asset_close(live()).map(to))),
        ("asset_list", j(b.asset_list(list("", 0, 5)).map(to))),
        (
            "asset_search",
            j(b.asset_search(AssetSearchRequest {
                target: live(),
                query: "x".into(),
                limit: 5,
            })
            .map(to)),
        ),
        ("asset_preview", j(b.asset_preview(path(TEXTURE)).map(to))),
        (
            "asset_extract",
            j(b.asset_extract(
                AssetExtractRequest {
                    target: live(),
                    paths: vec!["Data".into()],
                    convert: false,
                },
                &noop,
            )
            .map(to)),
        ),
        ("asset_sounds", j(b.asset_sounds(rec(ship_guid(1))).map(to))),
        (
            "asset_sound_search",
            j(b.asset_sound_search(AssetSoundSearchRequest {
                target: live(),
                query: "x".into(),
                limit: 5,
            })
            .map(to)),
        ),
        (
            "asset_trigger",
            j(b.asset_trigger(AssetTriggerRequest {
                target: live(),
                name: "x".into(),
            })
            .map(to)),
        ),
        (
            "asset_sound",
            j(b.asset_sound(AssetSoundRequest {
                target: live(),
                sound: SoundRef {
                    media_id: 1,
                    source: SourceKind::Embedded,
                    bank: "b".into(),
                    media_path: None,
                },
            })
            .map(to)),
        ),
        (
            "asset_export_sounds",
            j(b.asset_export_sounds(
                AssetExportSoundsRequest {
                    target: live(),
                    what: ship_guid(1),
                    sounds: vec![],
                    wem: false,
                },
                &noop,
            )
            .map(to)),
        ),
        (
            "asset_export_model",
            j(b.asset_export_model(
                AssetExportModelRequest {
                    target: live(),
                    what: ship_guid(1),
                    name: String::new(),
                    preset: ModelPreset::Full,
                    lod: 1,
                    stl: false,
                    force: false,
                },
                &noop,
            )
            .map(to)),
        ),
        (
            "asset_model_info",
            j(b.asset_model_info(AssetModelInfoRequest {
                target: live(),
                what: ship_guid(1),
                name: String::new(),
            })
            .map(to)),
        ),
        (
            "asset_model_view",
            j(b.asset_model_view(
                AssetModelViewRequest {
                    target: live(),
                    what: ship_guid(1),
                    name: String::new(),
                    lod: 3,
                    force: false,
                },
                &noop,
            )
            .map(to)),
        ),
        (
            "asset_model_chunk",
            j(b.asset_model_chunk(AssetModelChunkRequest {
                target: live(),
                what: ship_guid(1),
                lod: 3,
                offset: 0,
                len: 64,
            })
            .map(to)),
        ),
    ];
    for (name, r) in calls {
        assert_eq!(code(r), ErrorCode::DevDisabled, "{name}");
    }
    // The cache row in Settings works either way.
    assert!(b.asset_cache_status().is_ok());
    let e = ipc(
        "asset_list",
        json!({ "req": { "target": { "install": "inst-7f3a", "channel": "LIVE" }, "dir": "", "offset": 0, "limit": 5 } }),
    )
    .unwrap_err();
    assert_eq!(e["code"], "dev-disabled");
    let ok = ipc("asset_cache_status", json!({})).unwrap();
    assert_eq!(ok["is_default"], true);
}

#[test]
fn the_mock_caches_previews_sounds_and_models_and_switches_folders() {
    let b = dev_mock();
    assert!(!b.asset_preview(path(TEXTURE)).unwrap().cached);
    assert!(b.asset_preview(path(TEXTURE)).unwrap().cached);
    let c = b.asset_cache_status().unwrap();
    assert_eq!(c.files, 1);
    let s = b.asset_cache_pick().unwrap();
    assert!(!s.is_default && s.files == 0);
    assert_eq!(s.previous.as_ref().unwrap().files, 1);
    let s = b.asset_cache_move().unwrap();
    assert_eq!(s.files, 1);
    assert!(b.asset_cache_move().is_err());
    let e = b
        .asset_export_model(
            AssetExportModelRequest {
                target: live(),
                what: tag_guid(1),
                name: String::new(),
                preset: ModelPreset::Full,
                lod: 1,
                stl: false,
                force: false,
            },
            &noop,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    let picks = playable_picks(&b.asset_sounds(rec(ship_guid(3))).unwrap());
    assert_eq!(picks.len(), 4, "the unavailable one is left out");
}

#[test]
fn the_model_view_steps_through_lods_with_estimates_the_guard_and_the_cache() {
    use starorg_core::memlimit::GIB;
    let b = dev_mock();
    let info = || {
        b.asset_model_info(AssetModelInfoRequest {
            target: live(),
            what: ship_guid(2),
            name: "SYN_Ship_002".into(),
        })
        .unwrap()
    };
    let view = |lod, force| {
        b.asset_model_view(
            AssetModelViewRequest {
                target: live(),
                what: ship_guid(2),
                name: "SYN_Ship_002".into(),
                lod,
                force,
            },
            &noop,
        )
    };
    // Before any export: five steps, LOD 3 first, nothing cached, LOD 0 needs the most.
    let i = info();
    assert_eq!(i.lods.len(), 5);
    assert_eq!(i.default_lod, 3);
    assert!(i.available.is_empty());
    assert!(
        i.lods
            .iter()
            .all(|l| !l.cached && l.uses == l.lod && l.fit == MemoryFit::Ok)
    );
    assert!(i.lods[0].need > i.lods[3].need, "LOD 0 needs more memory");
    // A memory limit under the LOD 0 estimate: that step is refused, the others are fine.
    b.set_memory_limit(MemoryLimitRequest { gb: Some(1.5) })
        .unwrap();
    let i = info();
    assert_eq!(i.lods[0].fit, MemoryFit::OverLimit);
    assert_eq!(i.lods[3].fit, MemoryFit::Ok);
    let e = view(0, true).unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid, "force does not pass the limit");
    assert!(e.message.contains("memory limit"), "{}", e.message);
    assert!(info().lods.iter().all(|l| !l.cached), "nothing exported");
    // LOD 4 on a mesh with 0-3: LOD 3 is exported and cached once.
    let v = view(4, false).unwrap();
    assert_eq!((v.lod, v.asked, v.cached), (3, 4, false));
    assert_eq!(v.available, vec![0, 1, 2, 3]);
    let again = view(3, false).unwrap();
    assert!(again.cached && again.lod == 3 && again.bytes == v.bytes);
    let i = info();
    assert_eq!(i.lods[4].uses, 3);
    assert!(i.lods[3].cached && i.lods[4].cached && !i.lods[2].cached);
    assert_eq!(i.lods[3].bytes, Some(v.bytes));
    // The chunks put together are the GLB, with the triangles the view said.
    let mut glb = Vec::new();
    while (glb.len() as u64) < v.bytes {
        let c = b
            .asset_model_chunk(AssetModelChunkRequest {
                target: live(),
                what: ship_guid(2),
                lod: v.lod,
                offset: glb.len() as u64,
                len: 1000,
            })
            .unwrap();
        assert!(!c.is_empty() && c.len() <= 1000);
        glb.extend(c);
    }
    assert_eq!(
        starorg_data::assets::stl::glb_triangle_count(&glb),
        v.triangles
    );
    assert_eq!(&glb[..4], b"glTF");
    let past = b.asset_model_chunk(AssetModelChunkRequest {
        target: live(),
        what: ship_guid(2),
        lod: v.lod,
        offset: v.bytes + 1,
        len: 10,
    });
    assert_eq!(past.unwrap_err().code, ErrorCode::Invalid);
    // An LOD not loaded has no bytes to hand over; more detail is more triangles.
    let not = b.asset_model_chunk(AssetModelChunkRequest {
        target: live(),
        what: ship_guid(2),
        lod: 1,
        offset: 0,
        len: 10,
    });
    assert_eq!(not.unwrap_err().code, ErrorCode::NotFound);
    let detailed = view(1, false).unwrap();
    assert!(detailed.triangles > v.triangles && detailed.bytes > v.bytes);
    // A big limit again; the mock's 18.4 GB free is enough.
    b.set_memory_limit(MemoryLimitRequest { gb: None }).unwrap();
    assert!(info().free.unwrap() > GIB);
    // Export .glb of the viewed LOD is a cache copy.
    let r = b
        .asset_export_model(
            AssetExportModelRequest {
                target: live(),
                what: ship_guid(2),
                name: "SYN_Ship_002".into(),
                preset: ModelPreset::PrintShell,
                lod: 3,
                stl: false,
                force: false,
            },
            &noop,
        )
        .unwrap();
    assert!(r.cached);
    assert_eq!(r.bytes, v.bytes);
    // Not an entity: refused.
    let e = b
        .asset_model_info(AssetModelInfoRequest {
            target: live(),
            what: tag_guid(1),
            name: String::new(),
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
}

#[test]
fn model_chunks_are_raw_bytes_over_ipc() {
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    let b = dev_mock();
    b.asset_model_view(
        AssetModelViewRequest {
            target: live(),
            what: ship_guid(1),
            name: String::new(),
            lod: 3,
            force: false,
        },
        &noop,
    )
    .unwrap();
    let app = crate::builder(
        mock_builder(),
        crate::commands::DiscoveryConfig::default(),
        super::BackendState::new(b),
    )
    .build(mock_context(noop_assets()))
    .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let r = get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "asset_model_chunk".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: tauri::ipc::InvokeBody::Json(json!({ "req": {
                "target": { "install": "inst-7f3a", "channel": "LIVE" },
                "what": ship_guid(1), "lod": 3, "offset": 0, "len": 16
            }})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .unwrap();
    match r {
        tauri::ipc::InvokeResponseBody::Raw(bytes) => {
            assert_eq!(bytes.len(), 16);
            assert_eq!(&bytes[..4], b"glTF");
        }
        other => panic!("not raw bytes: {other:?}"),
    }
}

#[test]
fn the_mock_models_are_small_valid_glbs_less_detailed_per_lod() {
    let tris = |ship, lod| {
        starorg_data::assets::stl::glb_triangle_count(&mock_assets::model_glb(ship, lod)).unwrap()
    };
    for ship in [true, false] {
        let t: Vec<u64> = (0..=4).map(|l| tris(ship, l)).collect();
        assert!(t.windows(2).all(|w| w[0] >= w[1]), "{t:?}");
        assert!(mock_assets::model_glb(ship, 0).len() < 64 * 1024);
    }
    assert_eq!(tris(true, 4), tris(true, 3), "LOD 4 is LOD 3");
    assert_eq!(
        mock_assets::model_glb(true, 4),
        mock_assets::model_glb(true, 3)
    );
    let s = starorg_data::assets::stl::glb_triangles(&mock_assets::model_glb(true, 0)).unwrap();
    assert_eq!(s.len() as u64, tris(true, 0));
}

/// `mock-assets.gen.json`: the mock's synthetic archive and sounds, read by the TS mock.
/// Regenerated with the samples (`UPDATE_CONTRACT_SAMPLES=1`).
#[test]
fn mock_assets_data_matches_the_committed_file() {
    let path = app_src().join("mock-assets.gen.json");
    let text = serde_json::to_string(&mock_assets::gen_file()).unwrap() + "\n";
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
