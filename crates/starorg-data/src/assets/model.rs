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
    pub triangles: Option<u64>,
    pub ms: u64,
    /// Peak resident memory of the exporting process (Linux), bytes.
    pub peak_rss: Option<u64>,
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
    let opts = export_options(o);
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
    let mut s = out.as_os_str().to_owned();
    s.push(".part");
    let part = std::path::PathBuf::from(s);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| AssetError::internal(e.to_string()))?;
    }
    let w = std::fs::write(&part, &bytes).and_then(|()| std::fs::rename(&part, out));
    if let Err(e) = w {
        let _ = std::fs::remove_file(&part);
        return Err(AssetError::internal(format!("{}: {e}", out.display())));
    }
    let mut done = ModelDone {
        path: out.display().to_string(),
        bytes: bytes.len() as u64,
        record: r.id.to_string(),
        name: record_short_name(db, r),
        stl_path: None,
        stl_bytes: None,
        triangles: None,
        ms: 0,
        peak_rss: None,
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
