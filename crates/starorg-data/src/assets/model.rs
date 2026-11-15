//! Entity -> GLB through StarBreaker's export pipeline (`starbreaker-3d`), with two presets:
//!
//! - **Full (textures)**: materials and textures (mip 2, a quarter of full resolution, as
//!   StarBreaker's CLI defaults), interiors, lights and attachments.
//! - **Print shell (geometry only)**: no materials, no interior, no lights; the settings
//!   `sc-ship-models/scripts/export_ships.sh` uses for 3D prints (`--materials none
//!   --no-interior --no-lights`, LOD 0 by default).

use super::{
    AssetArchive, AssetError, AssetResult, find_record, peak_rss_bytes, record_short_name,
};
use serde::{Deserialize, Serialize};
use starbreaker_datacore::loadout::{EntityIndex, resolve_loadout_indexed};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelPreset {
    Full,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOptions {
    pub preset: ModelPreset,
    /// 0 = most detail.
    pub lod: u32,
    /// Also write an STL next to the GLB (geometry, node transforms baked).
    #[serde(default)]
    pub stl: bool,
}

impl ModelOptions {
    /// The cache key part: `full-lod1`.
    pub fn key(&self) -> String {
        format!("{}-lod{}", self.preset.key(), self.lod)
    }

    /// The asset cache key of this export of `what` (a record GUID or name):
    /// `<record>-print-lod3`, one file per record, preset and LOD (the STL is never cached).
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
            material_mode: MaterialMode::Textures,
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
    pub stl_path: Option<String>,
    pub stl_bytes: Option<u64>,
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
    let stl_out = o.stl.then(|| out.with_extension("stl"));
    if let Some(s) = &stl_out {
        a.check_out(s)?;
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
    let opts = export_options(&ModelOptions { lod, ..*o });
    let bytes = crate::extract::guarded("model export", || {
        let idx = EntityIndex::new(db);
        let tree = resolve_loadout_indexed(&idx, r);
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
        stl_path: None,
        stl_bytes: None,
        triangles: super::stl::glb_triangle_count(&bytes),
        ms: 0,
        peak_rss: None,
        lod,
        lods,
    };
    if let Some(s) = stl_out {
        progress(0.97, "Writing STL");
        let (n, len) = super::stl::glb_file_to_stl(&bytes, &s)?;
        done.stl_path = Some(s.display().to_string());
        done.stl_bytes = Some(len);
        done.triangles = Some(n);
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
