use super::files::{self, PreviewKind};
use super::model::{ModelOptions, ModelPreset};
use super::*;
use crate::starbreaker::synthetic as dcb;

/// The synthetic archive under `<tmp>/game/Data.p4k`, opened with the count check.
pub(crate) fn open() -> (tempfile::TempDir, AssetArchive) {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("game").join("Data.p4k");
    synthetic::write(&p).unwrap();
    let a = AssetArchive::open(&p).unwrap();
    (t, a)
}

#[test]
fn opens_lazily_and_lists_folders_first() {
    let (_t, a) = open();
    assert_eq!(a.entries(), synthetic::files().len());
    assert!(
        !a.db_ready() && !a.audio_ready(),
        "nothing read before a request"
    );
    let root = files::list(&a, "", 0, 100).unwrap();
    assert_eq!(root.parent, None);
    assert_eq!(root.entries.len(), 1);
    assert!(root.entries[0].dir && root.entries[0].name == "Data");
    let data = files::list(&a, "Data/", 0, 100).unwrap();
    let names: Vec<&str> = data.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["Objects", "Scripts", "Sounds", "Textures", "Game2.dcb"]
    );
    assert_eq!(data.parent.as_deref(), Some(""));
    let page = files::list(&a, "Data", 1, 2).unwrap();
    assert_eq!(page.total, 5);
    assert_eq!(page.entries.len(), 2);
    assert_eq!(page.entries[0].name, "Scripts");
    assert!(files::list(&a, "Data\\Nope", 0, 10).is_err());
    assert!(!a.db_ready(), "listing never reads the DataCore");
}

#[test]
fn search_matches_every_word() {
    let (_t, a) = open();
    let s = files::search(&a, "syn hull", 10);
    let paths: Vec<&str> = s.hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(s.total, 4);
    assert!(paths.contains(&synthetic::TEXTURE));
    assert!(paths.windows(2).all(|w| w[0] <= w[1]), "sorted");
    assert_eq!(files::search(&a, "Data/Scripts", 10).total, 2);
    assert_eq!(files::search(&a, "", 10).total, 0);
}

#[test]
fn previews_textures_text_and_audio() {
    let (t, a) = open();
    let out = t.path().join("cache").join("p.png");
    let p = files::preview(&a, &synthetic::TEXTURE.replace('\\', "/"), &out).unwrap();
    assert_eq!(p.kind, PreviewKind::Image);
    assert_eq!(
        (p.width, p.height),
        (Some(8), Some(8)),
        "mip sibling merged"
    );
    let png = std::fs::read(&out).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let x = files::preview(&a, synthetic::XML, &out).unwrap();
    assert_eq!(x.kind, PreviewKind::Text);
    assert!(x.text.unwrap().contains("speed"));
    let w = files::preview(&a, synthetic::WEM, &out).unwrap();
    assert_eq!(w.kind, PreviewKind::Audio);
    let s = files::preview(&a, "Data\\Textures\\ships\\syn_hull_diff.dds.1", &out).unwrap();
    assert_eq!(s.kind, PreviewKind::None);
    let b = files::preview(&a, "Data\\Objects\\ships\\syn\\syn_hull.skin", &out).unwrap();
    assert_eq!(b.kind, PreviewKind::None);
    assert!(files::preview(&a, "Data\\none.xml", &out).is_err());
    // Never into the game folder.
    let bad = t.path().join("game").join("p.png");
    let e = files::preview(&a, synthetic::TEXTURE, &bad).unwrap_err();
    assert!(e.message.contains("exports never go there"), "{e}");
    assert!(!bad.exists());
}

#[test]
fn extracts_a_folder_raw_and_converted() {
    let (t, a) = open();
    let plan = files::plan_extract(&a, &["Data\\Textures".into(), synthetic::XML.into()]).unwrap();
    let rels: Vec<String> = plan
        .iter()
        .map(|i| i.rel.to_string_lossy().replace('\\', "/"))
        .collect();
    assert_eq!(
        rels,
        [
            "Textures/ships/syn_hull_diff.dds",
            "Textures/ships/syn_hull_diff.dds.1",
            "syn_config.xml"
        ]
    );
    let raw = t.path().join("raw");
    let mut steps = Vec::new();
    let d = files::extract_all(&a, &plan, &raw, false, &mut |k, n| {
        steps.push((k, n));
        true
    })
    .unwrap();
    assert_eq!((d.files, d.converted, d.merged, d.failures), (3, 0, 0, 0));
    assert_eq!(steps.last(), Some(&(3, 3)));
    assert!(raw.join("Textures/ships/syn_hull_diff.dds.1").exists());
    let conv = t.path().join("conv");
    let d = files::extract_all(&a, &plan, &conv, true, &mut |_, _| true).unwrap();
    assert_eq!((d.files, d.converted, d.merged), (2, 1, 1));
    assert!(conv.join("Textures/ships/syn_hull_diff.png").exists());
    assert!(!conv.join("Textures/ships/syn_hull_diff.dds.1").exists());
    // Cancel after the first file: what is written stays, the call says cancelled.
    let c = t.path().join("cancel");
    let e = files::extract_all(&a, &plan, &c, false, &mut |k, _| k < 1).unwrap_err();
    assert_eq!(e.code, "cancelled");
    // Into the game folder: refused before anything is written.
    let g = t.path().join("game").join("out");
    let e = files::extract_all(&a, &plan, &g, false, &mut |_, _| true).unwrap_err();
    assert!(e.message.contains("exports never go there"));
    assert!(!g.exists());
    assert!(files::plan_extract(&a, &["Data\\Missing".into()]).is_err());
}

#[test]
fn unsafe_archive_paths_are_refused() {
    assert!(files::safe_rel("a\\b.xml").is_some());
    for bad in ["..\\x", "a\\..\\..\\x", "C:\\x", "\\abs", "a\\\\b", ""] {
        assert!(files::safe_rel(bad).is_none(), "{bad}");
    }
    assert!(files::is_dds_sibling("x.dds.1"));
    assert!(files::is_dds_sibling("x.DDS.3a"));
    assert!(files::is_dds_sibling("x.dds.a"));
    assert!(!files::is_dds_sibling("x.dds"));
    assert!(!files::is_dds_sibling("x.dds.bak"));
}

#[test]
fn records_are_found_by_guid_and_name_once_the_datacore_is_read() {
    let (_t, a) = open();
    let db = a.db().unwrap();
    assert!(a.db_ready());
    let by_guid = find_record(db, &dcb::guid_text(1, 3)).unwrap();
    assert_eq!(record_short_name(db, by_guid), "SYN_Item_003");
    let by_name = find_record(db, "syn_item_003").unwrap();
    assert_eq!(by_name.id, by_guid.id);
    assert!(find_record(db, "nope").is_err());
}

#[test]
fn sounds_build_the_audio_index_on_demand() {
    let (_t, a) = open();
    let s = audio::record_sounds(&a, &dcb::guid_text(1, 2)).unwrap();
    assert!(a.audio_ready());
    assert_eq!(s.name, "SYN_Item_002");
    assert!(s.triggers.is_empty());
    let idx = a.audio().unwrap();
    let r = audio::SoundRef {
        media_id: 123456,
        source: audio::SourceKind::MediaFile,
        bank: String::new(),
        media_path: None,
    };
    let wem = idx.media_bytes(&a, &r).unwrap();
    let c = audio::decode(&wem).unwrap();
    assert_eq!(c.format.ext(), "wav", "PCM becomes WAV");
    assert_eq!(c.bytes.len(), 44 + 480 * 2);
    let odd = audio::SoundRef {
        media_id: 123457,
        ..r.clone()
    };
    let e = audio::decode(&idx.media_bytes(&a, &odd).unwrap()).unwrap_err();
    assert_eq!(e.codec, Some(super::wemcodec::Codec::Other(0x8311)));
    let missing = audio::SoundRef {
        media_id: 9,
        ..r.clone()
    };
    assert!(idx.media_bytes(&a, &missing).is_err());
    assert!(audio::decode(b"not a wem").is_err());
    assert_eq!(
        audio::file_name("Play Engine/Loop", &r, "ogg"),
        "Play_Engine_Loop_123456.ogg"
    );
    let ext_ref = audio::SoundRef {
        media_path: Some("Data\\Sounds\\x\\voice_01.wem".into()),
        source: audio::SourceKind::External,
        ..r
    };
    assert!(ext_ref.key().starts_with('x'));
    assert_eq!(audio::file_name("", &ext_ref, "wem"), "voice_01.wem");
}

#[test]
fn models_need_an_entity_and_respect_the_guard() {
    let (t, a) = open();
    let o = ModelOptions {
        preset: ModelPreset::PrintShell,
        lod: 0,
        stl: false,
    };
    let out = t.path().join("m.glb");
    let e = model::export_model(&a, &dcb::guid_text(1, 1), &o, &out, &|_, _| true).unwrap_err();
    assert_eq!(e.code, "invalid");
    assert!(e.message.contains("not an entity"), "{e}");
    let e = model::export_model(
        &a,
        "x",
        &o,
        &t.path().join("game").join("m.glb"),
        &|_, _| true,
    )
    .unwrap_err();
    assert!(e.message.contains("exports never go there"));
    let e = model::export_model(&a, "x", &o, &out, &|_, _| false).unwrap_err();
    assert_eq!(e.code, "cancelled");
    assert_eq!(
        model::file_name("AEGS Javelin", &o),
        "AEGS_Javelin-print-lod0.glb"
    );
}

#[test]
fn presets_map_to_starbreaker_options() {
    use starbreaker_3d::MaterialMode;
    let full = model::export_options(&ModelOptions {
        preset: ModelPreset::Full,
        lod: 1,
        stl: false,
    });
    assert_eq!(full.material_mode, MaterialMode::Textures);
    assert!(full.include_interior && full.include_lights && full.include_attachments);
    assert_eq!((full.lod_level, full.texture_mip), (1, 2));
    // export_ships.sh: --lod 0 --materials none --no-interior --no-lights
    let print = model::export_options(&ModelOptions {
        preset: ModelPreset::PrintShell,
        lod: 9,
        stl: true,
    });
    assert_eq!(print.material_mode, MaterialMode::None);
    assert!(!print.include_interior && !print.include_lights && print.include_attachments);
    assert_eq!(print.lod_level, model::MAX_LOD);
    assert_eq!(print.kind, starbreaker_3d::ExportKind::Bundled);
}

/// A GLB with one triangle on a node translated by (0, 0, 5): the STL has it moved and
/// turned Z-up.
fn one_triangle_glb() -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [[0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in v {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    for i in [0u16, 1, 2] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    bin.extend_from_slice(&[0, 0]);
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"children": [1], "translation": [0.0, 0.0, 5.0]}, {"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 6}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
             "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
            {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
        ]
    });
    let mut j = serde_json::to_vec(&json).unwrap();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let total = 12 + 8 + j.len() + 8 + bin.len();
    let mut g = Vec::new();
    g.extend_from_slice(b"glTF");
    g.extend_from_slice(&2u32.to_le_bytes());
    g.extend_from_slice(&(total as u32).to_le_bytes());
    g.extend_from_slice(&(j.len() as u32).to_le_bytes());
    g.extend_from_slice(b"JSON");
    g.extend_from_slice(&j);
    g.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    g.extend_from_slice(b"BIN\0");
    g.extend_from_slice(&bin);
    g
}

#[test]
fn glb_to_stl_bakes_transforms() {
    let tris = stl::glb_triangles(&one_triangle_glb()).unwrap();
    assert_eq!(tris.len(), 1);
    // glTF (x, y, z) -> STL (x, -z, y); the node moved z by 5.
    assert_eq!(tris[0][0], [0.0, -5.0, 0.0]);
    assert_eq!(tris[0][1], [1.0, -5.0, 0.0]);
    assert_eq!(tris[0][2], [0.0, -5.0, 1.0]);
    let t = tempfile::tempdir().unwrap();
    let out = t.path().join("x.stl");
    let (n, len) = stl::glb_file_to_stl(&one_triangle_glb(), &out).unwrap();
    assert_eq!((n, len), (1, 134));
    let b = std::fs::read(&out).unwrap();
    assert_eq!(b.len(), 134);
    assert_eq!(u32::from_le_bytes(b[80..84].try_into().unwrap()), 1);
    assert!(stl::glb_triangles(b"nope").is_err());
    assert_eq!(stl::glb_triangle_count(&one_triangle_glb()), Some(1));
    assert_eq!(stl::glb_triangle_count(b"nope"), None);
    std::fs::write(t.path().join("one.glb"), one_triangle_glb()).unwrap();
    assert_eq!(
        stl::glb_file_triangle_count(&t.path().join("one.glb")),
        Some(1)
    );
    assert_eq!(
        stl::glb_file_triangle_count(&out),
        None,
        "an STL is not a GLB"
    );
}

#[test]
fn model_cache_keys_are_per_record_preset_and_lod() {
    let o = |preset, lod| ModelOptions {
        preset,
        lod,
        stl: false,
    };
    let g = "8f1c2d3e-0000-4000-8000-000000000001";
    let keys: Vec<String> = (0..=model::MAX_LOD)
        .map(|l| o(ModelPreset::PrintShell, l).cache_key(g))
        .collect();
    assert_eq!(keys[3], format!("{g}-print-lod3"));
    let unique: std::collections::HashSet<&String> = keys.iter().collect();
    assert_eq!(unique.len(), keys.len(), "one file per LOD");
    assert_ne!(
        o(ModelPreset::Full, 3).cache_key(g),
        o(ModelPreset::PrintShell, 3).cache_key(g)
    );
    // The STL switch is not part of the key (the STL is made from the cached GLB).
    let mut s = o(ModelPreset::PrintShell, 3);
    s.stl = true;
    assert_eq!(s.cache_key(g), keys[3]);
    // A name with unsafe characters is hashed, still per LOD.
    let a = o(ModelPreset::PrintShell, 0).cache_key("AEGS Javelin/x");
    let b = o(ModelPreset::PrintShell, 4).cache_key("AEGS Javelin/x");
    assert!(a.ends_with("-print-lod0") && b.ends_with("-print-lod4") && a[..32] == b[..32]);
}

#[test]
fn mesh_lods_are_kept_in_the_cache_so_a_missing_lod_finds_the_one_below() {
    use starorg_core::assetcache::{AssetCache, Kind};
    let t = tempfile::tempdir().unwrap();
    let c = AssetCache::new(t.path().join("assets"), 1 << 30);
    let b = "00000000000000000000000b";
    let g = "8f1c2d3e-0000-4000-8000-000000000001";
    assert!(model::cached_lods(&c, "LIVE", b, g).is_empty());
    model::cache_lods(&c, "LIVE", b, g, &[]).unwrap();
    assert!(
        model::cached_lods(&c, "LIVE", b, g).is_empty(),
        "nothing for unknown"
    );
    model::cache_lods(&c, "LIVE", b, g, &[0, 1, 2, 3]).unwrap();
    let known = model::cached_lods(&c, "LIVE", b, g);
    assert_eq!(known, vec![0, 1, 2, 3]);
    // LOD 4 asked: the key is LOD 3's, which an earlier export cached.
    let o = ModelOptions {
        preset: ModelPreset::PrintShell,
        lod: model::effective_lod(4, &known),
        stl: false,
    };
    assert_eq!(o.cache_key(g), format!("{g}-print-lod3"));
    // Our file: counted, cleared with the rest; garbage reads as unknown.
    assert_eq!(c.usage().files, 1);
    std::fs::write(
        c.path("LIVE", b, Kind::Models, &format!("{g}-lods"), "lods"),
        "x,7",
    )
    .unwrap();
    assert!(model::cached_lods(&c, "LIVE", b, g).is_empty());
    assert_eq!(c.clear().unwrap().files, 1);
}

#[test]
fn lods_come_from_the_mesh_files_and_clamp_the_asked_one() {
    let files = [
        "Data\\Objects\\Spaceships\\Ships\\AEGS\\Gladius\\AEGS_Gladius.cgam",
        "Data\\Objects\\Spaceships\\Ships\\AEGS\\Gladius\\AEGS_Gladius_lod1.cgam",
        "Data\\Objects\\Spaceships\\Ships\\AEGS\\Gladius\\AEGS_Gladius_lod2.cgam",
        "Data\\Objects\\Spaceships\\Ships\\AEGS\\Gladius\\AEGS_Gladius_lod3.cgam",
    ];
    let exists = |p: &str| files.iter().any(|f| f.eq_ignore_ascii_case(p));
    let lods = model::lods_in(
        "Objects/Spaceships/Ships/AEGS/Gladius/AEGS_Gladius.cga",
        exists,
    );
    assert_eq!(lods, vec![0, 1, 2, 3]);
    assert_eq!(
        model::lods_in(
            "Data/Objects/Spaceships/Ships/AEGS/Gladius/aegs_gladius.cga",
            exists
        ),
        vec![0, 1, 2, 3],
        "a Data/ prefix and any case"
    );
    assert_eq!(
        model::effective_lod(4, &lods),
        3,
        "LOD 4 is not there: LOD 3"
    );
    assert_eq!(model::effective_lod(2, &lods), 2);
    assert_eq!(
        model::effective_lod(9, &[]),
        model::MAX_LOD,
        "unknown: as asked"
    );
    assert_eq!(model::effective_lod(3, &[0, 1]), 1);
    assert_eq!(model::view_lod(&lods), 3);
    assert_eq!(model::view_lod(&[0]), 0);
    assert_eq!(model::view_lod(&[]), model::VIEW_LOD);
    assert!(model::lods_in("Objects/Characters/x.cdf", exists).is_empty());
    assert!(model::lods_in("", exists).is_empty());
    assert_eq!(model::lods_in("Objects/x.cgf", |_| false), vec![0]);
}

#[test]
fn globs_match_within_and_across_folders() {
    use files::glob_match as m;
    assert!(m("Data/Textures/*.dds", "Data\\Textures\\a.dds"));
    assert!(!m("Data/Textures/*.dds", "Data\\Textures\\ships\\a.dds"));
    assert!(m("Data/**/*.dds", "Data\\Textures\\ships\\A.DDS"));
    assert!(m("data\\**.xml", "Data\\Scripts\\syn_config.xml"));
    assert!(m(
        "Data/Scripts/syn_?onfig.xml",
        "Data\\Scripts\\syn_config.xml"
    ));
    assert!(!m("Data/Scripts/?", "Data\\Scripts\\ab"));
    assert!(!m("*.dds", "Data\\x.dds"));
    assert!(files::is_glob("a*") && !files::is_glob("Data\\x"));
    let (_t, a) = open();
    let plan = files::plan_glob(&a, "Data/**/*.dds*").unwrap();
    assert_eq!(plan.len(), 2);
    assert!(plan[0].rel.starts_with("Data"));
    assert!(files::plan_glob(&a, "nothing*").is_err());
}

#[test]
fn sound_search_finds_triggers_by_name_event_and_words() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("game").join("Data.p4k");
    synthetic::write_with_audio(&p).unwrap();
    let a = AssetArchive::open(&p).unwrap();
    let idx = a.audio().unwrap();
    let r = idx.search_triggers("engine", 50);
    assert_eq!(r.total, 1);
    assert_eq!(r.hits[0].name, "Play_SYN_Engine_Loop");
    assert_eq!(r.hits[0].duration_type.as_deref(), Some("Infinite"));
    // Words in any order; the event name counts too.
    assert_eq!(
        idx.search_triggers("boom syn", 50).hits[0].name,
        "Play_SYN_Explosion_Big"
    );
    assert_eq!(idx.search_triggers("syn", 50).total, 2);
    assert_eq!(idx.search_triggers("", 50).total, 0);
    let t = idx.trigger(&a, "Play_SYN_Engine_Loop").unwrap();
    assert_eq!(t.event.as_deref(), Some("Play_SYN_Engine_Loop"));
    assert_eq!(idx.trigger(&a, "Nope").unwrap_err().code, "not-found");
}
