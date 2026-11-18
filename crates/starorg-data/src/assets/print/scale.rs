//! Print size (experimental): the scale mode, sitting on the bed, the bed check (or shrinking
//! to the bed) and leaving out tiny parts. Pipeline steps 2-5 and 13 of [`super::apply`].
//!
//! Owner: the scale agent. [`factor`], [`scale`] and [`sit_on_bed`] are done; [`drop_tiny`] and
//! [`bed_check`] are pass-through stubs.

use super::{Aabb, Mesh, PrintOptions, ScaleMode};
use crate::assets::AssetResult;
use crate::assets::threemf::Unit;

/// How the mesh (metres) is scaled: multiply every coordinate by `k`; the result is in `unit`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub k: f32,
    pub unit: Unit,
}

/// The scale of a mesh with these bounds (metres): real size in metres without a scale mode or
/// `shrink_to_bed` (`k` 1); otherwise millimetres (`fit_mm`: the longest side that long;
/// `ratio` N: 1:N; none with `shrink_to_bed`: real size in mm, shrunk later by [`bed_check`]).
pub fn factor(bounds: &Aabb, o: &PrintOptions) -> Scale {
    let mm = |k: f32| Scale {
        k,
        unit: Unit::Millimeter,
    };
    match o.scale {
        ScaleMode::None if !o.shrink_to_bed => Scale {
            k: 1.0,
            unit: Unit::Meter,
        },
        ScaleMode::None => mm(1000.0),
        ScaleMode::FitMm(f) => {
            let l = bounds.longest();
            mm(if l > 0.0 { f / l } else { 1000.0 })
        }
        ScaleMode::Ratio(r) => mm(1000.0 / r.max(1) as f32),
    }
}

/// Multiply every coordinate by `k` (about the origin).
pub fn scale(mesh: &mut Mesh, k: f32) {
    if k != 1.0 {
        mesh.map_vertices(|p| p.map(|c| c * k));
    }
}

/// Move `meshes` together so the lowest point of all of them is at z = 0 (always applied).
pub fn sit_on_bed(meshes: &mut [&mut Mesh]) {
    let low = meshes
        .iter()
        .filter_map(|m| m.bounds())
        .map(|b| b.min[2])
        .fold(f32::INFINITY, f32::min);
    if low.is_finite() && low != 0.0 {
        for m in meshes.iter_mut() {
            m.map_vertices(|[x, y, z]| [x, y, z - low]);
        }
    }
}

/// TODO(scale agent): leave out connected parts whose largest side is under `min_size` (mesh
/// units, metres here: the pipeline converts `drop_small_mm` at print size). Returns how many
/// parts were left out. Stub: keeps everything.
pub fn drop_tiny(mesh: &mut Mesh, min_size: f32) -> usize {
    let _ = (mesh, min_size);
    0
}

/// TODO(scale agent): compare the mesh (in `unit`, sitting on the bed) with `o.bed_mm`; when
/// larger, shrink it to fit with `o.shrink_to_bed` (and say so in `notes`), else add a note
/// that it does not fit (with `o.split`, the split makes pieces that do). Stub: nothing.
pub fn bed_check(
    mesh: &mut Mesh,
    o: &PrintOptions,
    unit: Unit,
    notes: &mut Vec<String>,
) -> AssetResult<()> {
    let _ = (mesh, o, unit, notes);
    Ok(())
}
