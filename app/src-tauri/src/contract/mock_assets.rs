//! Developer mode assets in the mock backend (addendum 7.16): a synthetic archive tree, a
//! texture preview (an 8 x 8 PNG), XML text, sounds for the mock's ships and coolers (a 0.25 s
//! Ogg tone), and a cache folder whose size follows what was "played" and "exported". The
//! "dialogs" always pick `/synthetic/exports`. `mock-assets.gen.json` (written by the contract
//! tests) carries the data to the TS mock (`app/src/lib/contract/mock-assets.ts`).

use super::Progress;
use super::assets::*;
use super::dev::{WorkerState, disabled};
use super::mock::MockBackend;
use super::mock_dev::{COMPONENTS, SHIPS, component_guid, ship_guid};
use super::types::*;
use serde_json::{Value, json};

/// An 8 x 8 RGBA PNG (the texture preview).
pub const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAXklEQVR42hXKMQHAMAgAsEqpFKQgBSlIQQpOtvTIl3NOf5cgKZphOecKBEnRDHtfCIEgKZph44UUCJKiGTZfKIEgKZph64UWCJKiGbZfGIEgKZph54UVCJKiGXb7+wEW3J/B5ia3YAAAAABJRU5ErkJggg==";
/// A 0.25 s 440 Hz Ogg Vorbis tone (every mock sound).
pub const OGG_B64: &str = "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";

pub const EXPORTS: &str = "/synthetic/exports";
pub const CACHE_DEFAULT: &str = "/synthetic/cache/assets";
pub const CACHE_OTHER: &str = "/synthetic/other-drive/asset-cache";
pub const CACHE_MAX: u64 = 2 * 1024 * 1024 * 1024;
/// The mock's STL and 3MF of a print-shell export: an AEGS_Javelin LOD 0's STL, and a 3MF a
/// quarter of it (an RSI_Skylark's 3MF was 0.25 of its STL).
pub const STL_BYTES: u64 = 183_149_634;
pub const THREE_MF_BYTES: u64 = 45_000_000;

/// Mock asset state (per backend).
#[derive(Debug, Default)]
pub struct AssetMock {
    pub open: Option<(String, String)>,
    pub datacore: bool,
    pub audio: bool,
    /// Cache keys held, with sizes.
    pub cached: Vec<(String, u64)>,
    pub custom_dir: bool,
    /// (folder, bytes, files) of the folder before the last switch.
    pub previous: Option<(String, u64, u64)>,
    /// Records whose mesh LODs a model view read (GUID -> LODs), as the real backend.
    pub lods: Vec<(String, Vec<u32>)>,
}

/// The LODs of a mock model's mesh: ships 0-3 (LOD 4 shows LOD 3), items 0-1.
pub fn mock_lods(ship: bool) -> Vec<u32> {
    if ship { vec![0, 1, 2, 3] } else { vec![0, 1] }
}

/// A box from `a` to `b`, 24 vertices (flat normals when the viewer computes them).
fn glb_box(pos: &mut Vec<[f32; 3]>, idx: &mut Vec<u16>, a: [f32; 3], b: [f32; 3]) {
    let c = |x: usize, y: usize, z: usize| {
        [
            if x == 0 { a[0] } else { b[0] },
            if y == 0 { a[1] } else { b[1] },
            if z == 0 { a[2] } else { b[2] },
        ]
    };
    let faces: [[(usize, usize, usize); 4]; 6] = [
        [(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 0, 0)],
        [(0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)],
        [(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 1, 0)],
        [(1, 0, 0), (1, 1, 0), (1, 1, 1), (1, 0, 1)],
        [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)],
        [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
    ];
    for f in faces {
        let base = pos.len() as u16;
        pos.extend(f.iter().map(|&(x, y, z)| c(x, y, z)));
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// A lathe along Z (`len` metres, `radius` at its widest), `seg` around and `rings` along.
fn glb_hull(
    pos: &mut Vec<[f32; 3]>,
    idx: &mut Vec<u16>,
    len: f32,
    radius: f32,
    seg: u32,
    rings: u32,
) {
    for r in 0..=rings {
        let t = r as f32 / rings as f32;
        let rad = radius * (std::f32::consts::PI * t).sin().powf(0.6).max(0.02);
        // Flatter than round: a hull, not a tube.
        for k in 0..seg {
            let a = std::f32::consts::TAU * k as f32 / seg as f32;
            pos.push([rad * a.cos(), rad * 0.55 * a.sin(), len * (t - 0.5)]);
        }
    }
    for r in 0..rings {
        for k in 0..seg {
            let i = |r: u32, k: u32| (r * seg + k % seg) as u16;
            idx.extend([
                i(r, k),
                i(r + 1, k),
                i(r + 1, k + 1),
                i(r, k),
                i(r + 1, k + 1),
                i(r, k + 1),
            ]);
        }
    }
}

fn round4(v: &mut Vec<u8>, pad: u8) {
    while !v.len().is_multiple_of(4) {
        v.push(pad);
    }
}

/// The mock's model: a GLB of a small ship (22 m long, 16 m wide) or an item (1.2 m), less
/// detailed with each LOD (above the mesh's last LOD, that one), no materials, as a Print
/// shell export. Three nodes under a root, one with a transform, as StarBreaker's GLBs.
pub fn model_glb(ship: bool, lod: u32) -> Vec<u8> {
    let lod = starorg_data::assets::model::effective_lod(lod, &mock_lods(ship)) as usize;
    let seg = [32, 20, 12, 8, 6][lod];
    let rings = [40, 24, 14, 8, 5][lod];
    let mut meshes: Vec<(Vec<[f32; 3]>, Vec<u16>)> = Vec::new();
    let (mut p, mut i) = (Vec::new(), Vec::new());
    if ship {
        glb_hull(&mut p, &mut i, 22.0, 2.4, seg, rings);
        meshes.push((p, i));
        let (mut p, mut i) = (Vec::new(), Vec::new());
        glb_box(&mut p, &mut i, [-8.0, -0.2, 1.0], [-1.6, 0.2, 7.0]);
        glb_box(&mut p, &mut i, [1.6, -0.2, 1.0], [8.0, 0.2, 7.0]);
        meshes.push((p, i));
        let (mut p, mut i) = (Vec::new(), Vec::new());
        glb_box(&mut p, &mut i, [-0.15, 0.0, -1.5], [0.15, 2.4, 1.5]);
        meshes.push((p, i));
    } else {
        glb_hull(&mut p, &mut i, 1.2, 0.5, seg, rings.min(12));
        meshes.push((p, i));
    }
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut gl_meshes = Vec::new();
    for (n, (pos, idx)) in meshes.iter().enumerate() {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        let start = bin.len();
        for v in pos {
            for c in 0..3 {
                lo[c] = lo[c].min(v[c]);
                hi[c] = hi[c].max(v[c]);
                bin.extend_from_slice(&v[c].to_le_bytes());
            }
        }
        views.push(json!({"buffer": 0, "byteOffset": start, "byteLength": bin.len() - start, "target": 34962}));
        let istart = bin.len();
        for x in idx {
            bin.extend_from_slice(&x.to_le_bytes());
        }
        views.push(json!({"buffer": 0, "byteOffset": istart, "byteLength": bin.len() - istart, "target": 34963}));
        round4(&mut bin, 0);
        accessors.push(json!({"bufferView": 2 * n, "componentType": 5126, "count": pos.len(), "type": "VEC3", "min": lo, "max": hi}));
        accessors.push(json!({"bufferView": 2 * n + 1, "componentType": 5123, "count": idx.len(), "type": "SCALAR"}));
        gl_meshes.push(
            json!({"primitives": [{"attributes": {"POSITION": 2 * n}, "indices": 2 * n + 1}]}),
        );
    }
    let names = ["Hull", "Wings", "Fin"];
    let mut nodes = vec![
        json!({"name": if ship { "SYN_Ship" } else { "SYN_Item" }, "children": (1..=meshes.len()).collect::<Vec<_>>()}),
    ];
    for (n, name) in names.iter().enumerate().take(meshes.len()) {
        let mut node = json!({"name": name, "mesh": n});
        if n == 2 {
            node["translation"] = json!([0.0, 1.1, 7.5]);
        }
        nodes.push(node);
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "StarOrg Codex mock"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": nodes,
        "meshes": gl_meshes,
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": views,
        "accessors": accessors,
    });
    let mut j = serde_json::to_vec(&doc).expect("json");
    round4(&mut j, b' ');
    let total = 12 + 8 + j.len() + 8 + bin.len();
    let mut g = Vec::with_capacity(total);
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

/// Every file of the synthetic archive: (path, size).
pub fn files() -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = vec![
        ("Data\\Game2.dcb".into(), 352_190_114),
        ("Data\\Libs\\Config\\defaultProfile.xml".into(), 48_211),
        ("Data\\Libs\\Config\\syn_keybinds.xml".into(), 9_870),
        ("Data\\Scripts\\readme.txt".into(), 10),
    ];
    for n in 0..6u32 {
        let d = format!("Data\\Objects\\Spaceships\\Ships\\SYN\\syn_ship_{n:03}");
        v.push((
            format!("{d}\\syn_ship_{n:03}.skin"),
            1_200_000 + n as u64 * 7_000,
        ));
        v.push((
            format!("{d}\\syn_ship_{n:03}.skinm"),
            9_400_000 + n as u64 * 11_000,
        ));
        v.push((format!("{d}\\syn_ship_{n:03}.mtl"), 22_000 + n as u64 * 100));
        let t = format!("Data\\Textures\\ships\\syn\\syn_ship_{n:03}_diff.dds");
        v.push((t.clone(), 174));
        v.push((format!("{t}.1"), 4_194_304));
        v.push((format!("{t}.2"), 1_048_576));
    }
    for n in 0..4u32 {
        v.push((
            format!("Data\\Sounds\\wwise\\SYN_Ship_{n:02}.bnk"),
            812_000 + n as u64 * 3_000,
        ));
        v.push((
            format!("Data\\Sounds\\wwise\\Media\\{}.wem", 700_000 + n),
            64_000,
        ));
    }
    v.sort();
    v
}

pub fn xml_text(path: &str) -> String {
    format!(
        "<Config path=\"{}\">\n  <Value key=\"speed\" v=\"200\"/>\n  <Value key=\"mass\" v=\"1200\"/>\n</Config>\n",
        path.replace('\\', "/")
    )
}

fn sound(id: u32, playable: bool, bank: &str) -> SoundInfo {
    SoundInfo {
        sound: SoundRef {
            media_id: id,
            source: if playable {
                SourceKind::Embedded
            } else {
                SourceKind::Unavailable
            },
            bank: bank.into(),
            media_path: None,
        },
        label: id.to_string(),
        playable,
        detail: if playable {
            "embedded in the bank".into()
        } else {
            "the bank holds no data for it and no loose media file was found".into()
        },
    }
}

/// Every trigger the mock knows (ships' and coolers'), for the Sounds search (7.17).
pub fn all_triggers() -> Vec<TriggerInfo> {
    let mut out: Vec<TriggerInfo> = Vec::new();
    let names = (0..SHIPS)
        .map(ship_guid)
        .chain((0..COMPONENTS).filter(|n| n % 4 == 0).map(component_guid));
    for g in names {
        for t in record_sounds(&g).map(|s| s.triggers).unwrap_or_default() {
            if !out.iter().any(|x| x.name == t.name) {
                out.push(t);
            }
        }
    }
    out
}

/// The trigger search over [`all_triggers`]: every word in the name, event or bank; name
/// starts first, then shorter names.
pub fn search_triggers(query: &str, limit: usize) -> TriggerSearch {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return TriggerSearch::default();
    }
    let joined = words.join("_");
    let mut hits: Vec<(u8, TriggerInfo)> = all_triggers()
        .into_iter()
        .filter_map(|t| {
            let n = t.name.to_lowercase();
            let other = format!(
                "{} {}",
                t.event.clone().unwrap_or_default(),
                t.bank.clone().unwrap_or_default()
            )
            .to_lowercase();
            if !words
                .iter()
                .all(|w| n.contains(w.as_str()) || other.contains(w.as_str()))
            {
                return None;
            }
            let rank = if n.starts_with(&joined) || n.contains(&format!("_{joined}")) {
                0
            } else if words.iter().all(|w| n.contains(w.as_str())) {
                1
            } else {
                2
            };
            Some((rank, t))
        })
        .collect();
    let total = hits.len() as u32;
    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.name.len().cmp(&b.1.name.len()))
            .then(a.1.name.cmp(&b.1.name))
    });
    TriggerSearch {
        hits: hits
            .into_iter()
            .take(limit.min(500))
            .map(|(_, t)| TriggerHit {
                event: t.event.unwrap_or_default(),
                bank: t.bank.unwrap_or_default(),
                duration_type: t.duration_type,
                name: t.name,
            })
            .collect(),
        total,
    }
}

/// The sounds of a mock record: ships have an engine loop, a thruster and a door; coolers a
/// hum; everything else none.
pub fn record_sounds(what: &str) -> Option<RecordSounds> {
    let ship = (0..SHIPS).find(|&n| ship_guid(n) == what || format!("SYN_Ship_{n:03}") == what);
    let cooler = (0..COMPONENTS)
        .filter(|n| n % 4 == 0)
        .find(|&n| component_guid(n) == what || format!("SYN_cooler_{n:03}") == what);
    let trig = |name: String, from: String, bank: &str, sounds: Vec<SoundInfo>| TriggerInfo {
        property: "Components[SAudioComponentParams].audioTrigger".into(),
        from,
        bank: Some(bank.into()),
        event: Some(name.clone()),
        duration_type: Some("Infinite".into()),
        name,
        sounds,
    };
    let (record, name, triggers, loadout) = if let Some(n) = ship {
        let bank = format!("SYN_Ship_{:02}.bnk", n % 4);
        let base = 700_000 + n * 10;
        let me = format!("SYN_Ship_{n:03}");
        (
            ship_guid(n),
            me.clone(),
            vec![
                trig(
                    format!("Play_SYN_Ship_{n:03}_Engine_Loop"),
                    me.clone(),
                    &bank,
                    vec![sound(base, true, &bank), sound(base + 1, true, &bank)],
                ),
                trig(
                    format!("Play_SYN_Ship_{n:03}_Door_Open"),
                    me.clone(),
                    &bank,
                    vec![sound(base + 2, true, &bank), sound(base + 3, false, &bank)],
                ),
                trig(
                    "Play_SYN_Thruster_Main".into(),
                    "SYN_Thruster_Main".into(),
                    "SYN_Thrusters.bnk",
                    vec![sound(690_001, true, "SYN_Thrusters.bnk")],
                ),
            ],
            1,
        )
    } else if let Some(n) = cooler {
        let me = format!("SYN_cooler_{n:03}");
        (
            component_guid(n),
            me.clone(),
            vec![trig(
                "Play_SYN_Cooler_Hum".into(),
                me,
                "SYN_Items.bnk",
                vec![sound(680_000 + n, true, "SYN_Items.bnk")],
            )],
            0,
        )
    } else {
        let ix = &super::mock_dev::core().index;
        let i = ix.find(what)?;
        let e = ix.entry(i)?;
        (e.guid.clone(), e.name.clone(), vec![], 0)
    };
    let sounds = triggers.iter().map(|t| t.sounds.len() as u32).sum();
    let playable = triggers
        .iter()
        .flat_map(|t| &t.sounds)
        .filter(|s| s.playable)
        .count() as u32;
    Some(RecordSounds {
        record,
        name,
        triggers,
        sounds,
        playable,
        loadout,
    })
}

fn norm(p: &str) -> String {
    starorg_data::assets::norm_path(p)
}

/// One folder's children, folders first (the same order as the real worker).
pub fn list(dir: &str, offset: u32, limit: u32) -> Option<DirPage> {
    let dir = norm(dir);
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}\\")
    };
    let mut rows: Vec<FileEntry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (p, size) in files() {
        let Some(rest) = p.strip_prefix(&prefix) else {
            continue;
        };
        match rest.split_once('\\') {
            Some((sub, _)) => {
                if seen.insert(sub.to_string()) {
                    rows.push(FileEntry {
                        name: sub.into(),
                        path: format!("{prefix}{sub}"),
                        dir: true,
                        size: 0,
                        packed: 0,
                    });
                }
            }
            None => rows.push(FileEntry {
                name: rest.into(),
                path: p.clone(),
                dir: false,
                size,
                packed: size * 3 / 4,
            }),
        }
    }
    if rows.is_empty() && !dir.is_empty() {
        return None;
    }
    rows.sort_by(|x, y| {
        y.dir.cmp(&x.dir).then_with(|| {
            x.name
                .to_ascii_lowercase()
                .cmp(&y.name.to_ascii_lowercase())
        })
    });
    let total = rows.len() as u32;
    Some(DirPage {
        parent: if dir.is_empty() {
            None
        } else {
            Some(dir.rsplit_once('\\').map(|x| x.0).unwrap_or("").to_string())
        },
        dir,
        entries: rows
            .into_iter()
            .skip(offset as usize)
            .take(limit.min(500) as usize)
            .collect(),
        offset,
        total,
    })
}

pub fn search(query: &str, limit: u32) -> FileSearch {
    let words: Vec<String> = query
        .replace('/', "\\")
        .split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect();
    let hits: Vec<FileEntry> = if words.is_empty() {
        Vec::new()
    } else {
        files()
            .into_iter()
            .filter(|(p, _)| {
                let l = p.to_ascii_lowercase();
                words.iter().all(|w| l.contains(w.as_str()))
            })
            .map(|(p, size)| FileEntry {
                name: p.rsplit('\\').next().unwrap_or(&p).to_string(),
                path: p.clone(),
                dir: false,
                size,
                packed: size * 3 / 4,
            })
            .collect()
    };
    FileSearch {
        query: query.into(),
        total: hits.len() as u32,
        hits: hits.into_iter().take(limit.min(500) as usize).collect(),
    }
}

/// `len` bytes (at most [`MODEL_CHUNK`]) of `glb` from `offset`; an offset past the end is
/// refused, one at the end is empty.
pub fn model_chunk(glb: &[u8], offset: u64, len: u32) -> CmdResult<Vec<u8>> {
    let size = glb.len() as u64;
    if offset > size {
        return Err(AppError::new(
            ErrorCode::Invalid,
            format!("offset {offset} is past the end ({size} bytes)"),
        ));
    }
    let end = (offset + u64::from(len.min(MODEL_CHUNK))).min(size);
    Ok(glb[offset as usize..end as usize].to_vec())
}

/// What the TS mock needs (`mock-assets.gen.json`).
pub fn gen_file() -> Value {
    let ships: Vec<Value> = (0..SHIPS)
        .map(ship_guid)
        .chain((0..COMPONENTS).filter(|n| n % 4 == 0).map(component_guid))
        .filter_map(|g| record_sounds(&g).map(|s| json!([g, s])))
        .collect();
    json!({
        "files": files(),
        "png": PNG_B64,
        "ogg": OGG_B64,
        "sounds": ships,
        "exports": EXPORTS,
        "cacheDefault": CACHE_DEFAULT,
        "cacheOther": CACHE_OTHER,
        "cacheMax": CACHE_MAX,
        "glb": {
            "ship": (0..=starorg_data::assets::model::MAX_LOD).map(|l| starorg_core::assetcache::base64(&model_glb(true, l))).collect::<Vec<_>>(),
            "item": (0..=starorg_data::assets::model::MAX_LOD).map(|l| starorg_core::assetcache::base64(&model_glb(false, l))).collect::<Vec<_>>(),
        },
    })
}

impl MockBackend {
    fn asset_on(&self, r: &ChannelRef) -> CmdResult<()> {
        if !self.dev_mode_on() {
            return Err(disabled());
        }
        if !self.has_channel(r) {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("No channel {} in install {}", r.channel, r.install),
            ));
        }
        Ok(())
    }

    /// Requests start the asset worker when it is not open, as the real backend does.
    fn assets_open(&self, r: &ChannelRef) -> CmdResult<()> {
        self.asset_on(r)?;
        self.asset_state().open = Some((r.install.clone(), r.channel.clone()));
        Ok(())
    }

    fn cache_add(&self, key: String, bytes: u64) {
        let mut s = self.asset_state();
        if !s.cached.iter().any(|c| c.0 == key) {
            s.cached.push((key, bytes));
        }
    }

    fn cache_has(&self, key: &str) -> bool {
        self.asset_state().cached.iter().any(|c| c.0 == key)
    }

    fn cache_size(&self, key: &str) -> Option<u64> {
        self.asset_state()
            .cached
            .iter()
            .find(|c| c.0 == key)
            .map(|c| c.1)
    }

    /// The LODs a model view of this record read (empty before one).
    fn known_lods(&self, what: &str) -> Vec<u32> {
        self.asset_state()
            .lods
            .iter()
            .find(|(g, _)| g == what)
            .map(|(_, l)| l.clone())
            .unwrap_or_default()
    }

    /// (limit, free) for the memory guard: the mock's limit and machine.
    fn mock_memory(&self) -> (Option<u64>, Option<u64>) {
        let gb = super::Backend::memory_status(self)
            .map(|m| m.limit_gb)
            .unwrap_or(5.0);
        (
            Some(starorg_core::memlimit::gb_bytes(gb)),
            super::mock::MOCK_RAM.available,
        )
    }

    /// A record with a model: its name and whether it is a ship (else an item).
    fn model_record(&self, what: &str) -> CmdResult<(String, bool)> {
        let ix = &super::mock_dev::core().index;
        let i = ix
            .find(what)
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("no record {what:?}")))?;
        let e = ix.entry(i).expect("found");
        match e.struct_type.as_str() {
            "SynShip" => Ok((e.name.clone(), true)),
            "SynComponent" => Ok((e.name.clone(), false)),
            _ => Err(AppError::new(
                ErrorCode::Invalid,
                format!(
                    "{} is not an entity (EntityClassDefinition); only entities have models",
                    e.name
                ),
            )),
        }
    }

    /// A mock ship's paints: two liveries (the first fitted), none for an item.
    fn mock_paints(&self, what: &str) -> CmdResult<ModelPaints> {
        let (name, ship) = self.model_record(what)?;
        let short = name.rsplit('.').next().unwrap_or(&name).to_string();
        let paints = if ship {
            [
                ("Pirate", "Pirate Livery"),
                ("BlackGold", "Black and Gold Livery"),
            ]
            .into_iter()
            .map(|(t, n)| ModelPaint {
                tag: format!("{short}{t}"),
                name: Some(format!("{short} {n}")),
                items: vec![format!("Paint_{short}_{t}")],
            })
            .collect()
        } else {
            Vec::new()
        };
        let equipped = ship.then(|| format!("{short}Pirate"));
        Ok(ModelPaints {
            what: what.to_string(),
            paints,
            equipped,
        })
    }

    fn cache_view(&self) -> AssetCacheStatus {
        let s = self.asset_state();
        AssetCacheStatus {
            path: if s.custom_dir {
                CACHE_OTHER
            } else {
                CACHE_DEFAULT
            }
            .into(),
            default_path: CACHE_DEFAULT.into(),
            is_default: !s.custom_dir,
            bytes: s.cached.iter().map(|c| c.1).sum(),
            files: s.cached.len() as u64,
            max_bytes: CACHE_MAX,
            other_drive: s.custom_dir,
            previous: s
                .previous
                .clone()
                .map(|(path, bytes, files)| AssetCachePrevious { path, bytes, files }),
            problem: None,
        }
    }

    fn switch_cache(&self, custom: bool) -> AssetCacheStatus {
        {
            let mut s = self.asset_state();
            if s.custom_dir != custom {
                let old = if s.custom_dir {
                    CACHE_OTHER
                } else {
                    CACHE_DEFAULT
                };
                let bytes = s.cached.iter().map(|c| c.1).sum();
                let n = s.cached.len() as u64;
                s.previous = (n > 0).then(|| (old.to_string(), bytes, n));
                s.cached.clear();
                s.custom_dir = custom;
            }
        }
        self.cache_view()
    }

    fn job_done(&self, channel: &str, label: &str, progress: Progress<'_>) {
        let job = self.next_job_id();
        for (step, phase) in [(1, JobPhase::Running), (2, JobPhase::Done)] {
            progress(JobProgress {
                job: job.clone(),
                kind: JobKind::Dev,
                channel: Some(channel.into()),
                phase,
                step,
                steps: 2,
                label: label.into(),
                cancellable: true,
            });
        }
    }
}

impl AssetBackend for MockBackend {
    fn asset_status(&self, req: ChannelRef) -> CmdResult<AssetStatus> {
        self.asset_on(&req)?;
        let s = self.asset_state();
        let open = s.open == Some((req.install.clone(), req.channel.clone()));
        Ok(AssetStatus {
            state: if open {
                WorkerState::Ready
            } else {
                WorkerState::Stopped
            },
            channel: Some(req.channel.clone()),
            entries: if open { files().len() as u32 } else { 0 },
            datacore: open && s.datacore,
            audio: open && s.audio,
            memory: open.then_some(910 * 1024 * 1024),
            error: None,
        })
    }

    fn asset_close(&self, req: ChannelRef) -> CmdResult<AssetStatus> {
        self.asset_on(&req)?;
        {
            let mut s = self.asset_state();
            s.open = None;
            s.datacore = false;
            s.audio = false;
        }
        self.asset_status(req)
    }

    fn asset_list(&self, req: AssetListRequest) -> CmdResult<DirPage> {
        self.assets_open(&req.target)?;
        list(&req.dir, req.offset, req.limit).ok_or_else(|| {
            AppError::new(ErrorCode::NotFound, format!("no folder {}", norm(&req.dir)))
        })
    }

    fn asset_search(&self, req: AssetSearchRequest) -> CmdResult<FileSearch> {
        self.assets_open(&req.target)?;
        Ok(search(&req.query, req.limit))
    }

    fn asset_preview(&self, req: AssetPathRequest) -> CmdResult<AssetPreview> {
        self.asset_on(&req.target)?;
        let path = norm(&req.path);
        let size = files()
            .into_iter()
            .find(|f| f.0.eq_ignore_ascii_case(&path))
            .map(|f| f.1)
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("no file {path}")))?;
        let ext = starorg_data::assets::files::ext(&path);
        let key = format!("preview:{}", path.to_ascii_lowercase());
        let mut p = AssetPreview {
            path: path.clone(),
            size,
            kind: PreviewKind::None,
            format: ext.clone(),
            text: None,
            truncated: false,
            width: None,
            height: None,
            image: None,
            note: None,
            cached: false,
        };
        if ext == "dds" {
            p.kind = PreviewKind::Image;
            p.image = Some(format!("data:image/png;base64,{PNG_B64}"));
            if self.cache_has(&key) {
                p.cached = true;
                p.size = 0;
            } else {
                self.assets_open(&req.target)?;
                p.width = Some(1024);
                p.height = Some(1024);
                p.note = Some("2048 x 2048, shown at 1024 x 1024".into());
                self.cache_add(key, 151);
            }
            return Ok(p);
        }
        self.assets_open(&req.target)?;
        match ext.as_str() {
            "xml" | "mtl" | "txt" => {
                p.kind = PreviewKind::Text;
                p.format = if ext == "mtl" { "cryxml".into() } else { ext };
                p.text = Some(if p.format == "txt" {
                    "plain text".into()
                } else {
                    xml_text(&path)
                });
            }
            "wem" => p.kind = PreviewKind::Audio,
            _ if starorg_data::assets::files::is_dds_sibling(&path) => {
                p.note = Some("A mip level of a texture; open the .dds file".into())
            }
            _ => p.note = Some("No preview for this file type; extract it".into()),
        }
        Ok(p)
    }

    fn asset_extract(
        &self,
        req: AssetExtractRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        self.assets_open(&req.target)?;
        if req.paths.is_empty() {
            return Err(AppError::new(ErrorCode::Invalid, "nothing selected"));
        }
        let all = files();
        let mut picked = Vec::new();
        for p in &req.paths {
            let p = norm(p);
            let pre = format!("{}\\", p.to_ascii_lowercase());
            let hits: Vec<&(String, u64)> = all
                .iter()
                .filter(|f| {
                    f.0.eq_ignore_ascii_case(&p) || f.0.to_ascii_lowercase().starts_with(&pre)
                })
                .collect();
            if hits.is_empty() {
                return Err(AppError::new(
                    ErrorCode::NotFound,
                    format!("no file or folder {p}"),
                ));
            }
            picked.extend(hits);
        }
        picked.sort();
        picked.dedup();
        let merged = if req.convert {
            picked
                .iter()
                .filter(|f| starorg_data::assets::files::is_dds_sibling(&f.0))
                .count() as u32
        } else {
            0
        };
        let files_n = picked.len() as u32 - merged;
        let bytes = picked
            .iter()
            .filter(|f| !(req.convert && starorg_data::assets::files::is_dds_sibling(&f.0)))
            .map(|f| f.1)
            .sum();
        self.job_done(&req.target.channel, "Extracting game files", progress);
        Ok(AssetExportResult {
            cancelled: false,
            path: Some(EXPORTS.into()),
            files: files_n,
            bytes,
            cached: false,
            failed: Vec::new(),
            note: (merged > 0).then(|| {
                format!("{files_n} converted, {merged} texture mip files merged into their PNG")
            }),
        })
    }

    fn asset_sounds(&self, req: AssetRecordRequest) -> CmdResult<RecordSounds> {
        self.assets_open(&req.target)?;
        {
            let mut s = self.asset_state();
            s.datacore = true;
            s.audio = true;
        }
        record_sounds(&req.what)
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("no record {:?}", req.what)))
    }

    fn asset_sound_search(&self, req: AssetSoundSearchRequest) -> CmdResult<TriggerSearch> {
        self.assets_open(&req.target)?;
        self.asset_state().audio = true;
        Ok(search_triggers(&req.query, req.limit as usize))
    }

    fn asset_trigger(&self, req: AssetTriggerRequest) -> CmdResult<TriggerInfo> {
        self.assets_open(&req.target)?;
        self.asset_state().audio = true;
        all_triggers()
            .into_iter()
            .find(|t| t.name == req.name)
            .map(|mut t| {
                t.property = String::new();
                t.from = String::new();
                t
            })
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("no sound event {:?}", req.name),
                )
            })
    }

    fn asset_sound(&self, req: AssetSoundRequest) -> CmdResult<AssetAudio> {
        self.asset_on(&req.target)?;
        let key = req.sound.key();
        if req.sound.source == SourceKind::Unavailable {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("media {} has no data in the game files", req.sound.media_id),
            ));
        }
        let cached = self.cache_has(&format!("audio:{key}"));
        if !cached {
            self.assets_open(&req.target)?;
            self.cache_add(format!("audio:{key}"), 2_824);
        }
        Ok(AssetAudio {
            key,
            mime: "audio/ogg".into(),
            data: OGG_B64.into(),
            bytes: 2_824,
            duration: if cached { None } else { Some(0.25) },
            cached,
            note: None,
            format: "Vorbis (.ogg)".into(),
            hint: None,
        })
    }

    fn asset_export_sounds(
        &self,
        req: AssetExportSoundsRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        self.asset_on(&req.target)?;
        let picks = if req.sounds.is_empty() {
            let s = record_sounds(&req.what).ok_or_else(|| {
                AppError::new(ErrorCode::NotFound, format!("no record {:?}", req.what))
            })?;
            playable_picks(&s)
        } else {
            req.sounds.clone()
        };
        if picks.is_empty() {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "this record has no playable sounds",
            ));
        }
        let cached = !req.wem
            && picks
                .iter()
                .all(|p| self.cache_has(&format!("audio:{}", p.sound.key())));
        if !cached {
            self.assets_open(&req.target)?;
            self.job_done(&req.target.channel, "Exporting sounds", progress);
            for p in &picks {
                self.cache_add(format!("audio:{}", p.sound.key()), 2_824);
            }
        }
        let n = picks.len() as u32 * if req.wem { 2 } else { 1 };
        Ok(AssetExportResult {
            cancelled: false,
            path: Some(EXPORTS.into()),
            files: n,
            bytes: n as u64 * 2_824,
            cached,
            failed: Vec::new(),
            note: None,
        })
    }

    fn asset_export_model(
        &self,
        req: AssetExportModelRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult> {
        self.asset_on(&req.target)?;
        let (rec_name, _) = self.model_record(&req.what)?;
        if req.preset == ModelPreset::Full
            && let Some(tag) = req.paint.as_deref().filter(|t| !t.trim().is_empty())
            && !self
                .mock_paints(&req.what)?
                .paints
                .iter()
                .any(|p| p.tag.eq_ignore_ascii_case(tag.trim()))
        {
            return Err(AppError::new(
                ErrorCode::Invalid,
                format!("{rec_name} has no paint {tag:?} (see its paints)"),
            ));
        }
        let lod = req.lod.min(starorg_data::assets::model::MAX_LOD);
        let o = starorg_data::assets::model::ModelOptions {
            preset: req.preset,
            lod: starorg_data::assets::model::effective_lod(lod, &self.known_lods(&req.what)),
            stl: req.stl,
            three_mf: req.three_mf,
            paint: req.paint.clone(),
            layers: req.layers,
        };
        let key = format!("model:{}", o.cache_key(&req.what));
        let held = self.cache_size(&key);
        let bytes = held.unwrap_or(match req.preset {
            ModelPreset::Full => 348_136_664,
            ModelPreset::PrintShell => 56_398_596,
        });
        if held.is_none() {
            self.job_done(
                &req.target.channel,
                &format!("Exporting model {rec_name}"),
                progress,
            );
            if bytes <= CACHE_MAX / 4 {
                self.cache_add(key, bytes);
            }
        }
        let name = if req.name.is_empty() {
            &rec_name
        } else {
            &req.name
        };
        Ok(AssetExportResult {
            cancelled: false,
            path: Some(format!(
                "{EXPORTS}/{}",
                starorg_data::assets::model::file_name(name, &o)
            )),
            files: 1 + req.stl as u32 + req.three_mf as u32,
            bytes: bytes
                + if req.stl { STL_BYTES } else { 0 }
                + if req.three_mf { THREE_MF_BYTES } else { 0 },
            cached: held.is_some(),
            failed: Vec::new(),
            note: None,
        })
    }

    fn asset_model_paints(&self, req: AssetRecordRequest) -> CmdResult<ModelPaints> {
        self.assets_open(&req.target)?;
        self.asset_state().datacore = true;
        self.mock_paints(&req.what)
    }

    fn asset_model_info(&self, req: AssetModelInfoRequest) -> CmdResult<AssetModelInfo> {
        use starorg_core::memlimit;
        use starorg_data::assets::model::{self, ModelOptions};
        self.asset_on(&req.target)?;
        let (rec_name, ship) = self.model_record(&req.what)?;
        let name = if req.name.is_empty() {
            &rec_name
        } else {
            &req.name
        };
        let known = self.known_lods(&req.what);
        let (limit, free) = self.mock_memory();
        let lods = (0..=model::MAX_LOD)
            .map(|lod| {
                let uses = model::effective_lod(lod, &known);
                let need = memlimit::model_estimate(false, uses, name);
                let o = ModelOptions {
                    preset: ModelPreset::PrintShell,
                    lod: uses,
                    stl: false,
                    ..Default::default()
                };
                let bytes = self.cache_size(&format!("model:{}", o.cache_key(&req.what)));
                let triangles = bytes.and_then(|_| {
                    starorg_data::assets::stl::glb_triangle_count(&model_glb(ship, uses))
                });
                AssetModelLod {
                    lod,
                    uses,
                    need,
                    fit: memory_fit(memlimit::check(need, limit, free)),
                    cached: bytes.is_some(),
                    bytes,
                    triangles,
                }
            })
            .collect();
        Ok(AssetModelInfo {
            what: req.what,
            lods,
            default_lod: model::view_lod(&known),
            available: known,
            limit,
            free,
            max_bytes: CACHE_MAX / 4,
        })
    }

    fn asset_model_view(
        &self,
        req: AssetModelViewRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetModelView> {
        use starorg_core::memlimit;
        use starorg_data::assets::model::{self, ModelOptions};
        self.asset_on(&req.target)?;
        let (rec_name, ship) = self.model_record(&req.what)?;
        let name = if req.name.is_empty() {
            &rec_name
        } else {
            &req.name
        };
        let asked = req.lod.min(model::MAX_LOD);
        let known = self.known_lods(&req.what);
        let key = |lod| {
            let o = ModelOptions {
                preset: ModelPreset::PrintShell,
                lod,
                stl: false,
                ..Default::default()
            };
            format!("model:{}", o.cache_key(&req.what))
        };
        let lod = model::effective_lod(asked, &known);
        let view = |lod: u32, available: Vec<u32>, cached: bool| {
            let glb = model_glb(ship, lod);
            AssetModelView {
                what: req.what.clone(),
                lod,
                asked,
                available,
                bytes: glb.len() as u64,
                cached,
                triangles: starorg_data::assets::stl::glb_triangle_count(&glb),
                ms: (!cached).then_some(840),
                peak_rss: (!cached).then_some(1_116_000_000),
                chunk: MODEL_CHUNK,
            }
        };
        if self.cache_size(&key(lod)).is_some() {
            return Ok(view(lod, known, true));
        }
        let need = memlimit::model_estimate(false, lod, name);
        let (limit, free) = self.mock_memory();
        match memlimit::check(need, limit, free) {
            memlimit::Fit::OverLimit { need, limit } => {
                return Err(AppError::new(
                    ErrorCode::Invalid,
                    memlimit::over_limit_message("Viewing this model", need, limit),
                ));
            }
            memlimit::Fit::LowFree { need, free } if !req.force => {
                return Err(AppError::new(
                    ErrorCode::NeedsDecision,
                    memlimit::low_free_message("Viewing this model", need, free),
                ));
            }
            _ => {}
        }
        self.job_done(
            &req.target.channel,
            &format!("Loading model {name} (LOD {asked})"),
            progress,
        );
        let lods = mock_lods(ship);
        let lod = model::effective_lod(asked, &lods);
        {
            let mut s = self.asset_state();
            s.lods.retain(|(g, _)| g != &req.what);
            s.lods.push((req.what.clone(), lods.clone()));
        }
        let v = view(lod, lods, false);
        self.cache_add(key(lod), v.bytes);
        Ok(v)
    }

    fn asset_model_chunk(&self, req: AssetModelChunkRequest) -> CmdResult<Vec<u8>> {
        self.asset_on(&req.target)?;
        let (_, ship) = self.model_record(&req.what)?;
        let o = starorg_data::assets::model::ModelOptions {
            preset: ModelPreset::PrintShell,
            lod: req.lod.min(starorg_data::assets::model::MAX_LOD),
            stl: false,
            ..Default::default()
        };
        if self
            .cache_size(&format!("model:{}", o.cache_key(&req.what)))
            .is_none()
        {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "this model is not in the asset cache: load it first",
            ));
        }
        model_chunk(&model_glb(ship, o.lod), req.offset, req.len)
    }

    fn asset_cache_status(&self) -> CmdResult<AssetCacheStatus> {
        Ok(self.cache_view())
    }

    /// The mock's "folder dialog" always picks the other-drive folder.
    fn asset_cache_pick(&self) -> CmdResult<AssetCacheStatus> {
        Ok(self.switch_cache(true))
    }

    fn asset_cache_reset(&self) -> CmdResult<AssetCacheStatus> {
        Ok(self.switch_cache(false))
    }

    fn asset_cache_clear(&self) -> CmdResult<AssetCacheStatus> {
        self.asset_state().cached.clear();
        Ok(self.cache_view())
    }

    fn asset_cache_move(&self) -> CmdResult<AssetCacheStatus> {
        let prev = self.asset_state().previous.take();
        match prev {
            Some((_, bytes, files)) => {
                let mut s = self.asset_state();
                for k in 0..files {
                    s.cached.push((format!("moved:{k}"), bytes / files.max(1)));
                }
            }
            None => {
                return Err(AppError::new(
                    ErrorCode::NotFound,
                    "no previous cache folder to move from",
                ));
            }
        }
        Ok(self.cache_view())
    }
}
