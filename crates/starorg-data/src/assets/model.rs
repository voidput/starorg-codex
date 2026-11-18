//! Entity -> GLB through StarBreaker's export pipeline (`starbreaker-3d`), with two presets:
//!
//! - **Full (textures)**: materials and textures (mip 2, a quarter of full resolution, as
//!   StarBreaker's CLI defaults), interiors, lights and attachments.
//! - **Print shell (geometry only)**: no materials, no interior, no lights; the settings
//!   `sc-ship-models/scripts/export_ships.sh` uses for 3D prints (`--materials none
//!   --no-interior --no-lights`, LOD 0 by default).
//!
//! A Full export can wear another paint ([`ModelOptions::paint`], [`list_paints`]) and use
//! StarBreaker's experimental layer materials ([`ModelOptions::layers`]). A paint is an item on
//! the ship's `hardpoint_paint` port whose `AttachDef.Tags` name one of the ship's SubGeometry
//! entries (`Paint_Gladius @GladiusPirate`); that entry gives the material set and the tint
//! palette. The export swaps the paint item in the loadout it hands StarBreaker, as fitting
//! another paint in game does.

use super::{
    AssetArchive, AssetError, AssetResult, find_record, peak_rss_bytes, record_short_name,
};
use serde::{Deserialize, Serialize};
use starbreaker_datacore::database::Database;
use starbreaker_datacore::loadout::{
    EntityIndex, LoadoutNode, LoadoutTree, resolve_loadout_indexed,
};
use starbreaker_datacore::types::Record;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelPreset {
    Full,
    #[default]
    PrintShell,
}

impl ModelPreset {
    pub fn key(self) -> &'static str {
        match self {
            ModelPreset::Full => "full",
            ModelPreset::PrintShell => "print",
        }
    }

    /// The LOD the UI starts with: 1 for Full (StarBreaker's default), 0 for a print.
    pub fn default_lod(self) -> u32 {
        match self {
            ModelPreset::Full => 1,
            ModelPreset::PrintShell => 0,
        }
    }
}

/// The peak-memory estimate of one export of `name` (`starorg_core::memlimit::model_estimate`,
/// from LIVE 4.10, addendum 7.16): a Print shell about 1.4 GB (an Idris 1.75 GB), Full about
/// 6.4-6.9 GB (a Javelin), about 11 GB for capital ships (an Idris 10.9 GB).
pub fn memory_need(o: &ModelOptions, name: &str) -> u64 {
    starorg_core::memlimit::model_estimate(o.preset == ModelPreset::Full, o.lod, name)
}

/// The warning the CLI prints and the app shows before a Full export.
pub const FULL_MEMORY_NOTE: &str = "Full exports need a lot of memory: about 6.5 GB for an Aegis Javelin and 11 GB for an Idris (Print shell: under 2 GB).";

/// Highest LOD offered (ships carry 0-3 or 0-4).
pub const MAX_LOD: u32 = 4;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelOptions {
    pub preset: ModelPreset,
    /// 0 = most detail.
    pub lod: u32,
    /// Also write an STL next to the GLB (geometry, node transforms baked).
    #[serde(default)]
    pub stl: bool,
    /// Also write a 3MF next to the GLB (the STL's triangles, shared vertices, in metres).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub three_mf: bool,
    /// Full only: the paint to wear, by the ship's SubGeometry tag ([`ModelPaint::tag`]);
    /// `None` = the paint the default loadout fits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint: Option<String>,
    /// Full only: StarBreaker's layer materials too (`MaterialMode::All`, experimental: the
    /// layers' textures are written, how they blend is the viewer's guess).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub layers: bool,
    /// Print shell only: the experimental 3D print processing of the STL/3MF
    /// ([`super::print::PrintOptions`]); only `no_weapons` changes the GLB.
    #[serde(
        default,
        skip_serializing_if = "super::print::PrintOptions::is_default"
    )]
    pub print: super::print::PrintOptions,
}

impl ModelOptions {
    /// The paint a Full export wears (`None` for a Print shell, which has no materials).
    pub fn paint(&self) -> Option<&str> {
        (self.preset == ModelPreset::Full)
            .then_some(self.paint.as_deref())
            .flatten()
            .map(str::trim)
            .filter(|p| !p.is_empty())
    }

    /// Whether a Full export uses layer materials.
    pub fn layers(&self) -> bool {
        self.preset == ModelPreset::Full && self.layers
    }

    /// Whether a Print shell export leaves the weapons out (experimental).
    pub fn no_weapons(&self) -> bool {
        self.preset == ModelPreset::PrintShell && self.print.no_weapons
    }

    /// The cache key part: `full-lod1`, `full-lod1-paint-gladiuspirate-layers`,
    /// `print-lod0-noweapons` (the other print options change only the STL/3MF).
    pub fn key(&self) -> String {
        let mut k = format!("{}-lod{}", self.preset.key(), self.lod);
        if let Some(p) = self.paint() {
            k.push_str("-paint-");
            k.push_str(&starorg_core::assetcache::safe_name(
                &p.to_ascii_lowercase(),
            ));
        }
        if self.layers() {
            k.push_str("-layers");
        }
        if self.no_weapons() {
            k.push_str("-noweapons");
        }
        k
    }

    /// The asset cache key of this export of `what` (a record GUID or name):
    /// `<record>-print-lod3`, one file per record, preset and LOD (the STL and 3MF are never cached).
    pub fn cache_key(&self, what: &str) -> String {
        format!(
            "{}-{}",
            starorg_core::assetcache::safe_name(what),
            self.key()
        )
    }
}

/// The DataCore path of an entity's own geometry (`SGeometryResourceParams`), as StarBreaker's
/// exporter reads it.
const GEOMETRY_PATH: &str = "Components[SGeometryResourceParams].Geometry.Geometry.Geometry.path";

/// `Objects/Ships/x.cga` (DataCore) -> `Data\Objects\Ships\x.cga` (archive).
fn archive_path(path: &str) -> String {
    let clean = ["Data/", "data/", "Data\\", "data\\"]
        .iter()
        .find_map(|p| path.strip_prefix(p))
        .unwrap_or(path);
    format!("Data\\{}", clean.replace('/', "\\"))
}

/// The LODs the entity's own mesh has in the archive: 0 and every `<mesh>_lod<n>` (with its
/// `.cgam`/`.cgfm`/`.skinm` geometry) up to [`MAX_LOD`]. Empty when unknown (no geometry
/// component, a character definition): the requested LOD is then used as asked.
///
/// StarBreaker loads LOD 0 for a mesh without the asked LOD file, so asking a ship that has
/// LODs 0-3 for LOD 4 would give its hull at full detail; [`effective_lod`] avoids that.
pub fn root_lods(
    db: &starbreaker_datacore::Database<'_>,
    p4k: &starbreaker_p4k::MappedP4k,
    r: &starbreaker_datacore::types::Record,
) -> Vec<u32> {
    let Ok(q) = db.compile_path::<String>(r.struct_id(), GEOMETRY_PATH) else {
        return Vec::new();
    };
    let Some(geom) = db.query_single::<String>(&q, r).ok().flatten() else {
        return Vec::new();
    };
    lods_in(&geom, |p| p4k.entry_case_insensitive(p).is_some())
}

/// [`root_lods`] for a DataCore geometry path, with `exists` telling archive entries.
pub fn lods_in(geom: &str, exists: impl Fn(&str) -> bool) -> Vec<u32> {
    if geom.is_empty() || geom.to_ascii_lowercase().ends_with(".cdf") {
        return Vec::new();
    }
    let p = archive_path(geom);
    let Some(dot) = p.rfind('.').filter(|d| !p[*d..].contains('\\')) else {
        return Vec::new();
    };
    let (base, ext) = p.split_at(dot);
    let mut out = vec![0];
    out.extend((1..=MAX_LOD).filter(|n| exists(&format!("{base}_lod{n}{ext}m"))));
    out
}

/// The asset cache entry that keeps a record's mesh LODs (`0,1,2,3`) next to its GLBs, so a
/// LOD the mesh lacks finds the cached one below it after a restart too.
fn lods_key(what: &str) -> String {
    format!("{}-lods", starorg_core::assetcache::safe_name(what))
}

/// The mesh LODs an earlier export of `what` recorded in the cache (empty when unknown).
pub fn cached_lods(
    c: &starorg_core::assetcache::AssetCache,
    channel: &str,
    build: &str,
    what: &str,
) -> Vec<u32> {
    use starorg_core::assetcache::Kind;
    let Some(p) = c.peek(channel, build, Kind::Models, &lods_key(what), "lods") else {
        return Vec::new();
    };
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let mut v: Vec<u32> = text
        .trim()
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .filter(|l| *l <= MAX_LOD)
        .collect();
    v.sort_unstable();
    v.dedup();
    if v.first() == Some(&0) { v } else { Vec::new() }
}

/// Record the mesh LODs of `what` in the cache (nothing for an empty list).
pub fn cache_lods(
    c: &starorg_core::assetcache::AssetCache,
    channel: &str,
    build: &str,
    what: &str,
    lods: &[u32],
) -> std::io::Result<()> {
    use starorg_core::assetcache::{AssetCache, Kind};
    if lods.is_empty() {
        return Ok(());
    }
    c.prepare(channel, build, Kind::Models)?;
    let key = lods_key(what);
    let text = lods
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let (part, mut f) = AssetCache::new_part(&c.path(channel, build, Kind::Models, &key, "lods"))?;
    std::io::Write::write_all(&mut f, text.as_bytes())?;
    drop(f);
    c.commit(channel, build, Kind::Models, &key, "lods", &part)
        .map(|_| ())
}

/// The LOD an export asked for `requested` uses: the most detailed one the mesh has at or
/// below the asked detail (`lods` [0, 1, 2, 3] and 4 asked -> 3). `requested` when `lods` is
/// empty (unknown).
pub fn effective_lod(requested: u32, lods: &[u32]) -> u32 {
    let r = requested.min(MAX_LOD);
    lods.iter().copied().filter(|l| *l <= r).max().unwrap_or(r)
}

/// The LOD the model viewer starts with: 3, or the least detailed one the mesh has when that
/// is below 3 (`lods` unknown: 3).
pub const VIEW_LOD: u32 = 3;

/// [`VIEW_LOD`] for a mesh with these LODs.
pub fn view_lod(lods: &[u32]) -> u32 {
    effective_lod(VIEW_LOD, lods)
}

/// StarBreaker's options for a preset (bundled GLB, all cores).
pub fn export_options(o: &ModelOptions) -> starbreaker_3d::ExportOptions {
    use starbreaker_3d::{ExportOptions, MaterialMode};
    let base = ExportOptions {
        lod_level: o.lod.min(MAX_LOD),
        ..ExportOptions::default()
    };
    match o.preset {
        ModelPreset::Full => ExportOptions {
            material_mode: if o.layers {
                MaterialMode::All
            } else {
                MaterialMode::Textures
            },
            texture_mip: 2,
            include_interior: true,
            include_lights: true,
            ..base
        },
        ModelPreset::PrintShell => ExportOptions {
            material_mode: MaterialMode::None,
            include_interior: false,
            include_lights: false,
            ..base
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDone {
    pub path: String,
    pub bytes: u64,
    pub record: String,
    pub name: String,
    /// The STL/3MF files written next to the GLB.
    #[serde(default)]
    pub meshes: Vec<super::print::MeshFile>,
    /// What the print processing says about them (parts left out, bed fit, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Triangles in the GLB (every node's triangle primitives).
    pub triangles: Option<u64>,
    pub ms: u64,
    /// Peak resident memory of the exporting process (Linux), bytes.
    pub peak_rss: Option<u64>,
    /// The LOD exported ([`effective_lod`] of the one asked).
    #[serde(default)]
    pub lod: u32,
    /// The LODs the entity's own mesh has ([`root_lods`]); empty when unknown.
    #[serde(default)]
    pub lods: Vec<u32>,
}

/// Export `what` to `out` (via `out.part`). `progress(fraction, stage)` is called from the
/// export's threads; returning false cancels.
pub fn export_model(
    a: &AssetArchive,
    what: &str,
    o: &ModelOptions,
    out: &Path,
    progress: &(dyn Fn(f32, &str) -> bool + Sync),
) -> AssetResult<ModelDone> {
    a.check_out(out)?;
    for x in super::print::mesh_outputs(out, o).into_iter().flatten() {
        a.check_out(&x)?;
    }
    if o.preset == ModelPreset::PrintShell {
        o.print.validate().map_err(AssetError::invalid)?;
    }
    let started = Instant::now();
    if !progress(0.0, "Reading the DataCore") {
        return Err(AssetError::cancelled());
    }
    let db = a.db()?;
    let r = find_record(db, what)?;
    if Some(r.struct_id()) != db.struct_id("EntityClassDefinition") {
        return Err(AssetError::invalid(format!(
            "{} is not an entity (EntityClassDefinition); only entities have models",
            db.resolve_string2(r.name_offset)
        )));
    }
    let lods = root_lods(db, a.p4k(), r);
    let lod = effective_lod(o.lod, &lods);
    let opts = export_options(&ModelOptions { lod, ..o.clone() });
    let paint = o.paint().map(|tag| choose_paint(db, r, tag)).transpose()?;
    let bytes = crate::extract::guarded("model export", || {
        let idx = EntityIndex::new(db);
        let mut tree = resolve_loadout_indexed(&idx, r);
        if let Some(p) = &paint {
            wear_paint(&mut tree, p);
        }
        if o.no_weapons() {
            strip_weapons(db, &mut tree);
        }
        let p = starbreaker_common::Progress::with_callback(progress);
        let res = starbreaker_3d::assemble_glb_with_loadout_with_progress(
            db,
            a.p4k(),
            r,
            &tree,
            &opts,
            Some(&p),
            None,
            None,
            None,
        );
        match res {
            Ok(x) => Ok((!x.glb.is_empty()).then_some(x.glb)),
            Err(e) => Err(crate::extract::ExtractError::Output(e.to_string())),
        }
    })
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("cancelled") {
            AssetError::cancelled()
        } else {
            AssetError::internal(m)
        }
    })?
    .ok_or_else(|| AssetError::internal("the export returned no GLB"))?;
    super::write_atomic(out, &bytes)?;
    let mut done = ModelDone {
        path: out.display().to_string(),
        bytes: bytes.len() as u64,
        record: r.id.to_string(),
        name: record_short_name(db, r),
        meshes: Vec::new(),
        notes: Vec::new(),
        triangles: super::stl::glb_triangle_count(&bytes),
        ms: 0,
        peak_rss: None,
        lod,
        lods,
    };
    if o.stl || o.three_mf {
        if !progress(0.9, "Writing STL/3MF") {
            return Err(AssetError::cancelled());
        }
        let m =
            super::print::write_meshes_with(&bytes, out, o, &|f, s| progress(0.9 + 0.1 * f, s))?;
        done.meshes = m.files;
        done.notes = m.notes;
    }
    drop(bytes);
    done.ms = started.elapsed().as_millis() as u64;
    done.peak_rss = peak_rss_bytes();
    Ok(done)
}

/// `AEGS_Javelin-full-lod1.glb`
pub fn file_name(name: &str, o: &ModelOptions) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "{}-{}.glb",
        if base.is_empty() { "model" } else { &base },
        o.key()
    )
}

/// One paint a ship can wear: the SubGeometry tag it selects and the paint items that do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPaint {
    /// The ship's SubGeometry tag (`GladiusPirate`): what [`ModelOptions::paint`] names.
    pub tag: String,
    /// The in-game name of its paint ("Gladius Pirate Livery"), when the stock text has one.
    pub name: Option<String>,
    /// The paint items (short record names) that select it, this ship's first.
    pub items: Vec<String>,
}

/// The paints of a ship ([`list_paints`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPaints {
    pub what: String,
    /// In the ship's SubGeometry order; only tags some paint item selects.
    pub paints: Vec<ModelPaint>,
    /// The tag of the paint the default loadout fits (`None`: no paint, the base material).
    pub equipped: Option<String>,
}

/// The SubGeometry tag a paint's `AttachDef.Tags` select: `Paint_Gladius @GladiusPirate` ->
/// `GladiusPirate` (as StarBreaker's `resolve_paint_override` reads it).
pub fn paint_tag(tags: &str) -> Option<&str> {
    tags.split_whitespace()
        .find_map(|t| t.strip_prefix('@'))
        .filter(|t| !t.is_empty())
}

/// A paint item of the DataCore.
#[derive(Debug, Clone)]
struct PaintItem {
    record: Record,
    name: String,
    tag: String,
    /// Its `AttachDef.Localization.Name` key.
    loc: Option<String>,
}

const ATTACH: &str = "Components[SAttachableComponentParams].AttachDef";

fn query_text(db: &Database<'static>, r: &Record, path: &str) -> Option<String> {
    let c = db.compile_path::<String>(r.struct_id(), path).ok()?;
    db.query_single::<String>(&c, r).ok().flatten()
}

/// Every paint item (`AttachDef.Type` `Paints`) with a SubGeometry tag. Paint items are named
/// `Paint_*`, so only records with "paint" in the name are read.
fn paint_items(db: &Database<'static>) -> Vec<PaintItem> {
    let Some(entity) = db.struct_id("EntityClassDefinition") else {
        return Vec::new();
    };
    let ty = format!("{ATTACH}.Type");
    let tags = format!("{ATTACH}.Tags");
    let loc = format!("{ATTACH}.Localization.Name");
    let mut out = Vec::new();
    for r in db.records().iter().filter(|r| r.struct_id() == entity) {
        let name = super::record_short_name(db, r);
        if !name.to_ascii_lowercase().contains("paint") {
            continue;
        }
        if !query_text(db, r, &ty).is_some_and(|t| t.eq_ignore_ascii_case("Paints")) {
            continue;
        }
        let Some(tag) = query_text(db, r, &tags).and_then(|t| paint_tag(&t).map(str::to_string))
        else {
            continue;
        };
        out.push(PaintItem {
            record: *r,
            name,
            tag,
            loc: query_text(db, r, &loc).filter(|k| !k.trim().is_empty()),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The ship's own paint tags: the `Tags` of its top-level SubGeometry entries, raw, once each,
/// in order (as `resolve_paint_override` matches them; StarBreaker's `sub_geometry_variants`
/// splits `Paint_Gladius_Blue_Turquoise` into selector words, which a paint never names).
fn ship_tags(db: &Database<'static>, r: &Record) -> Vec<String> {
    use starbreaker_datacore::query::value::Value;
    fn field<'v, 'a>(v: &'v Value<'a>, name: &str) -> Option<&'v Value<'a>> {
        match v {
            Value::Object { fields, .. } => fields.iter().find(|(k, _)| *k == name).map(|x| &x.1),
            _ => None,
        }
    }
    let Ok(c) = db.compile_path::<Value>(r.struct_id(), "Components[SGeometryResourceParams]")
    else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for comp in db.query::<Value>(&c, r).unwrap_or_default() {
        let Some(Value::Array(subs)) =
            field(&comp, "Geometry").and_then(|g| field(g, "SubGeometry"))
        else {
            continue;
        };
        for sub in subs {
            let Some(Value::String(t)) = field(sub, "Tags") else {
                continue;
            };
            let t = t.trim();
            if !t.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// `AEGS_Gladius` -> `gladius`: the part of a ship's name its paint items carry.
fn family(ship: &str) -> String {
    let s = ship.to_ascii_lowercase();
    s.split('_').nth(1).unwrap_or(&s).to_string()
}

/// The items selecting `tag`, this ship's first (two ships may share a tag name).
fn items_for<'p>(items: &'p [PaintItem], tag: &str, ship: &str) -> Vec<&'p PaintItem> {
    let fam = family(ship);
    let mut hits: Vec<&PaintItem> = items
        .iter()
        .filter(|p| p.tag.eq_ignore_ascii_case(tag))
        .collect();
    hits.sort_by_key(|p| !p.name.to_ascii_lowercase().contains(&fam));
    hits
}

/// The paint port of a loadout's root (`hardpoint_paint`), as StarBreaker finds it.
fn paint_node(tree: &mut LoadoutTree) -> Option<&mut LoadoutNode> {
    tree.root
        .children
        .iter_mut()
        .find(|c| c.item_port_name.to_lowercase().contains("paint"))
}

/// The paints a ship can wear and the one its default loadout fits.
pub fn list_paints(a: &AssetArchive, what: &str) -> AssetResult<ModelPaints> {
    let db = a.db()?;
    let r = find_record(db, what)?;
    let ship = super::record_short_name(db, r);
    let items = paint_items(db);
    let paints = ship_tags(db, r)
        .into_iter()
        .filter_map(|tag| {
            let hits = items_for(&items, &tag, &ship);
            if hits.is_empty() {
                return None;
            }
            let name = hits
                .iter()
                .find_map(|p| p.loc.as_deref().and_then(|k| a.text(k)))
                .filter(|n| !n.contains("PLACEHOLDER"));
            Some(ModelPaint {
                name,
                items: hits.iter().map(|p| p.name.clone()).collect(),
                tag,
            })
        })
        .collect();
    let idx = EntityIndex::new(db);
    let mut tree = resolve_loadout_indexed(&idx, r);
    let equipped = paint_node(&mut tree)
        .and_then(|n| query_text(db, &n.record, &format!("{ATTACH}.Tags")))
        .and_then(|t| paint_tag(&t).map(str::to_string));
    Ok(ModelPaints {
        what: what.to_string(),
        paints,
        equipped,
    })
}

/// The paint item to fit for `tag`, refused when the ship has no such paint.
fn choose_paint(db: &Database<'static>, r: &Record, tag: &str) -> AssetResult<PaintItem> {
    let ship = super::record_short_name(db, r);
    if !ship_tags(db, r).iter().any(|t| t.eq_ignore_ascii_case(tag)) {
        return Err(AssetError::invalid(format!(
            "{ship} has no paint {tag:?} (see its paints)"
        )));
    }
    let items = paint_items(db);
    items_for(&items, tag, &ship)
        .first()
        .map(|p| (*p).clone())
        .ok_or_else(|| AssetError::invalid(format!("no paint item selects {tag:?} on {ship}")))
}

/// TODO(weapons agent): leave the weapons out of a Print shell's loadout (experimental
/// `no_weapons`): remove the nodes of weapon hardpoints (guns, missile racks, turrets' guns)
/// from `tree`, keeping turrets' bodies and everything else. Returns how many nodes were
/// removed. Stub: removes nothing.
pub fn strip_weapons(db: &Database<'static>, tree: &mut LoadoutTree) -> usize {
    let _ = (db, tree);
    0
}

/// Fit `p` on the root's paint port (adding the port when the default loadout leaves it out).
fn wear_paint(tree: &mut LoadoutTree, p: &PaintItem) {
    if let Some(n) = paint_node(tree) {
        n.record = p.record;
        n.entity_name = p.name.clone();
        n.geometry_path = None;
        n.material_path = None;
        n.children.clear();
        return;
    }
    tree.root.children.push(LoadoutNode {
        record: p.record,
        entity_name: p.name.clone(),
        item_port_name: "hardpoint_paint".into(),
        helper_bone_name: None,
        no_rotation: false,
        offset_position: [0.0; 3],
        offset_rotation: [0.0; 3],
        detach_direction: [0.0; 3],
        port_flags: String::new(),
        port_tags: String::new(),
        geometry_path: None,
        material_path: None,
        children: Vec::new(),
    });
}
