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
