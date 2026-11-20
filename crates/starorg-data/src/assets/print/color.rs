//! Multicolor (experimental): the triangles grouped by what they are (hull, glass, engines)
//! from their GLB node paths, one 3MF part per group with a display colour, and how the 3MF
//! places the parts. Pipeline steps 1, 11 and 14 of [`super::apply`].
//!
//! Owner: the color agent.
//!
//! What the names can tell. A Print shell GLB is exported with `MaterialMode::None`, so its
//! materials are only `material_NN` and say nothing; the node names survive (the full path from
//! the scene root, e.g. `rsi_skylark/geo_body/lower_cockpit_frame/geo_cockpit_glass_lower_ext`,
//! `AEGS_Gladius/Body/Nose/Canopy`, `.../hardpoint_thruster_main_left/.../geo_thruster_main_left`).
//! So [`classify`] works per node, not per material: a node named for glass (`glass`,
//! `canopy`, `windshield`, `window`) is glass, with its frame when the artist put both in one
//! node (the Gladius `Canopy` node is about a quarter glass and three quarters frame in the
//! Full export's materials); glass modelled inside the body node stays hull. Main and VTOL
//! thrusters (any node under a `thruster` or `engine` node, not a manoeuvring one: `mav`,
//! `retro`, `rcs`) are the engine group, whole (the nozzle glow is not a node of its own).
//! Everything else is hull.
//!
//! With Make solid, [`extract_groups`] gives one closed mesh per group: outer-surface cells keep
//! their group, glass and engine grow [`COLOR_DEPTH_MM`] into the solid (glass wins over
//! engine, engine over hull where they meet), every other cell is hull.
//!
//! Slicers ([`layout`]): one object made of one component per group, each with a colour of
//! one `basematerials` group. Bambu Studio loads it as one object with a part per group and
//! maps the `basematerials` colours to filaments (its importer reads an object's
//! `pid`/`pindex`). PrusaSlicer's current loader (master, after 2.9) makes each component a
//! part of one object; PrusaSlicer 2.9 places each component as an object of its own, in
//! place (its "load as a single object with multiple parts?" question, or select all and
//! Assemble, joins them). PrusaSlicer reads no colours, so the parts are named after the
//! groups (`hull`, `glass`, `engine`) and a filament is picked per part.

use super::voxel::{Bits, VoxelGrid};
use super::{Mesh, PrintOptions, Progress};
use crate::assets::threemf::Layout;
use crate::assets::{AssetError, AssetResult};
use rayon::prelude::*;

/// The hull group (everything not glass or engine).
pub const HULL: &str = "hull";
/// Canopies and windows.
pub const GLASS: &str = "glass";
/// Main and VTOL thrusters.
pub const ENGINE: &str = "engine";
/// The groups [`classify`] gives, in this order (lowest priority first: the 3MF lists the
/// parts in this order, so where two overlap a slicer that lets the later part win keeps the
/// glass).
pub const GROUPS: [&str; 3] = [HULL, ENGINE, GLASS];
/// How deep (millimetres) glass and engine reach into a solid from its outer surface.
pub const COLOR_DEPTH_MM: f32 = 1.2;
/// Groups with fewer cells than this become hull (specks too small to print in a colour).
pub const MIN_GROUP_CELLS: u64 = 27;

/// Which group a node path belongs to (see the module doc).
pub fn group_of(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let segs: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
    let own = segs.last().copied().unwrap_or("");
    let glassy = ["glass", "canopy", "windshield", "window"]
        .iter()
        .any(|k| own.contains(k));
    if glassy && !own.contains("frame") {
        return GLASS;
    }
    let engine_seg = |s: &str| {
        s.contains("thruster")
            || (s.contains("engine") && !s.contains("cryengine"))
            || s.contains("exhaust")
    };
    if segs.iter().any(|s| engine_seg(s)) {
        let manoeuvring = segs.iter().any(|s| {
            ["mav", "retro", "rcs", "maneuver", "manoeuvre"]
                .iter()
                .any(|k| s.contains(k))
        });
        if !manoeuvring {
            return ENGINE;
        }
    }
    HULL
}

/// Replace the mesh's groups (node paths from the GLB) with the colour groups of [`GROUPS`]
/// ([`group_of`] each node path). A mesh without groups becomes all hull.
pub fn classify(mesh: &mut Mesh) {
    let index = |g: &str| GROUPS.iter().position(|x| *x == g).unwrap_or(0) as u32;
    let map: Vec<u32> = mesh.labels.iter().map(|l| index(group_of(l))).collect();
    let groups = match &mesh.groups {
        Some(g) => g
            .iter()
            .map(|&i| map.get(i as usize).copied().unwrap_or(0))
            .collect(),
        None => vec![0; mesh.triangles.len()],
    };
    mesh.groups = Some(groups);
    mesh.labels = GROUPS.iter().map(|s| s.to_string()).collect();
}

/// The mesh split by group, `(label, mesh)` per non-empty group, in label order. Without
/// groups, the whole mesh as one unlabelled part.
pub fn by_group(mesh: &Mesh) -> Vec<(String, Mesh)> {
    let Some(groups) = &mesh.groups else {
        return vec![(String::new(), mesh.clone())];
    };
    let n = mesh.labels.len().max(1);
    let mut tris: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (t, &g) in groups.iter().enumerate() {
        tris[(g as usize).min(n - 1)].push(t);
    }
    tris.into_iter()
        .enumerate()
        .filter(|(_, t)| !t.is_empty())
        .map(|(g, t)| (mesh.label(g as u32).to_string(), mesh.subset(t)))
        .collect()
}

/// The display colour (RGBA) of a group label: light grey hull, blue glass, orange engine.
pub fn color_of(label: &str) -> Option<[u8; 4]> {
    match label {
        HULL => Some([0xB8, 0xBC, 0xC2, 0xFF]),
        GLASS => Some([0x3A, 0x6E, 0xA5, 0xFF]),
        ENGINE => Some([0xFF, 0x7A, 0x1A, 0xFF]),
        _ => None,
    }
}

/// How the 3MF places the parts: one assembly (one object with a part per colour) for
/// multicolor without split, else one build item per part (split pieces print apart).
pub fn layout(o: &PrintOptions) -> Layout {
    if o.multicolor && !o.split {
        Layout::Assembly
    } else {
        Layout::Separate
    }
}

/// How strongly a group holds a cell where groups meet (glass > engine > the rest).
fn priority(label: &str) -> u8 {
    match label {
        GLASS => 2,
        ENGINE => 1,
        _ => 0,
    }
}

/// The 6 neighbours of a cell (wrapping below 0, which [`VoxelGrid::get`] reads as empty).
fn near([x, y, z]: [u32; 3]) -> [[u32; 3]; 6] {
    [
        [x.wrapping_sub(1), y, z],
        [x + 1, y, z],
        [x, y.wrapping_sub(1), z],
        [x, y + 1, z],
        [x, y, z.wrapping_sub(1)],
        [x, y, z + 1],
    ]
}

/// Relabel a labelled solid for printing in colours: outer-surface cells keep their group,
/// glass and engine grow `depth` (mesh units) into the solid, everything else is hull (the
/// [`HULL`] label, else the first label without priority). Groups under [`MIN_GROUP_CELLS`]
/// become hull. Returns the cell value of the hull.
pub fn paint(grid: &mut VoxelGrid, depth: f32, progress: Progress<'_>) -> AssetResult<u8> {
    let tick = |f: f32| {
        if progress(f, "Colouring the solid") {
            Ok(())
        } else {
            Err(AssetError::cancelled())
        }
    };
    tick(0.0)?;
    let pr: Vec<u8> = std::iter::once(0)
        .chain(grid.labels.iter().map(|l| priority(l)))
        .collect();
    let prio = |v: u8| pr.get(v as usize).copied().unwrap_or(0);
    let hull = grid
        .labels
        .iter()
        .position(|l| l == HULL)
        .or_else(|| grid.labels.iter().position(|l| priority(l) == 0))
        .map_or(1, |i| (i + 1).min(255) as u8);
    let dims = grid.dims;
    // The outside: the empty cells connected to the grid's corner (voxelize pads the grid).
    let empty = grid.bits(&|c| c == 0);
    let mut outside = Bits::new(dims);
    if empty.get(0, 0, 0) {
        empty.flood(&mut outside, [0, 0, 0], |_, _, _, _| {});
    }
    tick(0.2)?;
    let (d0, d1) = (dims[0] as usize, dims[1] as usize);
    let plane = d0 * d1;
    let at = |i: usize| [(i % d0) as u32, ((i / d0) % d1) as u32, (i / plane) as u32];
    // Outer-surface cells (a face on the outside) keep their group; every other solid cell is
    // hull for now.
    let mut surface = outside.dilate(1.0);
    surface.and_not(&empty);
    drop(outside);
    drop(empty);
    let mut seeds_lo = [u32::MAX; 3];
    let mut seeds_hi = [0u32; 3];
    {
        let out = grid.cells_mut();
        for (i, c) in out.iter_mut().enumerate() {
            if *c == 0 {
                continue;
            }
            let p = at(i);
            if !surface.get(p[0], p[1], p[2]) {
                *c = hull;
            } else if prio(*c) > 0 {
                for k in 0..3 {
                    seeds_lo[k] = seeds_lo[k].min(p[k]);
                    seeds_hi[k] = seeds_hi[k].max(p[k] + 1);
                }
            } else {
                *c = hull;
            }
        }
    }
    tick(0.4)?;
    // Grow glass and engine inwards a layer at a time (the strongest neighbour wins).
    let layers = (depth / grid.voxel).ceil().max(1.0) as u32;
    if seeds_hi[0] > 0 {
        let lo = seeds_lo.map(|v| v.saturating_sub(layers));
        let hi = [0, 1, 2].map(|k| (seeds_hi[k] + layers).min(dims[k]));
        for layer in 0..layers {
            let g = &*grid;
            let set: Vec<(usize, u8)> = (lo[2]..hi[2])
                .into_par_iter()
                .flat_map_iter(|z| {
                    let mut v = Vec::new();
                    for y in lo[1]..hi[1] {
                        for x in lo[0]..hi[0] {
                            let i = g.index(x, y, z);
                            if surface.get(x, y, z) || g.cells()[i] != hull {
                                continue;
                            }
                            let best = near([x, y, z])
                                .iter()
                                .map(|&[a, b, c]| g.get(a, b, c))
                                .filter(|&n| prio(n) > 0)
                                .max_by_key(|&n| prio(n));
                            if let Some(b) = best {
                                v.push((i, b));
                            }
                        }
                    }
                    v
                })
                .collect();
            let out = grid.cells_mut();
            for (i, b) in set {
                out[i] = b;
            }
            tick(0.4 + 0.5 * (layer + 1) as f32 / layers as f32)?;
        }
    }
    // Specks become hull.
    let mut count = [0u64; 256];
    for &c in grid.cells() {
        count[c as usize] += 1;
    }
    for c in grid.cells_mut() {
        if *c != 0 && *c != hull && count[*c as usize] < MIN_GROUP_CELLS {
            *c = hull;
        }
    }
    tick(1.0)?;
    Ok(hull)
}

/// With Make solid: one closed mesh per group of a labelled solid ([`paint`] with `depth` in
/// mesh units, then [`super::voxel::extract_where`] per group on the group's own crop), in the
/// order of increasing priority (hull, engine, glass). Neighbouring groups' meshes touch (and
/// may overlap by up to a voxel where extracting closes a corner). An unlabelled grid gives one
/// unlabelled mesh.
pub fn extract_groups(
    grid: &VoxelGrid,
    depth: f32,
    progress: Progress<'_>,
) -> AssetResult<Vec<(String, Mesh)>> {
    if grid.labels.is_empty() {
        return Ok(vec![(
            String::new(),
            super::voxel::extract(grid, progress)?,
        )]);
    }
    let mut g = grid.clone();
    paint(&mut g, depth, &|f, s| progress(0.2 * f, s))?;
    // Each group's cells and bounds.
    let dims = g.dims;
    let (d0, d1) = (dims[0] as usize, dims[1] as usize);
    let mut lo = [[u32::MAX; 3]; 256];
    let mut hi = [[0u32; 3]; 256];
    for (i, &c) in g.cells().iter().enumerate() {
        if c == 0 {
            continue;
        }
        let p = [
            (i % d0) as u32,
            ((i / d0) % d1) as u32,
            (i / (d0 * d1)) as u32,
        ];
        let (l, h) = (&mut lo[c as usize], &mut hi[c as usize]);
        for k in 0..3 {
            l[k] = l[k].min(p[k]);
            h[k] = h[k].max(p[k] + 1);
        }
    }
    let mut values: Vec<u8> = (1..=255u8).filter(|&v| hi[v as usize][0] > 0).collect();
    values.sort_by_key(|&v| {
        let label = g.labels.get(v as usize - 1).map_or("", String::as_str);
        (priority(label), v)
    });
    let n = values.len().max(1) as f32;
    let mut out = Vec::new();
    for (k, v) in values.into_iter().enumerate() {
        let (a, b) = (0.2 + 0.8 * k as f32 / n, 0.2 + 0.8 * (k + 1) as f32 / n);
        let (l, h) = (lo[v as usize], hi[v as usize]);
        let cells: u64 = (0..3).map(|k| (h[k] - l[k]) as u64).product();
        let sub = |f: f32, s: &str| progress(a + (b - a) * f.clamp(0.0, 1.0), s);
        // A crop only pays when it is much smaller than the grid (the hull rarely is).
        let mesh = if cells * 2 < g.len() as u64 {
            super::voxel::extract_where(&g.crop(l, h)?, &move |c| c == v, &sub)?
        } else {
            super::voxel::extract_where(&g, &move |c| c == v, &sub)?
        };
        let label = g.labels.get(v as usize - 1).cloned().unwrap_or_default();
        out.push((label, mesh));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "color_tests.rs"]
mod tests;
