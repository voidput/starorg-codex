//! The STL and 3MF written next to a Print shell GLB (addendum 7.16), and the experimental 3D
//! print processing applied to them (spec `docs/plans/specs/model-print.md`). The GLB itself is
//! never changed here (only [`PrintOptions::no_weapons`] changes the export, in
//! [`super::model`]).
//!
//! The pipeline ([`apply`]) runs in a fixed order, each step in its own module:
//!
//! 1. classify groups ([`color::classify`], multicolor only)
//! 2. drop tiny parts ([`scale::drop_tiny`])
//! 3. scale ([`scale::factor`], [`scale::scale`])
//! 4. sit on the bed ([`scale::sit_on_bed`], always)
//! 5. bed check / shrink to the bed ([`scale::bed_check`])
//! 6. voxel solid ([`voxel::voxelize`]), 7. thicken ([`voxel::thicken`]), 8. hollow with a
//!    drain hole ([`voxel::hollow`]), 9. pegs ([`voxel::peg`] at [`split::plan`]'s cuts)
//! 10. split ([`split::split`])
//! 11. extract per group ([`voxel::extract`], then [`color::by_group`])
//! 12. simplify ([`simplify::simplify`])
//! 13. sit on the bed again
//! 14. write ([`write_output`]: STL and/or 3MF, [`color::layout`] for the 3MF build)
//!
//! Everything but sitting on the bed is off by default and experimental.

pub mod color;
pub mod scale;
pub mod simplify;
pub mod split;
pub mod voxel;

use super::model::{ModelOptions, ModelPreset};
use super::threemf::{self, Layout, Unit};
use super::{AssetError, AssetResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `progress(fraction, stage)`; returning false cancels (the step returns
/// [`AssetError::cancelled`]).
pub type Progress<'a> = &'a (dyn Fn(f32, &str) -> bool + Sync);

/// `--fit-mm` without a value in the UI: the longest side in millimetres.
pub const DEFAULT_FIT_MM: f32 = 150.0;
/// A common bed (Bambu X1/P1, Prusa Core One-ish), millimetres.
pub const DEFAULT_BED_MM: [f32; 3] = [256.0, 256.0, 256.0];
/// Parts smaller than this (largest side, millimetres at print size) are left out.
pub const DEFAULT_DROP_SMALL_MM: f32 = 1.0;
/// The voxel sizes offered for a solid, millimetres.
pub const SOLID_RES_MM: [f32; 3] = [0.1, 0.2, 0.4];
pub const DEFAULT_SOLID_RES_MM: f32 = 0.2;
/// Thin surfaces are grown to at least this, millimetres.
pub const DEFAULT_THICKEN_MM: f32 = 0.8;
pub const DEFAULT_WALL_MM: f32 = 2.0;
pub const DEFAULT_DRAIN_MM: f32 = 3.0;
/// Peg diameters offered, millimetres.
pub const PEG_MM: [f32; 3] = [3.0, 5.0, 6.0];
pub const DEFAULT_PEG_MM: f32 = 5.0;
/// How deep a peg goes into each side of a cut, millimetres.
pub const PEG_DEPTH_MM: f32 = 8.0;
pub const DEFAULT_SIMPLIFY_TRIS: u32 = 200_000;
/// The fewest triangles `simplify_tris` may ask for.
pub const MIN_SIMPLIFY_TRIS: u32 = 1_000;
/// The largest fit and bed sizes accepted, millimetres.
pub const MAX_MM: f32 = 10_000.0;

/// How the model is scaled for printing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "kebab-case")]
pub enum ScaleMode {
    /// Real size, in metres (the 3MF says `unit="meter"`; an STL has no unit).
    #[default]
    None,
    /// The longest side becomes this many millimetres.
    FitMm(f32),
    /// 1:N of the real size (`100` = 1:100), in millimetres.
    Ratio(u32),
}

/// A hollow solid: walls this thick and a drain hole of this diameter (0 = none) at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hollow {
    pub wall_mm: f32,
    pub drain_mm: f32,
}

impl Default for Hollow {
    fn default() -> Self {
        Self {
            wall_mm: DEFAULT_WALL_MM,
            drain_mm: DEFAULT_DRAIN_MM,
        }
    }
}

/// The experimental 3D print processing of the STL/3MF (Print shell only). Every option is off
/// by default; sitting on the bed always happens. Missing fields take their defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintOptions {
    pub scale: ScaleMode,
    /// The printer's build volume, millimetres (x, y, z).
    pub bed_mm: [f32; 3],
    /// Scale down to fit the bed when larger (otherwise only a note).
    pub shrink_to_bed: bool,
    /// Leave out parts whose largest side is under this, millimetres at print size.
    pub drop_small_mm: Option<f32>,
    /// Rebuild the model as one closed solid from voxels (needs a scale or `shrink_to_bed`).
    pub solid: bool,
    /// The voxel size of the solid, one of [`SOLID_RES_MM`].
    pub solid_res_mm: f32,
    /// Grow thin surfaces to at least this, millimetres (needs `solid`).
    pub thicken_mm: Option<f32>,
    /// Hollow the solid (needs `solid`).
    pub hollow: Option<Hollow>,
    /// Peg holes of this diameter (one of [`PEG_MM`], [`PEG_DEPTH_MM`] deep) across the split
    /// cuts (needs `split`).
    pub peg_mm: Option<f32>,
    /// Cut the solid into pieces that fit the bed (needs `solid`).
    pub split: bool,
    /// One object per material group (hull, glass, ...) with a colour each, in the 3MF.
    pub multicolor: bool,
    /// Simplify to about this many triangles in all.
    pub simplify_tris: Option<u32>,
    /// Export without the ship's weapons (changes the GLB; part of the cache key).
    pub no_weapons: bool,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            scale: ScaleMode::None,
            bed_mm: DEFAULT_BED_MM,
            shrink_to_bed: false,
            drop_small_mm: None,
            solid: false,
            solid_res_mm: DEFAULT_SOLID_RES_MM,
            thicken_mm: None,
            hollow: None,
            peg_mm: None,
            split: false,
            multicolor: false,
            simplify_tris: None,
            no_weapons: false,
        }
    }
}

fn positive(v: f32) -> bool {
    v.is_finite() && v > 0.0
}

fn one_of(v: f32, set: &[f32]) -> bool {
    set.iter().any(|s| (v - s).abs() < 1e-4)
}

impl PrintOptions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Refuse values out of range and options without the one they need. The messages name
    /// the CLI flags and read for the app too.
    pub fn validate(&self) -> Result<(), String> {
        match self.scale {
            ScaleMode::None => {}
            ScaleMode::FitMm(v) if positive(v) && v <= MAX_MM => {}
            ScaleMode::FitMm(v) => {
                return Err(format!(
                    "fit size {v} mm is out of range (more than 0, at most {MAX_MM})"
                ));
            }
            ScaleMode::Ratio(r) if r >= 1 => {}
            ScaleMode::Ratio(_) => return Err("scale ratio must be 1 or more (1:N)".into()),
        }
        if !self.bed_mm.iter().all(|v| positive(*v) && *v <= MAX_MM) {
            return Err(format!(
                "bed size {:?} mm is out of range (each side more than 0, at most {MAX_MM})",
                self.bed_mm
            ));
        }
        if let Some(v) = self.drop_small_mm
            && !positive(v)
        {
            return Err(format!("drop-small size {v} mm must be more than 0"));
        }
        if !one_of(self.solid_res_mm, &SOLID_RES_MM) {
            return Err(format!(
                "solid resolution {} mm must be 0.1, 0.2 or 0.4",
                self.solid_res_mm
            ));
        }
        if let Some(v) = self.thicken_mm
            && !positive(v)
        {
            return Err(format!("thicken size {v} mm must be more than 0"));
        }
        if let Some(h) = self.hollow {
            if !positive(h.wall_mm) {
                return Err(format!("hollow wall {} mm must be more than 0", h.wall_mm));
            }
            if !(h.drain_mm.is_finite() && h.drain_mm >= 0.0) {
                return Err(format!(
                    "drain hole {} mm must be 0 (none) or more",
                    h.drain_mm
                ));
            }
        }
        if let Some(v) = self.peg_mm
            && !one_of(v, &PEG_MM)
        {
            return Err(format!("peg size {v} mm must be 3, 5 or 6"));
        }
        if let Some(n) = self.simplify_tris
            && n < MIN_SIMPLIFY_TRIS
        {
            return Err(format!(
                "simplify to {n} triangles: at least {MIN_SIMPLIFY_TRIS}"
            ));
        }
        let needs_solid = [
            (self.thicken_mm.is_some(), "--thicken"),
            (self.hollow.is_some(), "--hollow"),
            (self.peg_mm.is_some(), "--peg"),
            (self.split, "--split"),
        ];
        if !self.solid
            && let Some((_, flag)) = needs_solid.iter().find(|(on, _)| *on)
        {
            return Err(format!("{flag} needs --solid (experimental)"));
        }
        if self.peg_mm.is_some() && !self.split {
            return Err("--peg needs --split (pegs join the split pieces)".into());
        }
        if self.solid && self.scale == ScaleMode::None && !self.shrink_to_bed {
            return Err(
                "--solid needs --fit-mm, --scale-ratio or --shrink-to-bed (a real-size ship is metres long)"
                    .into(),
            );
        }
        Ok(())
    }
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn size(&self) -> [f32; 3] {
        [0, 1, 2].map(|i| self.max[i] - self.min[i])
    }

    /// The longest side.
    pub fn longest(&self) -> f32 {
        self.size().into_iter().fold(0.0, f32::max)
    }
}

/// A shared-vertex triangle mesh (Z-up, in the unit of its stage: metres from the GLB,
/// millimetres once scaled), with an optional group per triangle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// Per triangle, an index into `labels` (as long as `triangles`); `None` = one group.
    /// From the GLB these are node paths; after [`color::classify`], colour groups.
    pub groups: Option<Vec<u32>>,
    pub labels: Vec<String>,
}

impl Mesh {
    /// Share vertices by exact `f32` bits (`-0.0` counts as `0.0`). Triangles with a
    /// non-finite coordinate or two equal corners are dropped (3MF requires three distinct
    /// vertices).
    pub fn from_triangles(tris: &[[[f32; 3]; 3]]) -> Mesh {
        Self::index(tris, None, Vec::new())
    }

    /// [`Mesh::from_triangles`] keeping the node of each triangle as its group.
    pub fn from_labeled(t: &super::stl::LabeledTriangles) -> Mesh {
        Self::index(&t.triangles, Some(&t.node), t.nodes.clone())
    }

    /// The GLB's triangles ([`super::stl::glb_labeled_triangles`]), refused without any.
    pub fn from_glb(glb: &[u8]) -> AssetResult<Mesh> {
        let m = Self::from_labeled(&super::stl::glb_labeled_triangles(glb)?);
        if m.triangles.is_empty() {
            return Err(AssetError::invalid("the model has no triangles"));
        }
        Ok(m)
    }

    fn index(tris: &[[[f32; 3]; 3]], group: Option<&[u32]>, labels: Vec<String>) -> Mesh {
        let mut ids: HashMap<[u32; 3], u32> = HashMap::with_capacity(tris.len());
        let mut vertices = Vec::with_capacity(tris.len());
        let mut triangles = Vec::with_capacity(tris.len());
        let mut groups = group.map(|_| Vec::with_capacity(tris.len()));
        for (n, t) in tris.iter().enumerate() {
            if t.iter().flatten().any(|c| !c.is_finite()) {
                continue;
            }
            let mut idx = [0u32; 3];
            for (i, p) in idx.iter_mut().zip(t) {
                // +0.0 for -0.0: same point, one vertex.
                let p = p.map(|c| if c == 0.0 { 0.0 } else { c });
                *i = *ids.entry(p.map(f32::to_bits)).or_insert_with(|| {
                    vertices.push(p);
                    (vertices.len() - 1) as u32
                });
            }
            if idx[0] != idx[1] && idx[1] != idx[2] && idx[0] != idx[2] {
                triangles.push(idx);
                if let (Some(g), Some(src)) = (groups.as_mut(), group) {
                    g.push(src.get(n).copied().unwrap_or(0));
                }
            }
        }
        Mesh {
            vertices,
            triangles,
            groups,
            labels,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// The group of triangle `t` (0 without groups).
    pub fn group(&self, t: usize) -> u32 {
        self.groups
            .as_ref()
            .and_then(|g| g.get(t).copied())
            .unwrap_or(0)
    }

    /// The label of group `g` (empty when unknown).
    pub fn label(&self, g: u32) -> &str {
        self.labels.get(g as usize).map_or("", String::as_str)
    }

    /// The corners of triangle `t`.
    pub fn triangle(&self, t: usize) -> [[f32; 3]; 3] {
        self.triangles[t].map(|i| self.vertices[i as usize])
    }

    /// Every triangle as three corners (for the STL).
    pub fn soup(&self) -> Vec<[[f32; 3]; 3]> {
        (0..self.triangles.len())
            .map(|t| self.triangle(t))
            .collect()
    }

    /// The bounds of the vertices the triangles use (`None` without triangles).
    pub fn bounds(&self) -> Option<Aabb> {
        let mut b: Option<Aabb> = None;
        for t in &self.triangles {
            for &i in t {
                let p = self.vertices[i as usize];
                let x = b.get_or_insert(Aabb { min: p, max: p });
                for (k, c) in p.iter().enumerate() {
                    x.min[k] = x.min[k].min(*c);
                    x.max[k] = x.max[k].max(*c);
                }
            }
        }
        b
    }

    /// Change every vertex.
    pub fn map_vertices(&mut self, f: impl Fn([f32; 3]) -> [f32; 3]) {
        for v in &mut self.vertices {
            *v = f(*v);
        }
    }

    /// Keep the triangles `keep(t)` says (their groups too), then drop unused vertices.
    pub fn retain_triangles(&mut self, mut keep: impl FnMut(usize) -> bool) {
        let flags: Vec<bool> = (0..self.triangles.len()).map(&mut keep).collect();
        let mut i = 0;
        self.triangles.retain(|_| {
            i += 1;
            flags[i - 1]
        });
        if let Some(g) = &mut self.groups {
            let mut i = 0;
            g.retain(|_| {
                i += 1;
                flags.get(i - 1).copied().unwrap_or(false)
            });
        }
        self.compact();
    }

    /// The triangles `tris` as a mesh of their own (same labels).
    pub fn subset(&self, tris: impl IntoIterator<Item = usize>) -> Mesh {
        let mut map: HashMap<u32, u32> = HashMap::new();
        let mut m = Mesh {
            labels: self.labels.clone(),
            groups: self.groups.as_ref().map(|_| Vec::new()),
            ..Mesh::default()
        };
        for t in tris {
            let tri = self.triangles[t].map(|v| {
                *map.entry(v).or_insert_with(|| {
                    m.vertices.push(self.vertices[v as usize]);
                    (m.vertices.len() - 1) as u32
                })
            });
            m.triangles.push(tri);
            if let Some(g) = &mut m.groups {
                g.push(self.group(t));
            }
        }
        m
    }

    /// Drop vertices no triangle uses.
    pub fn compact(&mut self) {
        let mut map = vec![u32::MAX; self.vertices.len()];
        let mut verts = Vec::new();
        for t in &mut self.triangles {
            for i in t.iter_mut() {
                let m = &mut map[*i as usize];
                if *m == u32::MAX {
                    verts.push(self.vertices[*i as usize]);
                    *m = (verts.len() - 1) as u32;
                }
                *i = *m;
            }
        }
        self.vertices = verts;
    }

    /// Add `other`'s triangles (its labels merged by name).
    pub fn append(&mut self, other: &Mesh) {
        let base = self.vertices.len() as u32;
        let before = self.triangles.len();
        self.vertices.extend_from_slice(&other.vertices);
        self.triangles
            .extend(other.triangles.iter().map(|t| t.map(|i| i + base)));
        if self.groups.is_none() && other.groups.is_none() {
            return;
        }
        let mut g = self.groups.take().unwrap_or_else(|| vec![0; before]);
        if self.labels.is_empty() {
            self.labels.push(String::new());
        }
        let remap: Vec<u32> = other
            .labels
            .iter()
            .map(|l| match self.labels.iter().position(|x| x == l) {
                Some(i) => i as u32,
                None => {
                    self.labels.push(l.clone());
                    (self.labels.len() - 1) as u32
                }
            })
            .collect();
        g.extend((0..other.triangles.len()).map(|t| {
            let o = other.group(t);
            remap.get(o as usize).copied().unwrap_or(0)
        }));
        self.groups = Some(g);
    }
}

/// A cut across the solid: cells below `at` (mesh units along `axis`, 0 = x) go to one side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    pub axis: usize,
    pub at: f32,
}

/// One printable object of the output.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// `piece-1`, `hull`, `piece-2 glass`; empty for a single part.
    pub name: String,
    pub mesh: Mesh,
    /// Display colour (RGBA) for the 3MF (multicolor).
    pub color: Option<[u8; 4]>,
}

/// What [`apply`] gives: the parts to write, their unit and placement, and notes for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintOutput {
    pub parts: Vec<Part>,
    pub unit: Unit,
    pub layout: Layout,
    pub notes: Vec<String>,
}

impl PrintOutput {
    pub fn triangles(&self) -> u64 {
        self.parts
            .iter()
            .map(|p| p.mesh.triangles.len() as u64)
            .sum()
    }
}

/// `progress` for the `a..b` part of the whole.
fn sub<'a>(p: Progress<'a>, a: f32, b: f32) -> impl Fn(f32, &str) -> bool + Sync + 'a {
    move |f, s| p(a + (b - a) * f.clamp(0.0, 1.0), s)
}

fn step(p: Progress<'_>, f: f32, s: &str) -> AssetResult<()> {
    if p(f, s) {
        Ok(())
    } else {
        Err(AssetError::cancelled())
    }
}

/// `piece-1` + `hull` -> `piece-1 hull`.
fn part_name(piece: &str, group: &str) -> String {
    match (piece.is_empty(), group.is_empty()) {
        (true, _) => group.to_string(),
        (_, true) => piece.to_string(),
        _ => format!("{piece} {group}"),
    }
}

/// The parts of one mesh: per colour group with multicolor, else the whole.
fn per_group(piece: &str, mesh: Mesh, o: &PrintOptions) -> Vec<Part> {
    if o.multicolor {
        color::by_group(&mesh)
            .into_iter()
            .map(|(label, m)| Part {
                name: part_name(piece, &label),
                color: color::color_of(&label),
                mesh: m,
            })
            .collect()
    } else {
        vec![Part {
            name: piece.to_string(),
            mesh,
            color: None,
        }]
    }
}

/// Run the print pipeline on `mesh` (metres, Z-up, from the GLB). The order is fixed (see the
/// module doc); each step is a no-op unless its option is on.
pub fn apply(mut mesh: Mesh, o: &PrintOptions, progress: Progress<'_>) -> AssetResult<PrintOutput> {
    o.validate().map_err(AssetError::invalid)?;
    if mesh.is_empty() {
        return Err(AssetError::invalid("the model has no triangles"));
    }
    let mut notes = Vec::new();
    // 1. Groups: colour groups for multicolor, none otherwise.
    if o.multicolor {
        step(progress, 0.0, "Grouping parts")?;
        color::classify(&mut mesh);
    } else {
        mesh.groups = None;
        mesh.labels.clear();
    }
    // 2-3. Drop tiny parts (measured at print size), scale.
    let bounds = mesh
        .bounds()
        .ok_or_else(|| AssetError::invalid("the model has no triangles"))?;
    let s = scale::factor(&bounds, o);
    if let Some(mm) = o.drop_small_mm {
        step(progress, 0.02, "Leaving out small parts")?;
        let n = scale::drop_tiny(&mut mesh, mm / (s.k * s.unit.mm()));
        if n > 0 {
            notes.push(format!("Left out {n} parts smaller than {mm} mm."));
        }
        if mesh.is_empty() {
            return Err(AssetError::invalid(format!(
                "every part is smaller than {mm} mm"
            )));
        }
    }
    scale::scale(&mut mesh, s.k);
    let unit = s.unit;
    let per_mm = 1.0 / unit.mm();
    // 4-5. Sit on the bed, check (or shrink to) the bed.
    scale::sit_on_bed(&mut [&mut mesh]);
    scale::bed_check(&mut mesh, o, unit, &mut notes)?;
    scale::sit_on_bed(&mut [&mut mesh]);
    // 6-11. Solid, thicken, hollow, pegs, split, extract per group.
    let mut parts = Vec::new();
    if o.solid {
        step(progress, 0.05, "Building the solid")?;
        let mut grid = voxel::voxelize(&mesh, o.solid_res_mm * per_mm, &sub(progress, 0.05, 0.4))?;
        notes.extend(voxel::coarsened(&grid, o.solid_res_mm * per_mm));
        drop(mesh);
        if let Some(t) = o.thicken_mm {
            step(progress, 0.4, "Thickening thin parts")?;
            voxel::thicken(&mut grid, t * per_mm, &sub(progress, 0.4, 0.5))?;
        }
        if let Some(h) = o.hollow {
            step(progress, 0.5, "Hollowing")?;
            voxel::hollow(
                &mut grid,
                h.wall_mm * per_mm,
                h.drain_mm * per_mm,
                &mut notes,
                &sub(progress, 0.5, 0.6),
            )?;
        }
        let cuts = if o.split {
            split::plan(&grid, o.bed_mm.map(|v| v * per_mm))
        } else {
            Vec::new()
        };
        if let Some(d) = o.peg_mm {
            step(progress, 0.6, "Adding pegs")?;
            voxel::peg(
                &mut grid,
                &cuts,
                d * per_mm,
                PEG_DEPTH_MM * per_mm,
                &mut notes,
            )?;
        }
        let pieces = if o.split {
            step(progress, 0.62, "Splitting")?;
            split::split(grid, &cuts)
        } else {
            vec![split::Piece {
                name: String::new(),
                grid,
            }]
        };
        let n = pieces.len().max(1) as f32;
        for (i, piece) in pieces.into_iter().enumerate() {
            let (a, b) = (0.65 + 0.2 * i as f32 / n, 0.65 + 0.2 * (i + 1) as f32 / n);
            let m = voxel::extract(&piece.grid, &sub(progress, a, b))?;
            parts.extend(per_group(&piece.name, m, o));
        }
    } else {
        parts = per_group("", mesh, o);
    }
    parts.retain(|p| !p.mesh.is_empty());
    if parts.is_empty() {
        return Err(AssetError::invalid("the model has no triangles left"));
    }
    // 12. Simplify, the budget shared by triangles.
    if let Some(n) = o.simplify_tris {
        let total: u64 = parts.iter().map(|p| p.mesh.triangles.len() as u64).sum();
        if total > n as u64 {
            let k = parts.len().max(1) as f32;
            for (i, p) in parts.iter_mut().enumerate() {
                let own = p.mesh.triangles.len() as u64;
                let target = ((n as u64 * own) / total).max(4) as u32;
                let (a, b) = (0.85 + 0.1 * i as f32 / k, 0.85 + 0.1 * (i + 1) as f32 / k);
                simplify::simplify(&mut p.mesh, target, &sub(progress, a, b))?;
            }
        }
    }
    // 13. Sit on the bed again: each split piece on its own, else all together.
    let layout = color::layout(o);
    if o.split {
        for p in &mut parts {
            scale::sit_on_bed(&mut [&mut p.mesh]);
        }
    } else {
        let mut all: Vec<&mut Mesh> = parts.iter_mut().map(|p| &mut p.mesh).collect();
        scale::sit_on_bed(&mut all);
    }
    step(progress, 0.95, "Writing")?;
    Ok(PrintOutput {
        parts,
        unit,
        layout,
        notes,
    })
}

/// One STL or 3MF file written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshFile {
    /// `stl` or `3mf`.
    pub kind: String,
    pub path: String,
    pub bytes: u64,
    pub triangles: u64,
}

/// The files written next to a GLB, and what the print processing says about them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meshes {
    pub files: Vec<MeshFile>,
    pub notes: Vec<String>,
}

impl Meshes {
    /// Files written and their bytes.
    pub fn files_bytes(&self) -> (u32, u64) {
        files_bytes(&self.files)
    }
}

/// Count and bytes of `files`.
pub fn files_bytes(files: &[MeshFile]) -> (u32, u64) {
    (files.len() as u32, files.iter().map(|f| f.bytes).sum())
}

/// Where the STL and 3MF of the GLB `out` go (next to it), when asked. Split pieces' STLs go
/// next to these ([`part_path`]).
pub fn mesh_outputs(out: &Path, o: &ModelOptions) -> [Option<PathBuf>; 2] {
    [
        o.stl.then(|| out.with_extension("stl")),
        o.three_mf.then(|| out.with_extension("3mf")),
    ]
}

/// `ship.glb` + `piece-1 hull` + `stl` -> `ship-piece-1_hull.stl`.
pub fn part_path(out: &Path, part: &str, ext: &str) -> PathBuf {
    let safe: String = part
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".into());
    out.with_file_name(format!("{stem}-{safe}.{ext}"))
}

/// The print options a model export applies: its own for a Print shell, none for Full.
pub fn options_of(o: &ModelOptions) -> PrintOptions {
    if o.preset == ModelPreset::PrintShell {
        o.print.clone()
    } else {
        PrintOptions::default()
    }
}

/// Write `p` as the STL (`stl`) and/or 3MF (`three_mf`) of the GLB `out`. The 3MF holds every
/// part ([`threemf::write_3mf_objects`] with `p.layout`); the STL is one file, or one per part
/// ([`part_path`]) for separate parts (split pieces).
pub fn write_output(
    p: &PrintOutput,
    out: &Path,
    stl: bool,
    three_mf: bool,
) -> AssetResult<Vec<MeshFile>> {
    let mut files = Vec::new();
    let file = |kind: &str, p: &Path, (triangles, bytes): (u64, u64)| MeshFile {
        kind: kind.into(),
        path: p.display().to_string(),
        bytes,
        triangles,
    };
    if stl {
        if p.layout == Layout::Separate && p.parts.len() > 1 {
            for part in &p.parts {
                let path = part_path(out, &part.name, "stl");
                let r = super::stl::write_stl(&part.mesh.soup(), &path)?;
                files.push(file("stl", &path, r));
            }
        } else {
            let path = out.with_extension("stl");
            let soup: Vec<[[f32; 3]; 3]> = p.parts.iter().flat_map(|x| x.mesh.soup()).collect();
            let r = super::stl::write_stl(&soup, &path)?;
            files.push(file("stl", &path, r));
        }
    }
    if three_mf {
        let path = out.with_extension("3mf");
        let objects: Vec<threemf::Object> = p
            .parts
            .iter()
            .map(|x| threemf::Object {
                name: &x.name,
                mesh: &x.mesh,
                color: x.color,
            })
            .collect();
        let r = threemf::write_3mf_objects(&objects, p.unit, p.layout, &path)?;
        files.push(file("3mf", &path, r));
    }
    Ok(files)
}

/// Write the STL and/or 3MF `o` asks for next to the GLB `out`, from its bytes (the triangles
/// are read once for both), through the print pipeline ([`apply`]).
pub fn write_meshes(glb: &[u8], out: &Path, o: &ModelOptions) -> AssetResult<Meshes> {
    write_meshes_with(glb, out, o, &|_, _| true)
}

/// [`write_meshes`] with progress (and cancelling).
pub fn write_meshes_with(
    glb: &[u8],
    out: &Path,
    o: &ModelOptions,
    progress: Progress<'_>,
) -> AssetResult<Meshes> {
    if !o.stl && !o.three_mf {
        return Ok(Meshes::default());
    }
    let p = apply(Mesh::from_glb(glb)?, &options_of(o), progress)?;
    Ok(Meshes {
        files: write_output(&p, out, o.stl, o.three_mf)?,
        notes: p.notes,
    })
}

#[cfg(test)]
mod tests;
