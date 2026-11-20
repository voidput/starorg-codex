//! Split (experimental): cut the voxel solid into pieces that each fit the bed. Pipeline
//! steps 9 (the cuts, planned before the pegs) and 10 of [`super::apply`].
//!
//! Owner: the split agent.
//!
//! - [`plan`]: the fewest axis-aligned planes (whole-grid cuts, so the pieces are the cells of
//!   a lattice) that make every piece fit the bed. Each axis gets the fewest cuts its extent
//!   needs; where the cuts go is searched (positions through thin sections, and slid from the
//!   low end to the high end), every combination when there are few, else axis by axis in
//!   every axis order, keeping the plan with the fewest non-empty pieces, then the fewest
//!   slivers (under 2 % of the solid), then the least cut area.
//! - [`pins`]: alignment pin holes ([`PIN_MM`], [`PIN_DEPTH_MM`] into each side) across each
//!   cut, when no `--peg` is asked for (the pegs then do this job).
//! - [`split`]: the pieces, each cropped to its cells, named `part1`, `part2`, ... and laid out
//!   side by side along x ([`LAYOUT_GAP`] apart), so the 3MF's separate build items do not
//!   overlap.

use super::Cut;
use super::voxel::{PEG_CAP_MM, PEG_WALL_MM, VoxelGrid};
use rayon::prelude::*;
use std::collections::HashMap;

/// Alignment pin holes across a cut: diameter, millimetres.
pub const PIN_MM: f32 = 2.0;
/// How deep a pin hole goes into each side of a cut, millimetres.
pub const PIN_DEPTH_MM: f32 = 4.0;
/// The gap between pieces laid out side by side, mesh units (millimetres: a solid is always
/// scaled).
pub const LAYOUT_GAP: f32 = 5.0;
/// Pin holes per cut face: two in its largest piece, one in each of the next ones.
const MAX_FACE_PIECES: usize = 3;

/// One piece of the split solid.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// `part1`, `part2`, ... (empty for an unsplit solid).
    pub name: String,
    pub grid: VoxelGrid,
}

/// The cell planes (cell index `k`: cells below `k` go to the low side) of `cuts` along each
/// axis, sorted, inside the grid.
fn planes(grid: &VoxelGrid, cuts: &[Cut]) -> [Vec<u32>; 3] {
    let mut p: [Vec<u32>; 3] = Default::default();
    for c in cuts {
        if let Some(k) = plane(grid, c) {
            p[c.axis.min(2)].push(k);
        }
    }
    for v in &mut p {
        v.sort_unstable();
        v.dedup();
    }
    p
}

/// The cell plane of `cut` (the same rounding as [`super::voxel::peg`]), `None` outside.
fn plane(grid: &VoxelGrid, cut: &Cut) -> Option<u32> {
    let a = cut.axis.min(2);
    let k = ((cut.at - grid.origin[a]) / grid.voxel - 0.5).ceil();
    (k >= 1.0 && k < grid.dims[a] as f32).then_some(k as u32)
}

/// Solid cells meeting across each plane `k` (cells `k - 1` and `k`) of each axis.
fn cut_areas(grid: &VoxelGrid) -> [Vec<u64>; 3] {
    let [d0, d1, d2] = grid.dims.map(|d| d as usize);
    let layer = d0 * d1;
    let cells = grid.cells();
    let zero = || [vec![0u64; d0 + 1], vec![0u64; d1 + 1], vec![0u64; d2 + 1]];
    if layer == 0 || d2 == 0 {
        return zero();
    }
    (0..d2)
        .into_par_iter()
        .fold(zero, |mut acc, z| {
            let l = &cells[layer * z..layer * (z + 1)];
            let below = (z > 0).then(|| &cells[layer * (z - 1)..layer * z]);
            for y in 0..d1 {
                let row = &l[d0 * y..d0 * (y + 1)];
                let prev = (y > 0).then(|| &l[d0 * (y - 1)..d0 * y]);
                for x in 0..d0 {
                    if row[x] == 0 {
                        continue;
                    }
                    if x > 0 && row[x - 1] != 0 {
                        acc[0][x] += 1;
                    }
                    if let Some(p) = prev
                        && p[x] != 0
                    {
                        acc[1][y] += 1;
                    }
                    if let Some(b) = below
                        && b[d0 * y + x] != 0
                    {
                        acc[2][z] += 1;
                    }
                }
            }
            acc
        })
        .reduce(zero, |mut a, b| {
            for i in 0..3 {
                for (x, y) in a[i].iter_mut().zip(&b[i]) {
                    *x += y;
                }
            }
            a
        })
}

/// The solid runs (`x0..x1`) of every row `(y, z)`, for counting the pieces of a plan quickly.
struct Runs {
    /// `(y, z, first run, end run)`.
    rows: Vec<(u32, u32, u32, u32)>,
    runs: Vec<(u32, u32)>,
}

impl Runs {
    fn new(grid: &VoxelGrid) -> Runs {
        let [d0, d1, _] = grid.dims.map(|d| d as usize);
        let mut rows = Vec::new();
        let mut runs = Vec::new();
        if d0 == 0 {
            return Runs { rows, runs };
        }
        let per_row: Vec<(usize, Vec<(u32, u32)>)> = grid
            .cells()
            .par_chunks(d0)
            .enumerate()
            .filter_map(|(r, row)| {
                let mut v = Vec::new();
                let mut x = 0;
                while x < d0 {
                    if row[x] == 0 {
                        x += 1;
                        continue;
                    }
                    let s = x;
                    while x < d0 && row[x] != 0 {
                        x += 1;
                    }
                    v.push((s as u32, x as u32));
                }
                (!v.is_empty()).then_some((r, v))
            })
            .collect();
        for (r, v) in per_row {
            let s = runs.len() as u32;
            runs.extend(v);
            rows.push(((r % d1) as u32, (r / d1) as u32, s, runs.len() as u32));
        }
        Runs { rows, runs }
    }

    /// Solid cells in each lattice cell cut by `planes`.
    fn pieces(&self, planes: &[Vec<u32>; 3]) -> Vec<u64> {
        let n = [0, 1, 2].map(|a| planes[a].len() + 1);
        let slab = |a: usize, c: u32| planes[a].partition_point(|k| *k <= c);
        let mut cells = vec![0u64; n[0] * n[1] * n[2]];
        for &(y, z, s, e) in &self.rows {
            let base = n[0] * (slab(1, y) + n[1] * slab(2, z));
            for &(x0, x1) in &self.runs[s as usize..e as usize] {
                let mut x = x0;
                for sx in slab(0, x0)..=slab(0, x1 - 1) {
                    let end = planes[0].get(sx).copied().unwrap_or(x1).min(x1);
                    cells[base + sx] += (end - x) as u64;
                    x = end;
                }
            }
        }
        cells
    }
}

/// Plane positions (`lo < k < hi`, sorted) giving `n` slabs of at most `cap` cells of
/// `lo..hi` worth trying: the least total cut area, and the slabs packed from the low end
/// shifting to the high end.
fn candidates(lo: u32, hi: u32, n: u32, cap: u32, area: &[u64]) -> Vec<Vec<u32>> {
    if n <= 1 {
        return vec![Vec::new()];
    }
    let valid = |b: &[u32]| {
        let mut prev = lo;
        for &k in b.iter().chain(std::iter::once(&hi)) {
            if k <= prev || k - prev > cap {
                return false;
            }
            prev = k;
        }
        true
    };
    let mut out = Vec::new();
    if let Some(b) = least_area(lo, hi, n, cap, area) {
        out.push(b);
    }
    let low: Vec<u32> = (1..n).map(|i| lo + i * cap).collect();
    let high: Vec<u32> = (1..n).map(|i| hi - (n - i) * cap).collect();
    const STEPS: u32 = 32;
    for t in 0..=STEPS {
        let b: Vec<u32> = low
            .iter()
            .zip(&high)
            .map(|(&l, &h)| {
                (l as f64 + (h as f64 - l as f64) * t as f64 / STEPS as f64).round() as u32
            })
            .collect();
        if valid(&b) && !out.contains(&b) {
            out.push(b);
        }
    }
    out
}

/// The plane positions of the least total cut area (`area[k]`) for `n` slabs of at most `cap`
/// cells of `lo..hi`; `None` when the search would be too long (absurdly many slabs).
fn least_area(lo: u32, hi: u32, n: u32, cap: u32, area: &[u64]) -> Option<Vec<u32>> {
    let (lo, hi, n, cap) = (lo as usize, hi as usize, n as usize, cap as usize);
    let ext = hi - lo;
    if (n as u64) * (ext as u64) * (cap as u64) > 200_000_000 {
        return None;
    }
    let at = |k: usize| area.get(k).copied().unwrap_or(0);
    // cost[i][k - lo]: the least area of i + 1 planes, the last at k; from[i][k - lo]: the
    // plane before it.
    let mut cost = vec![vec![u64::MAX; ext + 1]; n - 1];
    let mut from = vec![vec![0usize; ext + 1]; n - 1];
    for k in lo + 1..=(lo + cap).min(hi - 1) {
        cost[0][k - lo] = at(k);
    }
    for i in 1..n - 1 {
        for k in lo + 1..hi {
            let mut best = (u64::MAX, 0);
            for j in k.saturating_sub(cap).max(lo + 1)..k {
                let c = cost[i - 1][j - lo];
                if c < best.0 {
                    best = (c, j);
                }
            }
            if best.0 != u64::MAX {
                cost[i][k - lo] = best.0 + at(k);
                from[i][k - lo] = best.1;
            }
        }
    }
    let last = (hi.saturating_sub(cap).max(lo + 1)..hi)
        .filter(|k| cost[n - 2][k - lo] != u64::MAX)
        .min_by_key(|k| (cost[n - 2][k - lo], k.abs_diff((lo + hi) / 2)))?;
    let mut b = vec![last];
    let mut k = last;
    for i in (1..n - 1).rev() {
        k = from[i][k - lo];
        b.push(k);
    }
    b.reverse();
    Some(b.into_iter().map(|k| k as u32).collect())
}

/// (pieces, slivers, cut area): lower is better.
type Score = (u64, u64, u64);
/// A piece under 1 / SLIVER of the solid counts as a sliver.
const SLIVER: u64 = 50;
/// Up to this many plans, every one is scored; else axis by axis in each order.
const EXHAUSTIVE: usize = 4096;

/// The axis orders tried (when there are too many plans to score them all).
const ORDERS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

/// The cuts that make every piece fit `bed` (mesh units, x/y/z, as the pieces lie: they are
/// not turned): on each axis the fewest planes its extent needs, placed (searching each axis
/// order) for the fewest non-empty pieces, then the fewest slivers, then the least cut area
/// (thin sections). None when
/// the solid fits already.
pub fn plan(grid: &VoxelGrid, bed: [f32; 3]) -> Vec<Cut> {
    let Some((lo, hi)) = grid.solid_bounds() else {
        return Vec::new();
    };
    let v = grid.voxel;
    // Half a voxel to spare, against rounding (the surface never leaves its cells' box).
    let cap = bed.map(|b| {
        let c = (b / v - 0.5).floor();
        if c.is_finite() && c >= 1.0 {
            c.min(u32::MAX as f32) as u32
        } else {
            1
        }
    });
    let n: [u32; 3] = [0, 1, 2].map(|a| (hi[a] - lo[a]).div_ceil(cap[a]).max(1));
    if n == [1, 1, 1] {
        return Vec::new();
    }
    let area = cut_areas(grid);
    let cands: [Vec<Vec<u32>>; 3] =
        [0, 1, 2].map(|a| candidates(lo[a], hi[a], n[a], cap[a], &area[a]));
    let runs = Runs::new(grid);
    let total = grid.count_solid().max(1);
    let mut seen: HashMap<[usize; 3], Score> = HashMap::new();
    // Fewest pieces, then fewest slivers, then the least cut area.
    let mut score = |c: [usize; 3]| -> Score {
        *seen.entry(c).or_insert_with(|| {
            let p = [0, 1, 2].map(|a| cands[a][c[a]].clone());
            let a: u64 = (0..3)
                .map(|ax| p[ax].iter().map(|k| area[ax][*k as usize]).sum::<u64>())
                .sum();
            let cells = runs.pieces(&p);
            let pieces = cells.iter().filter(|n| **n > 0).count() as u64;
            let slivers = cells
                .iter()
                .filter(|n| **n > 0 && **n * SLIVER < total)
                .count() as u64;
            (pieces, slivers, a)
        })
    };
    let mut best = ([0usize; 3], score([0; 3]));
    let combos: usize = cands.iter().map(|c| c.len()).product();
    if combos <= EXHAUSTIVE {
        for i in 0..cands[0].len() {
            for j in 0..cands[1].len() {
                for k in 0..cands[2].len() {
                    let s = score([i, j, k]);
                    if s < best.1 {
                        best = ([i, j, k], s);
                    }
                }
            }
        }
    }
    for order in ORDERS.iter().filter(|_| combos > EXHAUSTIVE) {
        let mut c = [0usize; 3];
        let mut s = score(c);
        loop {
            let mut better = false;
            for &a in order {
                for i in 0..cands[a].len() {
                    let mut t = c;
                    t[a] = i;
                    let st = score(t);
                    if st < s {
                        (c, s, better) = (t, st, true);
                    }
                }
            }
            if !better {
                break;
            }
        }
        if s < best.1 {
            best = (c, s);
        }
    }
    let mut cuts = Vec::new();
    for a in 0..3 {
        for &k in &cands[a][best.0[a]] {
            cuts.push(Cut {
                axis: a,
                at: grid.origin[a] + k as f32 * v,
            });
        }
    }
    cuts
}

/// `grid` cut at `cuts` into pieces (each cropped to its solid cells; empty ones left out),
/// named `part1`, `part2`, ... (in order of x, then y, then z) and laid out side by side
/// along x, [`LAYOUT_GAP`] apart, their low y lined up (z is left for sitting on the bed).
/// Without cuts (or a single piece): the whole grid, unnamed and in place.
pub fn split(grid: VoxelGrid, cuts: &[Cut]) -> Vec<Piece> {
    let p = planes(&grid, cuts);
    if p.iter().all(|v| v.is_empty()) {
        return vec![Piece {
            name: String::new(),
            grid,
        }];
    }
    let edges = [0, 1, 2].map(|a| {
        let mut e = vec![0u32];
        e.extend(&p[a]);
        e.push(grid.dims[a]);
        e
    });
    let mut grids = Vec::new();
    for sx in edges[0].windows(2) {
        for sy in edges[1].windows(2) {
            for sz in edges[2].windows(2) {
                let (min, max) = ([sx[0], sy[0], sz[0]], [sx[1], sy[1], sz[1]]);
                let Ok(cell) = grid.crop(min, max) else {
                    continue;
                };
                let Some((a, b)) = cell.solid_bounds() else {
                    continue;
                };
                if let Ok(g) = cell.crop(a, b) {
                    grids.push(g);
                }
            }
        }
    }
    drop(grid);
    if grids.len() == 1 {
        return vec![Piece {
            name: String::new(),
            grid: grids.remove(0),
        }];
    }
    let low_y = grids
        .iter()
        .map(|g| g.origin[1])
        .fold(f32::INFINITY, f32::min);
    let mut x = grids
        .iter()
        .map(|g| g.origin[0])
        .fold(f32::INFINITY, f32::min);
    grids
        .into_iter()
        .enumerate()
        .map(|(i, mut g)| {
            g.origin[0] = x;
            g.origin[1] = low_y;
            x += g.dims[0] as f32 * g.voxel + LAYOUT_GAP;
            Piece {
                name: format!("part{}", i + 1),
                grid: g,
            }
        })
        .collect()
}

/// A note on the split: how many parts and where the cuts are, or that none was needed.
pub fn summary(cuts: &[Cut], pieces: &[Piece]) -> String {
    if cuts.is_empty() || pieces.len() < 2 {
        return "Split: the model fits the bed, no cut needed.".into();
    }
    let at: Vec<String> = cuts
        .iter()
        .map(|c| format!("{} {:.1}", ["x", "y", "z"][c.axis.min(2)], c.at + 0.0))
        .collect();
    format!(
        "Split into {} parts to fit the bed (cut{} at {}); each is its own file and build item.",
        pieces.len(),
        if cuts.len() == 1 { "" } else { "s" },
        at.join(", ")
    )
}

/// Distance (cells, 8-neighbour chamfer) from each `mask` cell to the nearest cell outside it
/// (the border counts as outside); 0 outside.
fn chamfer(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    const D: f32 = std::f32::consts::SQRT_2;
    let (pw, ph) = (w + 2, h + 2);
    let mut d = vec![0f32; pw * ph];
    for y in 0..h {
        for x in 0..w {
            if mask[x + w * y] {
                d[(x + 1) + pw * (y + 1)] = f32::INFINITY;
            }
        }
    }
    for y in 1..ph - 1 {
        for x in 1..pw - 1 {
            let i = x + pw * y;
            let m = d[i]
                .min(d[i - 1] + 1.0)
                .min(d[i - pw] + 1.0)
                .min(d[i - pw - 1] + D)
                .min(d[i - pw + 1] + D);
            d[i] = m;
        }
    }
    for y in (1..ph - 1).rev() {
        for x in (1..pw - 1).rev() {
            let i = x + pw * y;
            let m = d[i]
                .min(d[i + 1] + 1.0)
                .min(d[i + pw] + 1.0)
                .min(d[i + pw + 1] + D)
                .min(d[i + pw - 1] + D);
            d[i] = m;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            out[x + w * y] = d[(x + 1) + pw * (y + 1)];
        }
    }
    out
}

/// 4-connected pieces of `mask`, largest first.
fn face_pieces(mask: &[bool], w: usize, h: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    for s in 0..w * h {
        if !mask[s] || seen[s] {
            continue;
        }
        seen[s] = true;
        let mut stack = vec![s];
        let mut cells = Vec::new();
        while let Some(c) = stack.pop() {
            cells.push(c);
            let (x, y) = (c % w, c / w);
            let mut near = [None; 4];
            if x > 0 {
                near[0] = Some(c - 1);
            }
            if x + 1 < w {
                near[1] = Some(c + 1);
            }
            if y > 0 {
                near[2] = Some(c - w);
            }
            if y + 1 < h {
                near[3] = Some(c + w);
            }
            for q in near.into_iter().flatten() {
                if mask[q] && !seen[q] {
                    seen[q] = true;
                    stack.push(q);
                }
            }
        }
        out.push(cells);
    }
    out.sort_by_key(|p| std::cmp::Reverse(p.len()));
    out
}

/// The cell at plane index `k` of axis `a`, (u, v) along the other two.
fn cell3(a: usize, k: u32, u: u32, v: u32) -> [u32; 3] {
    let mut c = [0u32; 3];
    c[a] = k;
    c[(a + 1) % 3] = u;
    c[(a + 2) % 3] = v;
    c
}

/// Alignment pin holes of `diameter` (mesh units), `depth` deep into each side, across every
/// cut: two in the largest piece of the cut face (as far apart as there is room for, so the
/// parts cannot turn), one in each of the next pieces. A hole goes only where the face has
/// room for it, and never through (at least [`PEG_CAP_MM`] left). One note per cut.
pub fn pins(
    grid: &mut VoxelGrid,
    cuts: &[Cut],
    diameter: f32,
    depth: f32,
    notes: &mut Vec<String>,
) {
    if !(diameter > 0.0 && depth > 0.0) {
        return;
    }
    let v = grid.voxel;
    let r = 0.5 * diameter / v;
    let wall = PEG_WALL_MM / v;
    let cap = (PEG_CAP_MM / v).ceil().max(1.0) as u32;
    let depth_cells = (depth / v).round().max(1.0) as u32;
    let disc: Vec<(i64, i64)> = {
        let ri = r.ceil() as i64;
        let mut d = Vec::new();
        for dv in -ri..=ri {
            for du in -ri..=ri {
                let (fu, fv) = (du as f32, dv as f32);
                if fu * fu + fv * fv <= r * r {
                    d.push((du, dv));
                }
            }
        }
        if d.is_empty() {
            d.push((0, 0));
        }
        d
    };
    for (n, cut) in cuts.iter().enumerate() {
        let a = cut.axis.min(2);
        let (ua, va) = ((a + 1) % 3, (a + 2) % 3);
        let names = ["x", "y", "z"];
        let head = format!("Cut {} ({} {:.1})", n + 1, names[a], cut.at + 0.0);
        let Some(k) = plane(grid, cut) else {
            continue;
        };
        let (w, h) = (grid.dims[ua] as usize, grid.dims[va] as usize);
        let mut mask = vec![false; w * h];
        for vv in 0..h {
            for uu in 0..w {
                let lo = cell3(a, k - 1, uu as u32, vv as u32);
                let hi = cell3(a, k, uu as u32, vv as u32);
                mask[uu + w * vv] =
                    grid.is_solid(lo[0], lo[1], lo[2]) && grid.is_solid(hi[0], hi[1], hi[2]);
            }
        }
        let dist = chamfer(&mask, w, h);
        let room = r + 1.0;
        let mut spots = Vec::new();
        for (i, piece) in face_pieces(&mask, w, h)
            .iter()
            .take(MAX_FACE_PIECES)
            .enumerate()
        {
            let Some(best) = piece
                .iter()
                .copied()
                .filter(|p| dist[*p] >= room)
                .max_by(|p, q| dist[*p].total_cmp(&dist[*q]))
            else {
                continue;
            };
            spots.push(best);
            if i > 0 {
                continue;
            }
            // The second as far from the first as the face allows, a good wall preferred.
            let (bx, by) = ((best % w) as f32, (best / w) as f32);
            let apart = |p: usize| {
                let (dx, dy) = ((p % w) as f32 - bx, (p / w) as f32 - by);
                dx * dx + dy * dy
            };
            let min_apart = (6.0 * r).max(4.0);
            let far = |need: f32| {
                piece
                    .iter()
                    .copied()
                    .filter(|p| dist[*p] >= need && apart(*p) >= min_apart * min_apart)
                    .max_by(|p, q| apart(*p).total_cmp(&apart(*q)))
            };
            if let Some(s) = far(r + wall.max(1.0)).or_else(|| far(room)) {
                spots.push(s);
            }
        }
        let mut placed = Vec::new();
        let mut clamped = false;
        for s in spots {
            let (u0, v0) = ((s % w) as i64, (s / w) as i64);
            let cells: Vec<(u32, u32)> = disc
                .iter()
                .filter_map(|&(du, dv)| {
                    let (uu, vv) = (u0 + du, v0 + dv);
                    (uu >= 0 && vv >= 0 && (uu as usize) < w && (vv as usize) < h)
                        .then_some((uu as u32, vv as u32))
                })
                .collect();
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
                            let c = cell3(a, kk as u32, uu, vv);
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
                let dd = depth_cells.min(run(grid, up).saturating_sub(cap));
                clamped |= dd < depth_cells;
                if dd == 0 {
                    continue;
                }
                made = true;
                for &(uu, vv) in &cells {
                    for i in 0..dd {
                        let kk = if up { k + i } else { k - 1 - i };
                        let c = cell3(a, kk, uu, vv);
                        grid.set(c[0], c[1], c[2], 0);
                    }
                }
            }
            if made {
                let c = grid.center(
                    cell3(a, k, u0 as u32, v0 as u32)[0],
                    cell3(a, k, u0 as u32, v0 as u32)[1],
                    cell3(a, k, u0 as u32, v0 as u32)[2],
                );
                placed.push(format!(
                    "{} {:.1} {} {:.1}",
                    names[ua],
                    c[ua] + 0.0,
                    names[va],
                    c[va] + 0.0
                ));
            }
        }
        if placed.is_empty() {
            notes.push(format!(
                "{head}: no room for a {diameter} mm alignment pin hole."
            ));
            continue;
        }
        notes.push(format!(
            "{head}: {} alignment pin hole{} of {diameter} mm, {depth} mm into each side{}, at {} \
             (a {diameter} mm pin or a piece of filament lines the parts up).",
            placed.len(),
            if placed.len() == 1 { "" } else { "s" },
            if clamped {
                " (shallower where the part is thin)"
            } else {
                ""
            },
            placed.join(", ")
        ));
    }
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod tests;
