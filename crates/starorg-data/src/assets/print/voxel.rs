//! Voxel solid (experimental): the model rebuilt as one closed, printable solid from voxels,
//! then thickened, hollowed (with drain holes) and given peg holes; [`extract`] turns a grid
//! back into a closed, consistently outward-oriented 2-manifold mesh. Pipeline steps 6-9 and
//! 11 of [`super::apply`].
//!
//! Owner: the voxel agent. [`VoxelGrid`] is the shared API (split and colour code use it).
//!
//! How it works (robust over pretty):
//!
//! - [`voxelize`]: every triangle marks the cells its (slightly enlarged) box overlaps
//!   (Akenine-Moller separating-axis test), so the marked surface has no 6-connected gaps.
//!   The outside is flood-filled from the padded border through unmarked cells after a
//!   closing of [`SEAL_CELLS`] (gaps in the mesh up to about twice that are sealed). Game hulls
//!   have openings wider than that (cockpits, intakes, missing panels), so a cell the flood
//!   reaches is only outside when it is also in plain view (no surface cell between it and the
//!   border) along at least [`OUTSIDE_VOTES`] of the six axis directions (ray stabbing,
//!   Nooruddin and Turk); the inside of a hull with holes is seen through one or two of them.
//!   Whatever is not outside is solid. Open, self-intersecting and unwelded meshes therefore
//!   still give a closed solid. Trade-off: deep exterior pockets seen from fewer than three
//!   directions (under a belly between landing gear legs, a bore) fill in. The solid is about
//!   half a voxel larger on each side than the mesh. Each cell keeps the group of its closest triangle (surface cells) or
//!   of the surface cell before it along x (inside).
//! - [`extract`]: the solid is made well-composed (Latecki: no two cells meeting only at an
//!   edge or a corner, for the solid or the empty space; fixed by adding cells), then every
//!   solid/empty cell face becomes a quad sharing grid-corner vertices: a closed 2-manifold by
//!   construction, outward by construction. The staircase is then smoothed (Laplacian, each
//!   vertex clamped to [`SMOOTH_CLAMP`] voxels of its corner, undone where a triangle would
//!   turn over), which keeps the topology. Simplifying is the simplify step's job.
//! - [`thicken`], [`hollow`]: morphology on bitsets ([`Bits::erode`], [`Bits::dilate`] with a
//!   discrete ball, chained for large radii).
//! - [`peg`]: peg holes across each cut and a stand hole from the underside.

use super::{Cut, Mesh, Progress};
use crate::assets::{AssetError, AssetResult};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The most cells a grid may have (dense `u8`: about 1 GB).
pub const MAX_CELLS: u64 = 1 << 30;
/// [`voxelize`] coarsens the resolution until the grid has at most this many cells (half of
/// [`MAX_CELLS`]: the morphology needs a few bitsets on top of the cells).
pub const CELL_BUDGET: u64 = MAX_CELLS / 2;
/// Gaps in the mesh up to about twice this many voxels are sealed before the outside is found.
pub const SEAL_CELLS: f32 = 2.0;
/// A cell the flood reaches is outside only when it is in plain view along at least this
/// many of the six axis directions (else it is inside a hull with holes).
pub const OUTSIDE_VOTES: u32 = 3;
/// How far (voxels) smoothing may move a vertex from its grid corner, per axis.
pub const SMOOTH_CLAMP: f32 = 0.4;
/// Smoothing passes over the extracted surface.
pub const SMOOTH_ITERS: usize = 4;
/// The least material kept around a peg or stand hole, millimetres (warned below).
pub const PEG_WALL_MM: f32 = 0.8;
/// The least material left at the end of a peg or stand hole, millimetres.
pub const PEG_CAP_MM: f32 = 0.6;
/// Cavities smaller than this (cubic millimetres) are left solid when hollowing.
pub const MIN_CAVITY_MM3: f32 = 1.0;

/// A dense grid of cubic cells. A cell holds 0 (empty) or `1 + group` (solid, of the group
/// `labels[group]`; 1 when the mesh had no groups).
#[derive(Debug, Clone, PartialEq)]
pub struct VoxelGrid {
    /// The low corner of cell (0, 0, 0), mesh units.
    pub origin: [f32; 3],
    /// The edge of one cell, mesh units.
    pub voxel: f32,
    pub dims: [u32; 3],
    /// The group names cells refer to (the source mesh's `labels`).
    pub labels: Vec<String>,
    cells: Vec<u8>,
}

impl VoxelGrid {
    /// An empty grid; refused above [`MAX_CELLS`].
    pub fn new(origin: [f32; 3], voxel: f32, dims: [u32; 3]) -> AssetResult<Self> {
        let n = dims.iter().map(|d| *d as u64).product::<u64>();
        if n > MAX_CELLS {
            return Err(AssetError::invalid(format!(
                "the solid would need {n} voxels (at most {MAX_CELLS}): use a coarser resolution or a smaller size"
            )));
        }
        if !(voxel.is_finite() && voxel > 0.0) {
            return Err(AssetError::invalid("the voxel size must be more than 0"));
        }
        Ok(Self {
            origin,
            voxel,
            dims,
            labels: Vec::new(),
            cells: vec![0; n as usize],
        })
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The index of cell (x, y, z) in [`VoxelGrid::cells`] (x fastest).
    pub fn index(&self, x: u32, y: u32, z: u32) -> usize {
        (x as usize) + self.dims[0] as usize * (y as usize + self.dims[1] as usize * z as usize)
    }

    /// The cell (0 outside the grid).
    pub fn get(&self, x: u32, y: u32, z: u32) -> u8 {
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] {
            return 0;
        }
        self.cells[self.index(x, y, z)]
    }

    /// Set a cell (ignored outside the grid).
    pub fn set(&mut self, x: u32, y: u32, z: u32, v: u8) {
        if x < self.dims[0] && y < self.dims[1] && z < self.dims[2] {
            let i = self.index(x, y, z);
            self.cells[i] = v;
        }
    }

    pub fn is_solid(&self, x: u32, y: u32, z: u32) -> bool {
        self.get(x, y, z) != 0
    }

    pub fn cells(&self) -> &[u8] {
        &self.cells
    }

    pub fn cells_mut(&mut self) -> &mut [u8] {
        &mut self.cells
    }

    /// Solid cells.
    pub fn count_solid(&self) -> u64 {
        self.cells.iter().filter(|c| **c != 0).count() as u64
    }

    /// The centre of cell (x, y, z), mesh units.
    pub fn center(&self, x: u32, y: u32, z: u32) -> [f32; 3] {
        let c = [x, y, z];
        [0, 1, 2].map(|i| self.origin[i] + (c[i] as f32 + 0.5) * self.voxel)
    }

    /// The cell holding point `p` (`None` outside).
    pub fn cell_at(&self, p: [f32; 3]) -> Option<[u32; 3]> {
        let mut c = [0u32; 3];
        for i in 0..3 {
            let f = ((p[i] - self.origin[i]) / self.voxel).floor();
            if !(f >= 0.0 && f < self.dims[i] as f32) {
                return None;
            }
            c[i] = f as u32;
        }
        Some(c)
    }

    /// The cells `min..max` (exclusive, clamped) as a grid of their own, same labels.
    pub fn crop(&self, min: [u32; 3], max: [u32; 3]) -> AssetResult<VoxelGrid> {
        let max = [0, 1, 2].map(|i| max[i].min(self.dims[i]));
        let min = [0, 1, 2].map(|i| min[i].min(max[i]));
        let dims = [0, 1, 2].map(|i| max[i] - min[i]);
        let origin = [0, 1, 2].map(|i| self.origin[i] + min[i] as f32 * self.voxel);
        let mut g = VoxelGrid::new(origin, self.voxel, dims)?;
        g.labels = self.labels.clone();
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let v = self.get(min[0] + x, min[1] + y, min[2] + z);
                    if v != 0 {
                        g.set(x, y, z, v);
                    }
                }
            }
        }
        Ok(g)
    }

    /// The solid cells (`keep(cell)`) as a bitset of the same dims.
    pub fn bits(&self, keep: &(dyn Fn(u8) -> bool + Sync)) -> Bits {
        let mut b = Bits::new(self.dims);
        let d0 = self.dims[0] as usize;
        if d0 == 0 {
            return b;
        }
        let wpr = b.wpr;
        b.data
            .par_chunks_mut(wpr)
            .zip(self.cells.par_chunks(d0))
            .for_each(|(o, row)| {
                for (x, c) in row.iter().enumerate() {
                    if keep(*c) {
                        o[x / 64] |= 1 << (x % 64);
                    }
                }
            });
        b
    }

    /// The bounds of the solid cells (`min..max`, exclusive), `None` when empty.
    pub fn solid_bounds(&self) -> Option<([u32; 3], [u32; 3])> {
        let d0 = self.dims[0] as usize;
        let d1 = self.dims[1] as usize;
        if d0 == 0 {
            return None;
        }
        self.cells
            .par_chunks(d0)
            .enumerate()
            .filter_map(|(row, cs)| {
                let first = cs.iter().position(|c| *c != 0)?;
                let last = cs.iter().rposition(|c| *c != 0)?;
                let (y, z) = ((row % d1) as u32, (row / d1) as u32);
                Some(([first as u32, y, z], [last as u32 + 1, y + 1, z + 1]))
            })
            .reduce_with(|a, b| {
                (
                    [0, 1, 2].map(|i| a.0[i].min(b.0[i])),
                    [0, 1, 2].map(|i| a.1[i].max(b.1[i])),
                )
            })
    }

    /// This grid with `m` more empty cells on every side (same cells, same world positions).
    pub fn grown(&self, m: u32) -> AssetResult<VoxelGrid> {
        let dims = self.dims.map(|d| d + 2 * m);
        let origin = self.origin.map(|o| o - m as f32 * self.voxel);
        let mut g = VoxelGrid::new(origin, self.voxel, dims)?;
        g.labels = self.labels.clone();
        let d0 = self.dims[0] as usize;
        if d0 == 0 {
            return Ok(g);
        }
        for z in 0..self.dims[2] {
            for y in 0..self.dims[1] {
                let s = self.index(0, y, z);
                let t = g.index(m, y + m, z + m);
                g.cells[t..t + d0].copy_from_slice(&self.cells[s..s + d0]);
            }
        }
        Ok(g)
    }

    /// Turn on the cells of `add` that are empty, each with the group of a solid neighbour
    /// (1 without labels).
    fn add_bits(&mut self, add: &Bits) {
        let labeled = !self.labels.is_empty();
        let mut todo: Vec<[u32; 3]> = add
            .ones()
            .filter(|&[x, y, z]| self.get(x, y, z) == 0)
            .collect();
        if !labeled {
            for [x, y, z] in todo {
                self.set(x, y, z, 1);
            }
            return;
        }
        // Grow labels into the new cells, a layer per pass.
        while !todo.is_empty() {
            let mut set = Vec::new();
            let mut rest = Vec::new();
            for c in todo {
                match neighbour_label(self, c) {
                    Some(v) => set.push((c, v)),
                    None => rest.push(c),
                }
            }
            if set.is_empty() {
                for [x, y, z] in rest {
                    self.set(x, y, z, 1);
                }
                return;
            }
            for ([x, y, z], v) in set {
                self.set(x, y, z, v);
            }
            todo = rest;
        }
    }
}

/// The value of a solid 6-neighbour of `c`.
fn neighbour_label(g: &VoxelGrid, [x, y, z]: [u32; 3]) -> Option<u8> {
    let n = [
        (x.wrapping_sub(1), y, z),
        (x + 1, y, z),
        (x, y.wrapping_sub(1), z),
        (x, y + 1, z),
        (x, y, z.wrapping_sub(1)),
        (x, y, z + 1),
    ];
    n.into_iter()
        .map(|(a, b, c)| g.get(a, b, c))
        .find(|v| *v != 0)
}

// ---------------------------------------------------------------------------------------------
// Bitsets

/// A bit per cell, rows along x (each row padded to whole 64-bit words; the padding stays 0).
#[derive(Debug, Clone, PartialEq)]
pub struct Bits {
    dims: [u32; 3],
    wpr: usize,
    data: Vec<u64>,
}

/// Bits of `src` moved `k` cells up: `out[x] = src[x - k]` (word `i`).
fn up(src: &[u64], k: usize, i: usize) -> u64 {
    let (q, b) = (k / 64, k % 64);
    if i < q {
        return 0;
    }
    let j = i - q;
    let lo = src[j] << b;
    let hi = if b > 0 && j > 0 {
        src[j - 1] >> (64 - b)
    } else {
        0
    };
    lo | hi
}

/// Bits of `src` moved `k` cells down: `out[x] = src[x + k]` (word `i`).
fn down(src: &[u64], k: usize, i: usize) -> u64 {
    let (q, b) = (k / 64, k % 64);
    let j = i + q;
    if j >= src.len() {
        return 0;
    }
    let lo = src[j] >> b;
    let hi = if b > 0 && j + 1 < src.len() {
        src[j + 1] << (64 - b)
    } else {
        0
    };
    lo | hi
}

/// The (dy, dz, half-width along x) rows of a discrete ball of radius `r` cells.
fn ball(r: f32) -> Vec<(i32, i32, usize)> {
    let r2 = r * r + 1e-4;
    let ri = r.floor() as i32;
    let mut v = Vec::new();
    for dz in -ri..=ri {
        for dy in -ri..=ri {
            let s = r2 - (dy * dy + dz * dz) as f32;
            if s >= 0.0 {
                v.push((dy, dz, s.sqrt().floor() as usize));
            }
        }
    }
    v
}

/// The (dx, dy) cells of a disc of radius `r` cells.
fn disc(r: f32) -> Vec<(i32, i32)> {
    let r2 = r * r + 1e-4;
    let ri = r.floor() as i32;
    let mut v = Vec::new();
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if ((dx * dx + dy * dy) as f32) <= r2 {
                v.push((dx, dy));
            }
        }
    }
    v
}

/// The largest ball radius (cells) done in one pass; larger ones are chained.
const BALL_STEP: f32 = 4.0;

impl Bits {
    /// All clear.
    pub fn new(dims: [u32; 3]) -> Self {
        let wpr = (dims[0] as usize).div_ceil(64);
        Self {
            dims,
            wpr,
            data: vec![0; wpr * dims[1] as usize * dims[2] as usize],
        }
    }

    pub fn dims(&self) -> [u32; 3] {
        self.dims
    }

    fn rows(&self) -> usize {
        self.dims[1] as usize * self.dims[2] as usize
    }

    fn row_of(&self, y: u32, z: u32) -> usize {
        (y as usize + self.dims[1] as usize * z as usize) * self.wpr
    }

    fn row(&self, y: i64, z: i64) -> Option<&[u64]> {
        if y < 0 || z < 0 || y >= self.dims[1] as i64 || z >= self.dims[2] as i64 {
            return None;
        }
        let r = self.row_of(y as u32, z as u32);
        Some(&self.data[r..r + self.wpr])
    }

    fn tail(&self) -> u64 {
        match self.dims[0] % 64 {
            0 => !0,
            b => (1u64 << b) - 1,
        }
    }

    fn mask_tails(&mut self) {
        let (t, w) = (self.tail(), self.wpr);
        if w > 0 {
            self.data.par_chunks_mut(w).for_each(|r| r[w - 1] &= t);
        }
    }

    pub fn get(&self, x: u32, y: u32, z: u32) -> bool {
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] {
            return false;
        }
        let r = self.row_of(y, z) + x as usize / 64;
        (self.data[r] >> (x % 64)) & 1 == 1
    }

    pub fn set(&mut self, x: u32, y: u32, z: u32, on: bool) {
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] {
            return;
        }
        let r = self.row_of(y, z) + x as usize / 64;
        let m = 1u64 << (x % 64);
        if on {
            self.data[r] |= m;
        } else {
            self.data[r] &= !m;
        }
    }

    /// Set bits `x0..x1` of row (y, z).
    fn set_run(&mut self, y: u32, z: u32, x0: u32, x1: u32) {
        let r = self.row_of(y, z);
        for x in x0..x1 {
            self.data[r + x as usize / 64] |= 1u64 << (x % 64);
        }
    }

    pub fn count(&self) -> u64 {
        self.data.par_iter().map(|w| w.count_ones() as u64).sum()
    }

    pub fn any(&self) -> bool {
        self.data.par_iter().any(|w| *w != 0)
    }

    /// The set cells, x fastest.
    pub fn ones(&self) -> impl Iterator<Item = [u32; 3]> + '_ {
        let (wpr, d1) = (self.wpr, self.dims[1] as usize);
        self.data.iter().enumerate().flat_map(move |(i, w)| {
            let mut w = *w;
            let row = i / wpr.max(1);
            let base = (i % wpr.max(1)) * 64;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let b = w.trailing_zeros() as usize;
                w &= w - 1;
                Some([(base + b) as u32, (row % d1) as u32, (row / d1) as u32])
            })
        })
    }

    /// The complement (within the dims).
    pub fn not(&self) -> Bits {
        let mut b = self.clone();
        b.data.par_iter_mut().for_each(|w| *w = !*w);
        b.mask_tails();
        b
    }

    pub fn or_with(&mut self, o: &Bits) {
        self.data
            .par_iter_mut()
            .zip(o.data.par_iter())
            .for_each(|(a, b)| *a |= *b);
    }

    pub fn and_with(&mut self, o: &Bits) {
        self.data
            .par_iter_mut()
            .zip(o.data.par_iter())
            .for_each(|(a, b)| *a &= *b);
    }

    pub fn and_not(&mut self, o: &Bits) {
        self.data
            .par_iter_mut()
            .zip(o.data.par_iter())
            .for_each(|(a, b)| *a &= !*b);
    }

    /// Dilation by a ball of radius `r` cells (nothing below 1). Outside the dims is empty.
    pub fn dilate(&self, r: f32) -> Bits {
        self.chain(r, true)
    }

    /// Erosion by a ball of radius `r` cells (nothing below 1). Outside the dims is empty, so
    /// the border erodes.
    pub fn erode(&self, r: f32) -> Bits {
        self.chain(r, false)
    }

    fn chain(&self, r: f32, grow: bool) -> Bits {
        if r.is_nan() || r < 1.0 || self.wpr == 0 {
            return self.clone();
        }
        // Whole steps of BALL_STEP and the rest (balls add up along the axes).
        let mut b = self.morph(r.min(BALL_STEP), grow);
        let mut left = r - r.min(BALL_STEP);
        while left >= 1.0 {
            let s = left.min(BALL_STEP);
            b = b.morph(s, grow);
            left -= s;
        }
        b
    }

    fn morph(&self, r: f32, grow: bool) -> Bits {
        let offs = ball(r);
        let mut out = Bits::new(self.dims);
        let (wpr, d1, tail) = (self.wpr, self.dims[1] as usize, self.tail());
        out.data
            .par_chunks_mut(wpr)
            .enumerate()
            .for_each(|(row, o)| {
                let (y, z) = ((row % d1) as i64, (row / d1) as i64);
                if !grow {
                    o.fill(!0);
                }
                for &(dy, dz, w) in &offs {
                    let Some(s) = self.row(y + dy as i64, z + dz as i64) else {
                        if !grow {
                            o.fill(0);
                            break;
                        }
                        continue;
                    };
                    for i in 0..wpr {
                        let mut v = s[i];
                        if grow {
                            for k in 1..=w {
                                v |= up(s, k, i) | down(s, k, i);
                            }
                            o[i] |= v;
                        } else {
                            for k in 1..=w {
                                v &= up(s, k, i) & down(s, k, i);
                            }
                            o[i] &= v;
                        }
                    }
                }
                o[wpr - 1] &= tail;
            });
        out
    }

    /// The end (exclusive) of the run of `free` cells holding `x` in row `r`.
    fn run_end(&self, free: &impl Fn(usize) -> u64, x: u32) -> u32 {
        let mut i = x as usize / 64;
        let mut m = !free(i) & (!0u64 << (x % 64));
        loop {
            if m != 0 {
                return ((i * 64) as u32 + m.trailing_zeros()).min(self.dims[0]);
            }
            i += 1;
            if i >= self.wpr {
                return self.dims[0];
            }
            m = !free(i);
        }
    }

    /// The start of the run of `free` cells holding `x`.
    fn run_start(&self, free: &impl Fn(usize) -> u64, x: u32) -> u32 {
        let mut i = x as usize / 64;
        let b = x % 64;
        let keep = if b == 63 { !0 } else { (1u64 << (b + 1)) - 1 };
        let mut m = !free(i) & keep;
        loop {
            if m != 0 {
                return (i * 64) as u32 + 64 - m.leading_zeros();
            }
            if i == 0 {
                return 0;
            }
            i -= 1;
            m = !free(i);
        }
    }

    /// Mark in `seen` the cells of `self` 6-connected to `seed` (not yet seen), calling
    /// `on_run(y, z, x0, x1)` for each run of cells filled. Returns the cells filled.
    pub fn flood(
        &self,
        seen: &mut Bits,
        seed: [u32; 3],
        mut on_run: impl FnMut(u32, u32, u32, u32),
    ) -> u64 {
        let mut stack = vec![seed];
        let mut n = 0u64;
        let (d1, d2) = (self.dims[1], self.dims[2]);
        while let Some([x, y, z]) = stack.pop() {
            if !self.get(x, y, z) || seen.get(x, y, z) {
                continue;
            }
            let r = self.row_of(y, z);
            let (x0, x1) = {
                let free = |i: usize| self.data[r + i] & !seen.data[r + i];
                (self.run_start(&free, x), self.run_end(&free, x))
            };
            seen.set_run(y, z, x0, x1);
            n += (x1 - x0) as u64;
            on_run(y, z, x0, x1);
            let mut near = Vec::with_capacity(4);
            if y > 0 {
                near.push((y - 1, z));
            }
            if y + 1 < d1 {
                near.push((y + 1, z));
            }
            if z > 0 {
                near.push((y, z - 1));
            }
            if z + 1 < d2 {
                near.push((y, z + 1));
            }
            for (ny, nz) in near {
                let nr = self.row_of(ny, nz);
                let (w0, w1) = (x0 as usize / 64, (x1 as usize - 1) / 64);
                let mut carry = 0u64;
                for i in w0..=w1 {
                    let lo = if i == w0 { !0u64 << (x0 % 64) } else { !0 };
                    let hi = if i == w1 && x1 % 64 != 0 {
                        (1u64 << (x1 % 64)) - 1
                    } else {
                        !0
                    };
                    let m = self.data[nr + i] & !seen.data[nr + i] & lo & hi;
                    let mut starts = m & !((m << 1) | carry);
                    carry = m >> 63;
                    while starts != 0 {
                        let b = starts.trailing_zeros();
                        starts &= starts - 1;
                        stack.push([(i * 64) as u32 + b, ny, nz]);
                    }
                }
            }
        }
        n
    }

    /// Each 6-connected component of the set cells, as `f(cells, runs)` with its runs
    /// `(y, z, x0, x1)`.
    pub fn components(&self, mut f: impl FnMut(u64, Vec<[u32; 4]>)) {
        let mut seen = Bits::new(self.dims);
        for wi in 0..self.data.len() {
            loop {
                let m = self.data[wi] & !seen.data[wi];
                if m == 0 {
                    break;
                }
                let row = wi / self.wpr;
                let x = ((wi % self.wpr) * 64) as u32 + m.trailing_zeros();
                let d1 = self.dims[1] as usize;
                let seed = [x, (row % d1) as u32, (row / d1) as u32];
                let mut runs = Vec::new();
                let n = self.flood(&mut seen, seed, |y, z, a, b| runs.push([y, z, a, b]));
                f(n, runs);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Voxelising

fn tick(p: Progress<'_>, f: f32, s: &str) -> AssetResult<()> {
    if p(f, s) {
        Ok(())
    } else {
        Err(AssetError::cancelled())
    }
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Does triangle `t` overlap the cube of centre `c` and half edge `h`? (Akenine-Moller.)
fn tri_box(c: [f32; 3], h: f32, t: &[[f32; 3]; 3]) -> bool {
    let v = [sub3(t[0], c), sub3(t[1], c), sub3(t[2], c)];
    let (lo, hi) = (
        [0, 1, 2].map(|i| v[0][i].min(v[1][i]).min(v[2][i])),
        [0, 1, 2].map(|i| v[0][i].max(v[1][i]).max(v[2][i])),
    );
    if (0..3).any(|i| lo[i] > h || hi[i] < -h) {
        return false;
    }
    let e = [sub3(v[1], v[0]), sub3(v[2], v[1]), sub3(v[0], v[2])];
    let n = cross(e[0], e[1]);
    let d = dot(n, v[0]);
    if d.abs() > h * (n[0].abs() + n[1].abs() + n[2].abs()) {
        return false;
    }
    for ej in e {
        for i in 0..3 {
            let mut u = [0.0; 3];
            u[i] = 1.0;
            let a = cross(u, ej);
            let p = [dot(a, v[0]), dot(a, v[1]), dot(a, v[2])];
            let r = h * (a[0].abs() + a[1].abs() + a[2].abs());
            if p[0].min(p[1]).min(p[2]) > r || p[0].max(p[1]).max(p[2]) < -r {
                return false;
            }
        }
    }
    true
}

/// The resolution and grid a mesh of `size` gets at `res`, coarsened to [`CELL_BUDGET`].
fn plan_grid(size: [f32; 3], res: f32, pad: u32) -> ([u32; 3], f32) {
    let mut res = res;
    loop {
        let dims = size.map(|s| ((s / res).ceil().max(1.0) as u32) + 2 * pad);
        let n: u64 = dims.iter().map(|d| *d as u64).product();
        if n <= CELL_BUDGET {
            return (dims, res);
        }
        res *= ((n as f64 / CELL_BUDGET as f64).cbrt() as f32).max(1.01) * 1.001;
    }
}

/// A note when [`voxelize`] used a coarser voxel than `asked` (to stay in [`CELL_BUDGET`]).
pub fn coarsened(grid: &VoxelGrid, asked: f32) -> Option<String> {
    (grid.voxel > asked * 1.0001).then(|| {
        format!(
            "The solid uses {:.3} mm voxels instead of {:.3} mm (the model is too large for the finer grid).",
            grid.voxel, asked
        )
    })
}

/// Fill the inside of `mesh` (mesh units, millimetres) with cells of edge `res` (coarsened to
/// [`CELL_BUDGET`], see [`coarsened`]), labelled by the mesh's groups (the closest triangle
/// for surface cells); open and self-intersecting meshes still give a closed solid (see the
/// module doc).
pub fn voxelize(mesh: &Mesh, res: f32, progress: Progress<'_>) -> AssetResult<VoxelGrid> {
    if !(res.is_finite() && res > 0.0) {
        return Err(AssetError::invalid("the voxel size must be more than 0"));
    }
    let b = mesh
        .bounds()
        .ok_or_else(|| AssetError::invalid("the model has no triangles"))?;
    tick(progress, 0.0, "Building the solid")?;
    let seal = SEAL_CELLS;
    let pad = seal.ceil() as u32 + 2;
    let (dims, res) = plan_grid(b.size(), res, pad);
    // Half a cell off so axis-aligned faces at round sizes fall inside cells, not on borders.
    let origin = [0, 1, 2].map(|i| b.min[i] - (pad as f32 + 0.5) * res);
    let mut grid = VoxelGrid::new(origin, res, dims)?;
    let labeled = mesh.groups.is_some() && !mesh.labels.is_empty();
    if labeled {
        grid.labels = mesh.labels.clone();
    }
    rasterize(mesh, &mut grid, labeled, progress)?;
    tick(progress, 0.5, "Finding the inside")?;
    // Outside: flood from the corner through cells away from the (dilated) surface, keep what
    // is in plain view from enough directions (a hull with holes stays inside), then grow it
    // back by the same ball as the dilation (a closing of the surface).
    let surface = grid.bits(&|c| c != 0);
    let sealed = surface.dilate(seal);
    let open = sealed.not();
    let mut outside = Bits::new(dims);
    open.flood(&mut outside, [0, 0, 0], |_, _, _, _| {});
    drop(open);
    drop(sealed);
    tick(progress, 0.65, "Finding the inside")?;
    outside.and_with(&seen_from(&surface, OUTSIDE_VOTES));
    tick(progress, 0.75, "Finding the inside")?;
    let solid = outside.dilate(seal).not();
    drop(outside);
    drop(surface);
    tick(progress, 0.9, "Finding the inside")?;
    fill_inside(&mut grid, &solid, labeled);
    tick(progress, 1.0, "Building the solid")?;
    Ok(grid)
}

/// `a + b + c` per bit: (sum, carry).
fn full_add(a: u64, b: u64, c: u64) -> (u64, u64) {
    (a ^ b ^ c, (a & b) | (c & (a ^ b)))
}

/// The cells with no `surface` cell between them and the grid's border along at least `k`
/// (1..=6) of the six axis directions.
fn seen_from(surface: &Bits, k: u32) -> Bits {
    let [d0, d1, d2] = surface.dims.map(|d| d as usize);
    let wpr = surface.wpr;
    let tail = surface.tail();
    let full = {
        let mut r = vec![!0u64; wpr];
        if wpr > 0 {
            r[wpr - 1] = tail;
        }
        r
    };
    // Along z (both ways) and y (both ways): sweep keeping what is still clear.
    let sweep = |outer: usize, inner: usize, row: &dyn Fn(usize, usize) -> usize, rev: bool| {
        let mut out = Bits::new(surface.dims);
        for o in 0..outer {
            let mut clear = full.clone();
            for s in 0..inner {
                let s = if rev { inner - 1 - s } else { s };
                let r = row(o, s) * wpr;
                for (c, w) in clear.iter_mut().zip(&surface.data[r..r + wpr]) {
                    *c &= !*w;
                }
                out.data[r..r + wpr].copy_from_slice(&clear);
            }
        }
        out
    };
    let zrow = |y: usize, z: usize| y + d1 * z;
    let mz = sweep(d1, d2, &zrow, false);
    let pz = sweep(d1, d2, &zrow, true);
    let yrow = |z: usize, y: usize| y + d1 * z;
    let my = sweep(d2, d1, &yrow, false);
    let py = sweep(d2, d1, &yrow, true);
    let mut out = Bits::new(surface.dims);
    if wpr == 0 {
        return out;
    }
    out.data
        .par_chunks_mut(wpr)
        .enumerate()
        .for_each(|(row, o)| {
            let s = &surface.data[row * wpr..(row + 1) * wpr];
            // Along x: before the first and after the last surface cell of the row.
            let first = s
                .iter()
                .position(|w| *w != 0)
                .map(|i| i * 64 + s[i].trailing_zeros() as usize)
                .unwrap_or(d0);
            let last = s
                .iter()
                .rposition(|w| *w != 0)
                .map(|i| i * 64 + 63 - s[i].leading_zeros() as usize);
            for (i, oi) in o.iter_mut().enumerate() {
                let bit_range = |lo: usize, hi: usize| -> u64 {
                    // Bits lo..hi of word i.
                    let (a, b) = (lo.max(i * 64), hi.min(i * 64 + 64));
                    if a >= b {
                        return 0;
                    }
                    let n = b - a;
                    let m = if n == 64 { !0 } else { (1u64 << n) - 1 };
                    m << (a - i * 64)
                };
                let mx = bit_range(0, first);
                let px = match last {
                    Some(l) => bit_range(l + 1, d0),
                    None => bit_range(0, d0),
                };
                let r = row * wpr + i;
                let (s1, c1) = full_add(mx, px, mz.data[r]);
                let (s2, c2) = full_add(pz.data[r], my.data[r], py.data[r]);
                let (low, c3) = (s1 ^ s2, s1 & s2);
                // total = low + 2 * (c1 + c2 + c3)
                let twos_ge1 = c1 | c2 | c3;
                let twos_ge2 = (c1 & c2) | (c1 & c3) | (c2 & c3);
                let twos_3 = c1 & c2 & c3;
                *oi = match k {
                    0 | 1 => mx | px | mz.data[r] | pz.data[r] | my.data[r] | py.data[r],
                    2 => twos_ge1,
                    3 => twos_ge2 | (twos_ge1 & low),
                    4 => twos_ge2,
                    5 => twos_3 | (twos_ge2 & low),
                    _ => twos_3,
                };
            }
            o[wpr - 1] &= tail;
        });
    out
}

/// Mark the cells each triangle overlaps (1 + group; the closest triangle wins).
fn rasterize(
    mesh: &Mesh,
    grid: &mut VoxelGrid,
    labeled: bool,
    progress: Progress<'_>,
) -> AssetResult<()> {
    let (origin, res, dims) = (grid.origin, grid.voxel, grid.dims);
    let tris: Vec<[[f32; 3]; 3]> = (0..mesh.triangles.len())
        .map(|t| {
            mesh.triangle(t)
                .map(|p| [0, 1, 2].map(|i| (p[i] - origin[i]) / res))
        })
        .collect();
    let plane = dims[0] as usize * dims[1] as usize;
    let threads = rayon::current_num_threads().max(1) as u32;
    let layers = dims[2].div_ceil(threads * 8).max(1);
    let nslabs = dims[2].div_ceil(layers) as usize;
    const EPS: f32 = 1e-3;
    let cell_range = |lo: f32, hi: f32, n: u32| -> (u32, u32) {
        let a = (lo - EPS).floor().max(0.0) as u32;
        let b = ((hi + EPS).floor().max(0.0) as u32).min(n.saturating_sub(1));
        (a, b)
    };
    let mut bins: Vec<Vec<u32>> = vec![Vec::new(); nslabs];
    for (t, tri) in tris.iter().enumerate() {
        let (lo, hi) = (
            tri[0][2].min(tri[1][2]).min(tri[2][2]),
            tri[0][2].max(tri[1][2]).max(tri[2][2]),
        );
        let (z0, z1) = cell_range(lo, hi, dims[2]);
        for s in (z0 / layers)..=(z1 / layers) {
            bins[s as usize].push(t as u32);
        }
    }
    let done = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let h = 0.5 + EPS;
    grid.cells
        .par_chunks_mut(plane * layers as usize)
        .enumerate()
        .for_each(|(s, chunk)| {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let zs = s as u32 * layers;
            let ze = (zs + layers).min(dims[2]);
            let mut best: HashMap<usize, f32> = HashMap::new();
            for &t in &bins[s] {
                let tri = &tris[t as usize];
                let v = if labeled {
                    1 + mesh.group(t as usize).min(254) as u8
                } else {
                    1
                };
                let lo = [0, 1, 2].map(|i| tri[0][i].min(tri[1][i]).min(tri[2][i]));
                let hi = [0, 1, 2].map(|i| tri[0][i].max(tri[1][i]).max(tri[2][i]));
                let (x0, x1) = cell_range(lo[0], hi[0], dims[0]);
                let (y0, y1) = cell_range(lo[1], hi[1], dims[1]);
                let (mut z0, mut z1) = cell_range(lo[2], hi[2], dims[2]);
                z0 = z0.max(zs);
                z1 = z1.min(ze - 1);
                let n = cross(sub3(tri[1], tri[0]), sub3(tri[2], tri[0]));
                let nl1 = n[0].abs() + n[1].abs() + n[2].abs();
                let nlen = dot(n, n).sqrt();
                let d = dot(n, tri[0]);
                let r = h * nl1;
                for z in z0..=z1.max(z0) {
                    if z > z1 {
                        break;
                    }
                    for y in y0..=y1 {
                        let (cy, cz) = (y as f32 + 0.5, z as f32 + 0.5);
                        // The cells along x the triangle's plane can reach in this row.
                        let (mut a, mut b) = (x0, x1);
                        if n[0].abs() > 1e-6 * nl1 {
                            let m = d - n[1] * cy - n[2] * cz;
                            let (p, q) = ((m - r) / n[0], (m + r) / n[0]);
                            let (p, q) = (p.min(q) - 0.5, p.max(q) - 0.5);
                            a = a.max(p.floor().max(0.0) as u32);
                            b = b.min(q.ceil().max(0.0) as u32);
                        }
                        for x in a..=b.max(a) {
                            if x > b {
                                break;
                            }
                            let c = [x as f32 + 0.5, cy, cz];
                            if !tri_box(c, h, tri) {
                                continue;
                            }
                            let i = x as usize
                                + dims[0] as usize
                                    * (y as usize + dims[1] as usize * (z - zs) as usize);
                            if labeled {
                                let dist = if nlen > 0.0 {
                                    (dot(n, c) - d).abs() / nlen
                                } else {
                                    0.0
                                };
                                let e = best.entry(i).or_insert(f32::INFINITY);
                                if dist < *e {
                                    *e = dist;
                                    chunk[i] = v;
                                }
                            } else {
                                chunk[i] = 1;
                            }
                        }
                    }
                }
            }
            let k = done.fetch_add(1, Ordering::Relaxed) + 1;
            if !progress(0.5 * k as f32 / nslabs as f32, "Building the solid") {
                stop.store(true, Ordering::Relaxed);
            }
        });
    if stop.load(Ordering::Relaxed) {
        return Err(AssetError::cancelled());
    }
    Ok(())
}

/// Keep only `solid`, labelling solid cells not on the surface: the label of the surface cell
/// before (or else after) them along x; 1 without labels.
fn fill_inside(grid: &mut VoxelGrid, solid: &Bits, labeled: bool) {
    let d0 = grid.dims[0] as usize;
    if d0 == 0 {
        return;
    }
    let wpr = solid.wpr;
    grid.cells
        .par_chunks_mut(d0)
        .zip(solid.data.par_chunks(wpr))
        .for_each(|(row, s)| {
            let on = |x: usize| (s[x / 64] >> (x % 64)) & 1 == 1;
            let mut last = 0u8;
            for (x, c) in row.iter_mut().enumerate() {
                if !on(x) {
                    *c = 0;
                } else if *c != 0 {
                    last = *c;
                } else if !labeled {
                    *c = 1;
                } else {
                    *c = last;
                }
            }
            if labeled {
                let mut next = 0u8;
                for x in (0..d0).rev() {
                    if row[x] != 0 {
                        next = row[x];
                    } else if on(x) {
                        row[x] = if next != 0 { next } else { 1 };
                    }
                }
            }
        });
}

// ---------------------------------------------------------------------------------------------
// Thicken, hollow

/// Grow the solid so no part is thinner than `min` (mesh units): the parts an opening of
/// radius `min / 2` removes are dilated by `min / 2` (so a sheet becomes at least `min`
/// thick). Trade-off: thin parts grow outwards on both sides (their outline moves by up to
/// `min / 2`) and gaps narrower than `min` between them fill in; thick parts are untouched.
/// The grid grows when the solid is too close to its border.
pub fn thicken(grid: &mut VoxelGrid, min: f32, progress: Progress<'_>) -> AssetResult<()> {
    let r = 0.5 * min / grid.voxel;
    if r.is_nan() || r < 1.0 {
        return Ok(());
    }
    tick(progress, 0.0, "Thickening thin parts")?;
    let margin = r.ceil() as u32 + 1;
    if let Some((lo, hi)) = grid.solid_bounds()
        && (0..3).any(|i| lo[i] < margin || hi[i] + margin > grid.dims[i])
    {
        *grid = grid.grown(margin)?;
    }
    let solid = grid.bits(&|c| c != 0);
    let open = solid.erode(r);
    tick(progress, 0.3, "Thickening thin parts")?;
    let open = open.dilate(r);
    tick(progress, 0.5, "Thickening thin parts")?;
    // What the opening removes next to what it keeps is a rounded edge or corner of a thick
    // part (at most (sqrt 3 - 1) r away), not a thin part.
    let near = open.dilate(0.75 * r + 1.0);
    drop(open);
    tick(progress, 0.7, "Thickening thin parts")?;
    let mut thin = solid;
    thin.and_not(&near);
    drop(near);
    if !thin.any() {
        return Ok(());
    }
    let add = thin.dilate(r);
    tick(progress, 0.9, "Thickening thin parts")?;
    grid.add_bits(&add);
    tick(progress, 1.0, "Thickening thin parts")
}

/// Hollow the solid, keeping walls `wall` thick (mesh units), with drain holes of diameter
/// `drain` (0 = none) from the lowest point of each cavity straight down through the bottom
/// wall (a second one for a large cavity); says where in `notes`. Cavities under
/// [`MIN_CAVITY_MM3`] stay solid.
pub fn hollow(
    grid: &mut VoxelGrid,
    wall: f32,
    drain: f32,
    notes: &mut Vec<String>,
    progress: Progress<'_>,
) -> AssetResult<()> {
    tick(progress, 0.0, "Hollowing")?;
    let v = grid.voxel;
    let solid = grid.bits(&|c| c != 0);
    let inner = solid.erode(wall / v);
    drop(solid);
    tick(progress, 0.5, "Hollowing")?;
    let min_cells = (MIN_CAVITY_MM3 / (v * v * v)).ceil() as u64;
    let mut cavities = Vec::new();
    inner.components(|n, runs| {
        if n >= min_cells.max(1) {
            cavities.push(runs);
        }
    });
    drop(inner);
    if cavities.is_empty() {
        notes.push(format!(
            "Not hollowed: the model is too thin for {wall} mm walls."
        ));
        return Ok(());
    }
    tick(progress, 0.7, "Hollowing")?;
    for runs in &cavities {
        for &[y, z, x0, x1] in runs {
            let i = grid.index(x0, y, z);
            grid.cells[i..i + (x1 - x0) as usize].fill(0);
        }
    }
    let mut holes = Vec::new();
    if drain > 0.0 {
        let r = (0.5 * drain / v).max(0.5);
        for runs in &cavities {
            for [x, y, z] in drain_points(runs, drain / v) {
                if drill_down(grid, x, y, z, r) {
                    let c = grid.center(x, y, 0);
                    holes.push(format!("x {:.1} y {:.1}", c[0] + 0.0, c[1] + 0.0));
                }
            }
        }
    }
    let mut note = format!(
        "Hollowed with {wall} mm walls: {} cavit{}",
        cavities.len(),
        if cavities.len() == 1 { "y" } else { "ies" }
    );
    if drain > 0.0 {
        if holes.is_empty() {
            note.push_str(", no drain hole (no way down)");
        } else {
            note.push_str(&format!(
                ", {} drain hole{} of {drain} mm at {}",
                holes.len(),
                if holes.len() == 1 { "" } else { "s" },
                holes.join(", ")
            ));
        }
    } else {
        note.push_str(", no drain hole");
    }
    note.push('.');
    notes.push(note);
    tick(progress, 1.0, "Hollowing")
}

/// Where a cavity drains: its lowest cell (closest to its centre), and for a cavity wider than
/// eight holes the lowest cell at least half its width away from that.
fn drain_points(runs: &[[u32; 4]], hole: f32) -> Vec<[u32; 3]> {
    let (mut sx, mut sy, mut n) = (0f64, 0f64, 0f64);
    let mut lo = [u32::MAX; 2];
    let mut hi = [0u32; 2];
    let mut zmin = u32::MAX;
    for &[y, z, x0, x1] in runs {
        let k = (x1 - x0) as f64;
        sx += (x0 + x1 - 1) as f64 * 0.5 * k;
        sy += y as f64 * k;
        n += k;
        lo = [lo[0].min(x0), lo[1].min(y)];
        hi = [hi[0].max(x1), hi[1].max(y + 1)];
        zmin = zmin.min(z);
    }
    if n == 0.0 {
        return Vec::new();
    }
    let (cx, cy) = ((sx / n) as f32, (sy / n) as f32);
    let near = |x: u32, y: u32, px: f32, py: f32| {
        let (dx, dy) = (x as f32 - px, y as f32 - py);
        dx * dx + dy * dy
    };
    // The cell of run `r` closest to (px, py).
    let at = |r: &[u32; 4], px: f32| -> u32 { (px.round().max(0.0) as u32).clamp(r[2], r[3] - 1) };
    let first = runs
        .iter()
        .filter(|r| r[1] == zmin)
        .map(|r| [at(r, cx), r[0], r[1]])
        .min_by(|a, b| near(a[0], a[1], cx, cy).total_cmp(&near(b[0], b[1], cx, cy)))
        .unwrap_or([0, 0, 0]);
    let mut out = vec![first];
    let width = (hi[0] - lo[0]).max(hi[1] - lo[1]) as f32;
    if width >= 8.0 * hole {
        let far = 0.5 * width;
        let (fx, fy) = (first[0] as f32, first[1] as f32);
        let second = runs
            .iter()
            .flat_map(|r| [[r[2], r[0], r[1]], [r[3] - 1, r[0], r[1]]])
            .filter(|c| near(c[0], c[1], fx, fy) >= far * far)
            .min_by_key(|c| c[2]);
        if let Some(s) = second {
            out.push(s);
        }
    }
    out
}

/// Drill a vertical hole of radius `r` cells down from just under cavity cell (x, y, z)
/// until it comes out (an empty layer below it, or the bottom of the grid). False when the
/// column was already open.
fn drill_down(grid: &mut VoxelGrid, x: u32, y: u32, z: u32, r: f32) -> bool {
    let d = disc(r);
    let cells = |zz: u32| {
        d.iter().filter_map(move |&(dx, dy)| {
            let (a, b) = (x as i64 + dx as i64, y as i64 + dy as i64);
            (a >= 0 && b >= 0).then_some((a as u32, b as u32, zz))
        })
    };
    let mut cleared = false;
    let mut zz = z;
    while zz > 0 {
        zz -= 1;
        for (a, b, c) in cells(zz) {
            if grid.get(a, b, c) != 0 {
                grid.set(a, b, c, 0);
                cleared = true;
            }
        }
        if zz == 0 || cells(zz - 1).all(|(a, b, c)| grid.get(a, b, c) == 0) {
            break;
        }
    }
    cleared
}

// ---------------------------------------------------------------------------------------------
// Pegs

/// Squared Euclidean distance transform along one line (Felzenszwalb-Huttenlocher).
fn edt_1d(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let fq = f[q] + (q * q) as f64;
        let mut s;
        loop {
            let p = v[k];
            s = (fq - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let d = q as f64 - p as f64;
        *o = d * d + f[p];
    }
}

/// The distance (cells) from each cell of the `w` x `h` mask to the nearest cell outside it
/// (outside the mask's bounds counts as outside).
fn edt_2d(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    // Padded by one cell of background on every side; "far" is above any squared distance.
    let (pw, ph) = (w + 2, h + 2);
    let far = 1e10f64;
    let mut g = vec![0f64; pw * ph];
    for y in 0..h {
        for x in 0..w {
            if mask[x + w * y] {
                g[(x + 1) + pw * (y + 1)] = far;
            }
        }
    }
    let m = pw.max(ph);
    let (mut v, mut z) = (vec![0usize; m], vec![0f64; m + 1]);
    let mut line = vec![0f64; m];
    let mut out = vec![0f64; m];
    for x in 0..pw {
        for y in 0..ph {
            line[y] = g[x + pw * y];
        }
        edt_1d(&line[..ph], &mut out[..ph], &mut v, &mut z);
        for y in 0..ph {
            g[x + pw * y] = out[y];
        }
    }
    for y in 0..ph {
        line[..pw].copy_from_slice(&g[pw * y..pw * (y + 1)]);
        edt_1d(&line[..pw], &mut out[..pw], &mut v, &mut z);
        g[pw * y..pw * (y + 1)].copy_from_slice(&out[..pw]);
    }
    let mut d = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            d[x + w * y] = g[(x + 1) + pw * (y + 1)].sqrt() as f32;
        }
    }
    d
}

/// The cell (u, v) along axes `ua`, `va` and `a = k` as grid coordinates.
fn cell3(a: usize, ua: usize, va: usize, k: u32, u: u32, v: u32) -> [u32; 3] {
    let mut c = [0u32; 3];
    c[a] = k;
    c[ua] = u;
    c[va] = v;
    c
}

/// Peg holes of diameter `diameter` (mesh units), `depth` deep into each side, across every
/// cut of `cuts` (from [`super::split::plan`]): one at the most solid point of each piece of
/// the cut face (a second for a large face), so the split pieces can be pinned together; and a
/// stand hole `depth` deep into the underside, as near the centre of mass (x/y) as there is
/// room. Holes are clamped so they never break through (at least [`PEG_CAP_MM`] left), with a
/// note when clamped, when the wall around them is under [`PEG_WALL_MM`], or when there is
/// no room.
pub fn peg(
    grid: &mut VoxelGrid,
    cuts: &[Cut],
    diameter: f32,
    depth: f32,
    notes: &mut Vec<String>,
) -> AssetResult<()> {
    if !(diameter > 0.0 && depth > 0.0) {
        return Ok(());
    }
    let v = grid.voxel;
    let r = 0.5 * diameter / v;
    let wall = PEG_WALL_MM / v;
    let cap = (PEG_CAP_MM / v).ceil().max(1.0) as u32;
    let depth_cells = (depth / v).round().max(1.0) as u32;
    for (n, cut) in cuts.iter().enumerate() {
        cut_pegs(grid, n + 1, cut, r, wall, cap, depth_cells, diameter, notes);
    }
    stand_hole(grid, r, wall, cap, depth_cells, diameter, depth, notes);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cut_pegs(
    grid: &mut VoxelGrid,
    n: usize,
    cut: &Cut,
    r: f32,
    wall: f32,
    cap: u32,
    depth: u32,
    diameter: f32,
    notes: &mut Vec<String>,
) {
    let a = cut.axis.min(2);
    let (ua, va) = ((a + 1) % 3, (a + 2) % 3);
    let v = grid.voxel;
    let k = ((cut.at - grid.origin[a]) / v - 0.5).ceil();
    if !(k >= 1.0 && k < grid.dims[a] as f32) {
        return;
    }
    let k = k as u32;
    let (w, h) = (grid.dims[ua] as usize, grid.dims[va] as usize);
    let mut mask = vec![false; w * h];
    for vv in 0..h {
        for uu in 0..w {
            let lo = cell3(a, ua, va, k - 1, uu as u32, vv as u32);
            let hi = cell3(a, ua, va, k, uu as u32, vv as u32);
            mask[uu + w * vv] =
                grid.is_solid(lo[0], lo[1], lo[2]) && grid.is_solid(hi[0], hi[1], hi[2]);
        }
    }
    let dist = edt_2d(&mask, w, h);
    // 4-connected pieces of the cut face, largest first.
    let mut comp = vec![u32::MAX; w * h];
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    for s in 0..w * h {
        if !mask[s] || comp[s] != u32::MAX {
            continue;
        }
        let id = pieces.len() as u32;
        let mut stack = vec![s];
        comp[s] = id;
        let mut cells = Vec::new();
        while let Some(c) = stack.pop() {
            cells.push(c);
            let (x, y) = (c % w, c / w);
            let mut push = |q: usize| {
                if mask[q] && comp[q] == u32::MAX {
                    comp[q] = id;
                    stack.push(q);
                }
            };
            if x > 0 {
                push(c - 1);
            }
            if x + 1 < w {
                push(c + 1);
            }
            if y > 0 {
                push(c - w);
            }
            if y + 1 < h {
                push(c + w);
            }
        }
        pieces.push(cells);
    }
    pieces.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let axis = ["x", "y", "z"][a];
    let mut placed = Vec::new();
    let mut thin = false;
    let mut clamped = false;
    for piece in pieces.iter().take(4) {
        let best = piece
            .iter()
            .copied()
            .max_by(|p, q| dist[*p].total_cmp(&dist[*q]));
        let Some(best) = best else { continue };
        if dist[best] < r + 1.0 {
            continue;
        }
        let mut spots = vec![best];
        // A second peg on a large face, so the pieces cannot turn.
        let apart = 6.0 * r;
        let (bx, by) = ((best % w) as f32, (best / w) as f32);
        let second = piece
            .iter()
            .copied()
            .filter(|p| {
                let (dx, dy) = ((p % w) as f32 - bx, (p / w) as f32 - by);
                dx * dx + dy * dy >= apart * apart
            })
            .max_by(|p, q| dist[*p].total_cmp(&dist[*q]));
        if let Some(s) = second
            && dist[s] >= 2.0 * r + wall
        {
            spots.push(s);
        }
        for s in spots {
            thin |= dist[s] < r + wall;
            let (u0, v0) = ((s % w) as u32, (s / w) as u32);
            let d = disc(r);
            let cells: Vec<(u32, u32)> = d
                .iter()
                .filter_map(|&(du, dv)| {
                    let (uu, vv) = (u0 as i64 + du as i64, v0 as i64 + dv as i64);
                    (uu >= 0 && vv >= 0 && (uu as usize) < w && (vv as usize) < h)
                        .then_some((uu as u32, vv as u32))
                })
                .collect();
            // Solid run from the cut face, each side.
            let run = |g: &VoxelGrid, up: bool| -> u32 {
                cells
                    .iter()
                    .map(|&(uu, vv)| {
                        let mut n = 0u32;
                        loop {
                            let kk = if up {
                                k as i64 + n as i64
                            } else {
                                k as i64 - 1 - n as i64
                            };
                            if kk < 0 || kk >= g.dims[a] as i64 {
                                break;
                            }
                            let c = cell3(a, ua, va, kk as u32, uu, vv);
                            if !g.is_solid(c[0], c[1], c[2]) {
                                break;
                            }
                            n += 1;
                        }
                        n
                    })
                    .min()
                    .unwrap_or(0)
            };
            let mut made = false;
            for up in [false, true] {
                let room = run(grid, up).saturating_sub(cap);
                let dd = depth.min(room);
                clamped |= dd < depth;
                if dd == 0 {
                    continue;
                }
                made = true;
                for &(uu, vv) in &cells {
                    for i in 0..dd {
                        let kk = if up { k + i } else { k - 1 - i };
                        let c = cell3(a, ua, va, kk, uu, vv);
                        grid.set(c[0], c[1], c[2], 0);
                    }
                }
            }
            if made {
                let [cx, cy, cz] = cell3(a, ua, va, k, u0, v0);
                let c = grid.center(cx, cy, cz);
                let names = ["x", "y", "z"];
                placed.push(format!(
                    "{} {:.1} {} {:.1}",
                    names[ua],
                    c[ua] + 0.0,
                    names[va],
                    c[va] + 0.0
                ));
            }
        }
    }
    if placed.is_empty() {
        notes.push(format!(
            "Cut {n} ({axis} {:.1}): no room for a {diameter} mm peg hole.",
            cut.at
        ));
        return;
    }
    let mut note = format!(
        "Cut {n} ({axis} {:.1}): {} peg hole{} of {diameter} mm at {}",
        cut.at,
        placed.len(),
        if placed.len() == 1 { "" } else { "s" },
        placed.join(", ")
    );
    if clamped {
        note.push_str(" (shallower where the part is thin)");
    }
    if thin {
        note.push_str(&format!(
            " (warning: under {PEG_WALL_MM} mm of wall around)"
        ));
    }
    note.push('.');
    notes.push(note);
}

#[allow(clippy::too_many_arguments)]
fn stand_hole(
    grid: &mut VoxelGrid,
    r: f32,
    wall: f32,
    cap: u32,
    depth: u32,
    diameter: f32,
    depth_mm: f32,
    notes: &mut Vec<String>,
) {
    let [d0, d1, d2] = grid.dims.map(|d| d as usize);
    // Per column: the lowest solid cell and the top (exclusive) of the run above it.
    let mut bottom = vec![u32::MAX; d0 * d1];
    let mut top = vec![0u32; d0 * d1];
    let (mut sx, mut sy, mut n) = (0f64, 0f64, 0f64);
    for z in 0..d2 {
        let layer = &grid.cells[d0 * d1 * z..d0 * d1 * (z + 1)];
        for (c, v) in layer.iter().enumerate() {
            if *v == 0 {
                continue;
            }
            if bottom[c] == u32::MAX {
                bottom[c] = z as u32;
                top[c] = z as u32 + 1;
            } else if top[c] == z as u32 {
                top[c] += 1;
            }
            sx += (c % d0) as f64;
            sy += (c / d0) as f64;
            n += 1.0;
        }
    }
    if n == 0.0 {
        return;
    }
    let (cx, cy) = ((sx / n) as f32, (sy / n) as f32);
    let foot: Vec<bool> = bottom.iter().map(|b| *b != u32::MAX).collect();
    let dist = edt_2d(&foot, d0, d1);
    let mut order: Vec<usize> = (0..d0 * d1).filter(|c| dist[*c] >= r + 1.0).collect();
    let far = |c: usize| {
        let (dx, dy) = ((c % d0) as f32 - cx, (c / d0) as f32 - cy);
        dx * dx + dy * dy
    };
    order.sort_by(|a, b| far(*a).total_cmp(&far(*b)));
    let d = disc(r);
    let slope = depth.div_ceil(2);
    // The first spot with room, nearest the centre of mass; a full-depth one a little
    // further (up to two diameters) is preferred.
    let mut pick: Option<(usize, Vec<usize>, u32, u32)> = None;
    for c in order {
        if let Some((f, _, _, _)) = &pick
            && far(c).sqrt() > far(*f).sqrt() + 4.0 * r
        {
            break;
        }
        let (x0, y0) = ((c % d0) as i64, (c / d0) as i64);
        let cols: Vec<usize> = d
            .iter()
            .filter_map(|&(dx, dy)| {
                let (x, y) = (x0 + dx as i64, y0 + dy as i64);
                (x >= 0 && y >= 0 && (x as usize) < d0 && (y as usize) < d1)
                    .then_some(x as usize + d0 * y as usize)
            })
            .collect();
        if cols.iter().any(|c| !foot[*c]) {
            continue;
        }
        let z0 = cols.iter().map(|c| bottom[*c]).min().unwrap_or(0);
        let zb = cols.iter().map(|c| bottom[*c]).max().unwrap_or(0);
        if zb - z0 > slope {
            continue;
        }
        let room = cols
            .iter()
            .map(|c| top[*c].saturating_sub(z0 + cap))
            .min()
            .unwrap_or(0);
        let dd = depth.min(room);
        if dd < 2 {
            continue;
        }
        let full = dd == depth;
        if pick.is_none() || full {
            pick = Some((c, cols, z0, dd));
        }
        if full {
            break;
        }
    }
    if let Some((c, cols, z0, dd)) = pick {
        for &col in &cols {
            let (x, y) = ((col % d0) as u32, (col / d0) as u32);
            for z in z0..z0 + dd {
                grid.set(x, y, z, 0);
            }
        }
        let p = grid.center((c % d0) as u32, (c / d0) as u32, z0);
        let mut note = format!(
            "Stand hole of {diameter} mm, {:.1} mm deep, in the underside at x {:.1} y {:.1}",
            dd as f32 * grid.voxel,
            p[0] + 0.0,
            p[1] + 0.0
        );
        if dd < depth {
            note.push_str(&format!(" (not {depth_mm} mm: the part is thin there)"));
        }
        if dist[c] < r + wall {
            note.push_str(&format!(
                " (warning: under {PEG_WALL_MM} mm of wall around)"
            ));
        }
        note.push('.');
        notes.push(note);
        return;
    }
    notes.push(format!(
        "No room for a {diameter} mm stand hole in the underside."
    ));
}

// ---------------------------------------------------------------------------------------------
// Extracting

/// Padding (cells) around the grid while extracting.
const XPAD: u32 = 3;

/// The surface of the solid cells as a closed, consistently outward-oriented 2-manifold mesh
/// (mesh units), with `groups`/`labels` from the cells' groups (each triangle: its solid
/// cell's group). The groups' patches are open where groups meet; [`extract_where`] gives one
/// closed mesh per group.
pub fn extract(grid: &VoxelGrid, progress: Progress<'_>) -> AssetResult<Mesh> {
    extract_where(grid, &|c| c != 0, progress)
}

/// [`extract`] of the cells `keep(cell)` says only (e.g. `|c| c == 1 + group`: a closed mesh
/// of one group).
pub fn extract_where(
    grid: &VoxelGrid,
    keep: &(dyn Fn(u8) -> bool + Sync),
    progress: Progress<'_>,
) -> AssetResult<Mesh> {
    tick(progress, 0.0, "Building the surface")?;
    let pd = grid.dims.map(|d| d + 2 * XPAD);
    let mut p = Bits::new(pd);
    let (gd0, gd1) = (grid.dims[0] as usize, grid.dims[1] as usize);
    if gd0 > 0 {
        let (wpr, pd1) = (p.wpr, pd[1] as usize);
        p.data.par_chunks_mut(wpr).enumerate().for_each(|(row, o)| {
            let (y, z) = ((row % pd1) as u32, (row / pd1) as u32);
            if y < XPAD || z < XPAD || y >= grid.dims[1] + XPAD || z >= grid.dims[2] + XPAD {
                return;
            }
            let s = (y - XPAD) as usize * gd0 + (z - XPAD) as usize * gd0 * gd1;
            for (x, c) in grid.cells[s..s + gd0].iter().enumerate() {
                if keep(*c) {
                    let xx = x + XPAD as usize;
                    o[xx / 64] |= 1 << (xx % 64);
                }
            }
        });
    }
    // Well-composed: add cells until no critical edge or corner is left.
    let mut rounds = 0;
    loop {
        let f = criticals(&p);
        if !f.any() {
            break;
        }
        p.or_with(&f);
        rounds += 1;
        if rounds > 10_000 {
            return Err(AssetError::internal("could not close the solid's surface"));
        }
        tick(progress, 0.1, "Building the surface")?;
    }
    // Nothing may reach the outer layer (blocks past it are not checked).
    let edge = (0..pd[2]).any(|z| {
        (0..pd[1]).any(|y| {
            let border = z == 0 || y == 0 || z == pd[2] - 1 || y == pd[1] - 1;
            let row = p.row(y as i64, z as i64).unwrap_or(&[]);
            if border {
                row.iter().any(|w| *w != 0)
            } else {
                row.first().is_some_and(|w| w & 1 != 0)
                    || (p.dims[0] > 0 && {
                        let x = p.dims[0] - 1;
                        (row[x as usize / 64] >> (x % 64)) & 1 != 0
                    })
            }
        })
    });
    if edge {
        return Err(AssetError::internal("could not close the solid's surface"));
    }
    tick(progress, 0.3, "Building the surface")?;
    let faces = boundary_faces(&p);
    tick(progress, 0.5, "Building the surface")?;
    let labeled = !grid.labels.is_empty();
    build_mesh(grid, &faces, labeled, progress)
}

/// The cells to add so `p` (and its complement) has no critical configuration: two solid (or
/// two empty) cells meeting only at an edge (C1) or a corner (C2), the rest of the block the
/// other kind.
fn criticals(p: &Bits) -> Bits {
    let (wpr, d1) = (p.wpr, p.dims[1] as usize);
    let zero = vec![0u64; wpr];
    let rows: Vec<(usize, Vec<u64>, Vec<u64>)> = (0..p.rows())
        .into_par_iter()
        .filter_map(|row| {
            let (y, z) = ((row % d1) as i64, (row / d1) as i64);
            let ra = p.row(y, z)?;
            let rb = p.row(y + 1, z).unwrap_or(&zero);
            let rc = p.row(y, z + 1).unwrap_or(&zero);
            let rd = p.row(y + 1, z + 1).unwrap_or(&zero);
            let mut fa = vec![0u64; wpr];
            let mut fa1 = vec![0u64; wpr];
            let mut fb = vec![0u64; wpr];
            let mut fb1 = vec![0u64; wpr];
            let mut any = false;
            for i in 0..wpr {
                let (a, b, c, d) = (ra[i], rb[i], rc[i], rd[i]);
                let (a1, b1, c1, d1) = (
                    down(ra, 1, i),
                    down(rb, 1, i),
                    down(rc, 1, i),
                    down(rd, 1, i),
                );
                if (a | b | c | d | a1 | b1 | c1 | d1) == 0 {
                    continue;
                }
                let mut wa = 0u64;
                let mut wa1 = 0u64;
                let mut wb = 0u64;
                let mut wb1 = 0u64;
                // C1, edges along x, y, z.
                wb |= a & d & !b & !c;
                wa |= b & c & !a & !d;
                wa1 |= a & c1 & !a1 & !c;
                wa |= a1 & c & !a & !c1;
                wa1 |= a & b1 & !a1 & !b;
                wa |= a1 & b & !a & !b1;
                // C2: antipodal pairs of the 2x2x2 block.
                let cs = [a, a1, b, b1, c, c1, d, d1];
                let pairs = [(0, 7), (1, 6), (2, 5), (3, 4)];
                for (pi, (s, t)) in pairs.into_iter().enumerate() {
                    let mut or = 0u64;
                    let mut and = !0u64;
                    for (j, w) in cs.iter().enumerate() {
                        if j != s && j != t {
                            or |= w;
                            and &= w;
                        }
                    }
                    let solid_pair = cs[s] & cs[t] & !or;
                    let empty_pair = !cs[s] & !cs[t] & and;
                    match pi {
                        0 => wa1 |= solid_pair,
                        _ => wa |= solid_pair,
                    }
                    match s {
                        0 => wa |= empty_pair,
                        1 => wa1 |= empty_pair,
                        2 => wb |= empty_pair,
                        _ => wb1 |= empty_pair,
                    }
                }
                if (wa | wa1 | wb | wb1) != 0 {
                    any = true;
                    fa[i] |= wa;
                    fa1[i] |= wa1;
                    fb[i] |= wb;
                    fb1[i] |= wb1;
                }
            }
            if !any {
                return None;
            }
            for (i, (a, b)) in fa.iter_mut().zip(fb.iter_mut()).enumerate() {
                *a |= up(&fa1, 1, i);
                *b |= up(&fb1, 1, i);
            }
            Some((row, fa, fb))
        })
        .collect();
    let mut f = Bits::new(p.dims);
    for (row, fa, fb) in rows {
        let r = row * wpr;
        for (o, a) in f.data[r..r + wpr].iter_mut().zip(&fa) {
            *o |= a;
        }
        let (y, z) = (row % d1, row / d1);
        if y + 1 < d1 {
            let r = f.row_of(y as u32 + 1, z as u32);
            for (o, b) in f.data[r..r + wpr].iter_mut().zip(&fb) {
                *o |= b;
            }
        }
    }
    f.mask_tails();
    // Only add empty cells.
    f.and_not(p);
    f
}

/// One solid cell face against an empty cell: (x, y, z, direction 0..6 = +x -x +y -y +z -z).
type Face = [u32; 4];

fn boundary_faces(p: &Bits) -> Vec<Face> {
    let (wpr, d1) = (p.wpr, p.dims[1] as usize);
    let zero = vec![0u64; wpr];
    let per_row: Vec<Vec<Face>> = (0..p.rows())
        .into_par_iter()
        .map(|row| {
            let (y, z) = ((row % d1) as i64, (row / d1) as i64);
            let mut out = Vec::new();
            let Some(a) = p.row(y, z) else { return out };
            if a.iter().all(|w| *w == 0) {
                return out;
            }
            let n = [
                p.row(y + 1, z).unwrap_or(&zero),
                p.row(y - 1, z).unwrap_or(&zero),
                p.row(y, z + 1).unwrap_or(&zero),
                p.row(y, z - 1).unwrap_or(&zero),
            ];
            for i in 0..wpr {
                let m = [
                    a[i] & !down(a, 1, i),
                    a[i] & !up(a, 1, i),
                    a[i] & !n[0][i],
                    a[i] & !n[1][i],
                    a[i] & !n[2][i],
                    a[i] & !n[3][i],
                ];
                for (dir, mut w) in m.into_iter().enumerate() {
                    while w != 0 {
                        let b = w.trailing_zeros();
                        w &= w - 1;
                        out.push([(i * 64) as u32 + b, y as u32, z as u32, dir as u32]);
                    }
                }
            }
            out
        })
        .collect();
    per_row.into_iter().flatten().collect()
}

/// The corners of a face, counter-clockwise seen from outside (offsets from the cell).
const CORNERS: [[[u32; 3]; 4]; 6] = [
    [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]],
    [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]],
    [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
    [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]],
    [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]],
    [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]],
];

const NORMALS: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
];

fn build_mesh(
    grid: &VoxelGrid,
    faces: &[Face],
    labeled: bool,
    progress: Progress<'_>,
) -> AssetResult<Mesh> {
    let pd = grid.dims.map(|d| d + 2 * XPAD);
    let (kx, ky) = (pd[0] as u64 + 1, pd[1] as u64 + 1);
    let mut ids: HashMap<u64, u32> = HashMap::with_capacity(faces.len());
    let mut corner: Vec<[u32; 3]> = Vec::with_capacity(faces.len());
    let mut quads: Vec<[u32; 4]> = Vec::with_capacity(faces.len());
    for f in faces {
        let q = CORNERS[f[3] as usize].map(|o| {
            let c = [f[0] + o[0], f[1] + o[1], f[2] + o[2]];
            let key = c[0] as u64 + kx * (c[1] as u64 + ky * c[2] as u64);
            *ids.entry(key).or_insert_with(|| {
                corner.push(c);
                (corner.len() - 1) as u32
            })
        });
        quads.push(q);
    }
    drop(ids);
    tick(progress, 0.6, "Smoothing the surface")?;
    let (o, v) = (grid.origin, grid.voxel);
    let base: Vec<[f32; 3]> = corner
        .iter()
        .map(|c| [0, 1, 2].map(|i| o[i] + (c[i] as f32 - XPAD as f32) * v))
        .collect();
    let mut pos = smooth(&base, &quads, v);
    tick(progress, 0.8, "Smoothing the surface")?;
    // Triangulate (shorter diagonal); undo smoothing where a triangle would turn over.
    let tri_ok = |pos: &[[f32; 3]], t: [u32; 3], n: [f32; 3]| {
        let [a, b, c] = t.map(|i| pos[i as usize]);
        let cr = cross(sub3(b, a), sub3(c, a));
        dot(cr, n) > 1e-4 * v * v
    };
    let split = |pos: &[[f32; 3]], q: [u32; 4], n: [f32; 3]| -> Option<[[u32; 3]; 2]> {
        let d2 = |i: u32, j: u32| {
            let e = sub3(pos[i as usize], pos[j as usize]);
            dot(e, e)
        };
        let ac = [[q[0], q[1], q[2]], [q[0], q[2], q[3]]];
        let bd = [[q[0], q[1], q[3]], [q[1], q[2], q[3]]];
        let (first, second) = if d2(q[0], q[2]) <= d2(q[1], q[3]) {
            (ac, bd)
        } else {
            (bd, ac)
        };
        [first, second]
            .into_iter()
            .find(|ts| ts.iter().all(|t| tri_ok(pos, *t, n)))
    };
    let mut rounds = 0;
    loop {
        let bad: Vec<usize> = (0..quads.len())
            .into_par_iter()
            .filter(|&i| split(&pos, quads[i], NORMALS[faces[i][3] as usize]).is_none())
            .collect();
        if bad.is_empty() {
            break;
        }
        rounds += 1;
        for i in bad {
            for k in quads[i] {
                pos[k as usize] = base[k as usize];
            }
        }
        if rounds > 64 {
            pos.clone_from(&base);
        }
    }
    let mut triangles = Vec::with_capacity(quads.len() * 2);
    let mut groups = labeled.then(|| Vec::with_capacity(quads.len() * 2));
    for (i, q) in quads.iter().enumerate() {
        let n = NORMALS[faces[i][3] as usize];
        let ts = split(&pos, *q, n).unwrap_or([[q[0], q[1], q[2]], [q[0], q[2], q[3]]]);
        triangles.extend_from_slice(&ts);
        if let Some(g) = &mut groups {
            let f = faces[i];
            let c = [f[0], f[1], f[2]].map(|c| c.wrapping_sub(XPAD));
            let mut val = grid.get(c[0], c[1], c[2]);
            if val == 0 {
                val = neighbour_label(grid, c).unwrap_or(1);
            }
            let gi = val.saturating_sub(1) as u32;
            g.extend_from_slice(&[gi, gi]);
        }
    }
    tick(progress, 1.0, "Building the surface")?;
    Ok(Mesh {
        vertices: pos,
        triangles,
        groups,
        labels: if labeled {
            grid.labels.clone()
        } else {
            Vec::new()
        },
    })
}

/// Laplacian smoothing along the quads' edges, each vertex kept within [`SMOOTH_CLAMP`]
/// voxels of its corner.
fn smooth(base: &[[f32; 3]], quads: &[[u32; 4]], voxel: f32) -> Vec<[f32; 3]> {
    let n = base.len();
    let mut deg = vec![0u32; n + 1];
    for q in quads {
        for k in 0..4 {
            deg[q[k] as usize] += 1;
            deg[q[(k + 1) % 4] as usize] += 1;
        }
    }
    let mut start = vec![0usize; n + 1];
    for i in 0..n {
        start[i + 1] = start[i] + deg[i] as usize;
    }
    let mut fill = start.clone();
    let mut adj = vec![0u32; start[n]];
    for q in quads {
        for k in 0..4 {
            let (a, b) = (q[k], q[(k + 1) % 4]);
            adj[fill[a as usize]] = b;
            fill[a as usize] += 1;
            adj[fill[b as usize]] = a;
            fill[b as usize] += 1;
        }
    }
    // Each edge is listed twice per vertex (two quads); averaging over duplicates is the same.
    let clamp = SMOOTH_CLAMP * voxel;
    let mut pos = base.to_vec();
    for _ in 0..SMOOTH_ITERS {
        let next: Vec<[f32; 3]> = (0..n)
            .into_par_iter()
            .map(|i| {
                let nb = &adj[start[i]..start[i + 1]];
                if nb.is_empty() {
                    return pos[i];
                }
                let mut s = [0f32; 3];
                for &j in nb {
                    for k in 0..3 {
                        s[k] += pos[j as usize][k];
                    }
                }
                let m = nb.len() as f32;
                [0, 1, 2].map(|k| {
                    let avg = s[k] / m;
                    let p = pos[i][k] + 0.5 * (avg - pos[i][k]);
                    p.clamp(base[i][k] - clamp, base[i][k] + clamp)
                })
            })
            .collect();
        pos = next;
    }
    pos
}

#[cfg(test)]
#[path = "voxel_tests.rs"]
mod tests;
