//! Simplify (experimental): reduce a part's triangles to a budget, keeping its shape (and its
//! groups). Pipeline step 12 of [`super::apply`], which shares `simplify_tris` among the parts
//! by their triangle counts.
//!
//! Quadric error metric edge collapse (Garland and Heckbert), in pure Rust:
//!
//! - each vertex carries the area-weighted quadric of its faces' planes, plus a plane through
//!   every open edge and every edge between two groups (perpendicular to its face), so holes
//!   and colour borders keep their place;
//! - the cheapest edge collapses first, to the point that minimises the summed quadric (an
//!   endpoint or the midpoint when that point is ill-conditioned or far off the edge);
//! - a collapse is refused when it would break the surface: the edge's link condition (the
//!   vertices both ends share are exactly the edge's opposite corners), an inner edge joining
//!   two boundary vertices, a face that would flip or fold (its normal turning by more than
//!   [`MAX_TURN_COS`]) or become a sliver, two faces that would coincide, or a small open
//!   piece (a lone triangle, a strip) that would be erased;
//! - vertices on non-manifold edges (more than two faces) never move, so a soup of overlapping
//!   GLB meshes is simplified only where it is a proper surface.
//!
//! A closed (watertight, consistently wound) mesh stays closed. The result has about `target`
//! triangles, never more than it had; fewer collapses happen when the checks refuse the rest.

use super::{Mesh, Progress};
use crate::assets::{AssetError, AssetResult};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// A collapse is refused when a remaining face's normal would turn by more than about 78°.
const MAX_TURN_COS: f64 = 0.2;
/// Open edges and group borders weigh this much more than a face of the same area.
const BORDER_WEIGHT: f64 = 10.0;
/// How many collapses between progress reports (and cancel checks).
const REPORT_EVERY: usize = 2048;
/// Passes over the remaining edges, for collapses refused earlier that later become possible.
const MAX_PASSES: usize = 4;

/// Reduce `mesh` to about `target` triangles (never more than it has), keeping it closed where
/// it was, its winding and its triangles' groups. A mesh at or under `target` is left as is.
pub fn simplify(mesh: &mut Mesh, target: u32, progress: Progress<'_>) -> AssetResult<()> {
    let target = target.max(4) as usize;
    if mesh.triangles.len() <= target {
        return Ok(());
    }
    let mut s = State::new(mesh);
    let start = s.alive_count;
    for _ in 0..MAX_PASSES {
        let before = s.alive_count;
        s.fill_heap();
        s.run(target, start, progress)?;
        if s.alive_count <= target || s.alive_count == before {
            break;
        }
    }
    s.write_back(mesh);
    Ok(())
}

/// A symmetric 4x4 quadric: `a2 ab ac ad b2 bc bd c2 cd d2`.
#[derive(Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    /// `w` times the squared distance to the plane `n·p + d = 0` (`n` unit length).
    fn plane(n: [f64; 3], d: f64, w: f64) -> Quadric {
        let [a, b, c] = n;
        Quadric(
            [
                a * a,
                a * b,
                a * c,
                a * d,
                b * b,
                b * c,
                b * d,
                c * c,
                c * d,
                d * d,
            ]
            .map(|x| x * w),
        )
    }

    fn add(&mut self, o: &Quadric) {
        for (a, b) in self.0.iter_mut().zip(o.0) {
            *a += b;
        }
    }

    fn sum(a: &Quadric, b: &Quadric) -> Quadric {
        let mut q = *a;
        q.add(b);
        q
    }

    fn eval(&self, p: [f64; 3]) -> f64 {
        let q = &self.0;
        let [x, y, z] = p;
        q[0] * x * x
            + 2.0 * q[1] * x * y
            + 2.0 * q[2] * x * z
            + 2.0 * q[3] * x
            + q[4] * y * y
            + 2.0 * q[5] * y * z
            + 2.0 * q[6] * y
            + q[7] * z * z
            + 2.0 * q[8] * z
            + q[9]
    }

    /// The point minimising the quadric, when the 3x3 system is well conditioned.
    fn optimum(&self) -> Option<[f64; 3]> {
        let q = &self.0;
        let (a, b, c, e, f, i) = (q[0], q[1], q[2], q[4], q[5], q[7]);
        let r = [-q[3], -q[6], -q[8]];
        let det = a * (e * i - f * f) - b * (b * i - f * c) + c * (b * f - e * c);
        let scale = (a.abs() + e.abs() + i.abs()).powi(3);
        if !det.is_finite() || det.abs() <= 1e-10 * scale || scale == 0.0 {
            return None;
        }
        let x =
            (r[0] * (e * i - f * f) - b * (r[1] * i - f * r[2]) + c * (r[1] * f - e * r[2])) / det;
        let y =
            (a * (r[1] * i - f * r[2]) - r[0] * (b * i - f * c) + c * (b * r[2] - r[1] * c)) / det;
        let z =
            (a * (e * r[2] - r[1] * f) - b * (b * r[2] - r[1] * c) + r[0] * (b * f - e * c)) / det;
        let p = [x, y, z];
        p.iter().all(|v| v.is_finite()).then_some(p)
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn normal(p: [[f64; 3]; 3]) -> [f64; 3] {
    cross(sub(p[1], p[0]), sub(p[2], p[0]))
}

/// A candidate collapse in the heap: cheapest first.
struct Cand {
    cost: f64,
    /// The vertex kept, the vertex removed, and where the kept one moves.
    keep: u32,
    gone: u32,
    to: [f64; 3],
    /// Both vertices' versions when this was computed (stale when either changed).
    ver: [u32; 2],
}

impl PartialEq for Cand {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Cand {}
impl PartialOrd for Cand {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cand {
    fn cmp(&self, o: &Self) -> Ordering {
        // Reversed: BinaryHeap is a max-heap.
        o.cost.total_cmp(&self.cost)
    }
}

struct State {
    pos: Vec<[f64; 3]>,
    quad: Vec<Quadric>,
    tris: Vec<[u32; 3]>,
    alive: Vec<bool>,
    alive_count: usize,
    /// Per vertex, the triangles that use it (dead ones are pruned on the way).
    vt: Vec<Vec<u32>>,
    /// On a non-manifold edge: never moves.
    locked: Vec<bool>,
    /// On an open edge.
    boundary: Vec<bool>,
    removed: Vec<bool>,
    version: Vec<u32>,
    heap: BinaryHeap<Cand>,
}

impl State {
    fn new(mesh: &Mesh) -> State {
        let nv = mesh.vertices.len();
        let pos: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.map(f64::from)).collect();
        let mut quad = vec![Quadric::default(); nv];
        let mut vt: Vec<Vec<u32>> = vec![Vec::new(); nv];
        // Face planes.
        for (t, tri) in mesh.triangles.iter().enumerate() {
            let p = tri.map(|i| pos[i as usize]);
            let n = normal(p);
            let l = len(n);
            if l > 0.0 {
                let n = n.map(|c| c / l);
                let q = Quadric::plane(n, -dot(n, p[0]), l * 0.5);
                for &i in tri {
                    quad[i as usize].add(&q);
                }
            }
            for &i in tri {
                vt[i as usize].push(t as u32);
            }
        }
        // Edges: open, non-manifold, group borders.
        let mut edges: HashMap<(u32, u32), Vec<u32>> =
            HashMap::with_capacity(mesh.triangles.len() * 2);
        for (t, tri) in mesh.triangles.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                edges
                    .entry((a.min(b), a.max(b)))
                    .or_default()
                    .push(t as u32);
            }
        }
        let mut locked = vec![false; nv];
        let mut boundary = vec![false; nv];
        let border = |quad: &mut [Quadric], a: u32, b: u32, t: u32| {
            let p = mesh.triangles[t as usize].map(|i| pos[i as usize]);
            let (pa, pb) = (pos[a as usize], pos[b as usize]);
            let e = sub(pb, pa);
            let n = cross(e, normal(p));
            let l = len(n);
            if l > 0.0 {
                let n = n.map(|c| c / l);
                let q = Quadric::plane(n, -dot(n, pa), BORDER_WEIGHT * dot(e, e));
                quad[a as usize].add(&q);
                quad[b as usize].add(&q);
            }
        };
        for (&(a, b), ts) in &edges {
            match ts.len() {
                1 => {
                    boundary[a as usize] = true;
                    boundary[b as usize] = true;
                    border(&mut quad, a, b, ts[0]);
                }
                2 => {
                    if mesh.group(ts[0] as usize) != mesh.group(ts[1] as usize) {
                        border(&mut quad, a, b, ts[0]);
                        border(&mut quad, a, b, ts[1]);
                    }
                }
                _ => {
                    locked[a as usize] = true;
                    locked[b as usize] = true;
                }
            }
        }
        State {
            pos,
            quad,
            alive: vec![true; mesh.triangles.len()],
            alive_count: mesh.triangles.len(),
            tris: mesh.triangles.clone(),
            vt,
            locked,
            boundary,
            removed: vec![false; nv],
            version: vec![0; nv],
            heap: BinaryHeap::new(),
        }
    }

    /// Every remaining edge, once.
    fn fill_heap(&mut self) {
        self.heap.clear();
        let mut seen: HashSet<(u32, u32)> = HashSet::with_capacity(self.alive_count * 2);
        for t in 0..self.tris.len() {
            if !self.alive[t] {
                continue;
            }
            let tri = self.tris[t];
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                if seen.insert((a.min(b), a.max(b))) {
                    self.push(a, b);
                }
            }
        }
    }

    fn push(&mut self, a: u32, b: u32) {
        let (ai, bi) = (a as usize, b as usize);
        if self.locked[ai] && self.locked[bi] {
            return;
        }
        let q = Quadric::sum(&self.quad[ai], &self.quad[bi]);
        let (pa, pb) = (self.pos[ai], self.pos[bi]);
        let (keep, gone, to) = if self.locked[ai] || (self.boundary[ai] && !self.boundary[bi]) {
            (a, b, pa)
        } else if self.locked[bi] || (self.boundary[bi] && !self.boundary[ai]) {
            (b, a, pb)
        } else {
            let mid = [
                (pa[0] + pb[0]) * 0.5,
                (pa[1] + pb[1]) * 0.5,
                (pa[2] + pb[2]) * 0.5,
            ];
            let el = len(sub(pb, pa));
            let mut best = (q.eval(mid), mid);
            for p in [pa, pb] {
                let c = q.eval(p);
                if c < best.0 {
                    best = (c, p);
                }
            }
            if let Some(p) = q.optimum()
                && len(sub(p, mid)) <= el
            {
                let c = q.eval(p);
                if c < best.0 {
                    best = (c, p);
                }
            }
            (a, b, best.1)
        };
        let cost = q.eval(to).max(0.0);
        self.heap.push(Cand {
            cost,
            keep,
            gone,
            to,
            ver: [self.version[keep as usize], self.version[gone as usize]],
        });
    }

    fn prune(&mut self, v: u32) {
        let alive = &self.alive;
        self.vt[v as usize].retain(|&t| alive[t as usize]);
    }

    fn run(&mut self, target: usize, start: usize, progress: Progress<'_>) -> AssetResult<()> {
        let mut done = 0usize;
        let need = (start - target).max(1) as f32;
        while self.alive_count > target {
            let Some(c) = self.heap.pop() else { break };
            let (k, g) = (c.keep as usize, c.gone as usize);
            if self.removed[k] || self.removed[g] || c.ver != [self.version[k], self.version[g]] {
                continue;
            }
            if !self.collapse(c.keep, c.gone, c.to) {
                continue;
            }
            done += 1;
            if done.is_multiple_of(REPORT_EVERY) {
                let f = (start - self.alive_count) as f32 / need;
                if !progress(f, "Simplifying") {
                    return Err(AssetError::cancelled());
                }
            }
        }
        Ok(())
    }

    /// The other vertices of `v`'s live triangles.
    fn ring(&self, v: u32) -> Vec<u32> {
        let mut r: Vec<u32> = self.vt[v as usize]
            .iter()
            .flat_map(|&t| self.tris[t as usize])
            .filter(|&x| x != v)
            .collect();
        r.sort_unstable();
        r.dedup();
        r
    }

    /// Collapse `gone` into `keep`, moved to `to`, when that keeps the surface sound.
    fn collapse(&mut self, keep: u32, gone: u32, to: [f64; 3]) -> bool {
        self.prune(keep);
        self.prune(gone);
        let (ki, gi) = (keep as usize, gone as usize);
        // The triangles on the edge, and their opposite corners.
        let shared: Vec<u32> = self.vt[gi]
            .iter()
            .copied()
            .filter(|&t| self.tris[t as usize].contains(&keep))
            .collect();
        if shared.is_empty() || shared.len() > 2 {
            return false;
        }
        // An inner edge between two boundary vertices would pinch the surface.
        if shared.len() == 2 && self.boundary[ki] && self.boundary[gi] {
            return false;
        }
        let mut opposite: Vec<u32> = shared
            .iter()
            .flat_map(|&t| self.tris[t as usize])
            .filter(|&x| x != keep && x != gone)
            .collect();
        opposite.sort_unstable();
        opposite.dedup();
        if opposite.len() != shared.len() {
            return false;
        }
        // Link condition.
        let (rk, rg) = (self.ring(keep), self.ring(gone));
        let common = rk.iter().filter(|x| rg.binary_search(x).is_ok()).count();
        if common != opposite.len() {
            return false;
        }
        // Never erase a small open piece: the kept vertex and each opposite corner keep a face.
        let left = |v: u32| {
            self.vt[v as usize]
                .iter()
                .any(|&t| self.alive[t as usize] && !shared.contains(&t))
        };
        if !(left(keep) || left(gone)) || !opposite.iter().all(|&o| left(o)) {
            return false;
        }
        // No fold, flip or sliver among the faces that move; no two faces becoming one.
        let mut seen: Vec<[u32; 3]> = Vec::new();
        for &v in &[keep, gone] {
            for &t in &self.vt[v as usize] {
                let tri = self.tris[t as usize];
                if tri.contains(&keep) && tri.contains(&gone) {
                    continue;
                }
                let old = tri.map(|i| self.pos[i as usize]);
                let new_tri = tri.map(|i| if i == gone { keep } else { i });
                let new = new_tri.map(|i| if i == keep { to } else { self.pos[i as usize] });
                let (no, nn) = (normal(old), normal(new));
                let (lo, ln) = (len(no), len(nn));
                if lo > 0.0 {
                    if ln <= 1e-12 * lo || dot(no, nn) < MAX_TURN_COS * lo * ln {
                        return false;
                    }
                    // A sliver: the new face's area tiny against its longest edge squared.
                    let e = (0..3)
                        .map(|k| dot(sub(new[k], new[(k + 1) % 3]), sub(new[k], new[(k + 1) % 3])))
                        .fold(0.0, f64::max);
                    if ln < 1e-3 * e {
                        return false;
                    }
                }
                let mut key = new_tri;
                key.sort_unstable();
                if seen.contains(&key) {
                    return false;
                }
                seen.push(key);
            }
        }
        // Apply.
        for &t in &shared {
            self.alive[t as usize] = false;
            self.alive_count -= 1;
        }
        let moved: Vec<u32> = self.vt[gi].clone();
        for t in moved {
            if !self.alive[t as usize] {
                continue;
            }
            for i in &mut self.tris[t as usize] {
                if *i == gone {
                    *i = keep;
                }
            }
            self.vt[ki].push(t);
        }
        self.vt[gi].clear();
        self.prune(keep);
        self.pos[ki] = to;
        let q = self.quad[gi];
        self.quad[ki].add(&q);
        self.boundary[ki] |= self.boundary[gi];
        self.locked[ki] |= self.locked[gi];
        self.removed[gi] = true;
        self.version[ki] += 1;
        self.version[gi] += 1;
        for n in self.ring(keep) {
            self.push(keep, n);
        }
        true
    }

    fn write_back(self, mesh: &mut Mesh) {
        mesh.vertices = self.pos.iter().map(|p| p.map(|c| c as f32)).collect();
        mesh.triangles = self.tris;
        let alive = self.alive;
        mesh.retain_triangles(|t| alive[t]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn all() -> impl Fn(f32, &str) -> bool + Sync {
        |_, _| true
    }

    /// An icosphere of radius `r`, `n` subdivisions (20 * 4^n triangles), outward winding.
    fn sphere(r: f32, n: u32) -> Mesh {
        let t = (1.0 + 5f32.sqrt()) / 2.0;
        let mut v: Vec<[f32; 3]> = vec![
            [-1.0, t, 0.0],
            [1.0, t, 0.0],
            [-1.0, -t, 0.0],
            [1.0, -t, 0.0],
            [0.0, -1.0, t],
            [0.0, 1.0, t],
            [0.0, -1.0, -t],
            [0.0, 1.0, -t],
            [t, 0.0, -1.0],
            [t, 0.0, 1.0],
            [-t, 0.0, -1.0],
            [-t, 0.0, 1.0],
        ];
        let mut f: Vec<[u32; 3]> = vec![
            [0, 11, 5],
            [0, 5, 1],
            [0, 1, 7],
            [0, 7, 10],
            [0, 10, 11],
            [1, 5, 9],
            [5, 11, 4],
            [11, 10, 2],
            [10, 7, 6],
            [7, 1, 8],
            [3, 9, 4],
            [3, 4, 2],
            [3, 2, 6],
            [3, 6, 8],
            [3, 8, 9],
            [4, 9, 5],
            [2, 4, 11],
            [6, 2, 10],
            [8, 6, 7],
            [9, 8, 1],
        ];
        let norm = |p: [f32; 3]| {
            let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            p.map(|c| c / l * r)
        };
        v = v.into_iter().map(norm).collect();
        for _ in 0..n {
            let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
            let mut nf = Vec::new();
            for tri in &f {
                let mut m = [0u32; 3];
                for k in 0..3 {
                    let (a, b) = (tri[k], tri[(k + 1) % 3]);
                    m[k] = *mid.entry((a.min(b), a.max(b))).or_insert_with(|| {
                        let (pa, pb) = (v[a as usize], v[b as usize]);
                        v.push(norm([
                            (pa[0] + pb[0]) / 2.0,
                            (pa[1] + pb[1]) / 2.0,
                            (pa[2] + pb[2]) / 2.0,
                        ]));
                        (v.len() - 1) as u32
                    });
                }
                nf.push([tri[0], m[0], m[2]]);
                nf.push([tri[1], m[1], m[0]]);
                nf.push([tri[2], m[2], m[1]]);
                nf.push([m[0], m[1], m[2]]);
            }
            f = nf;
        }
        Mesh {
            vertices: v,
            triangles: f,
            groups: None,
            labels: Vec::new(),
        }
    }

    /// A cube from -1 to 1, each face an `n` x `n` grid (12 n² triangles), outward winding.
    fn grid_cube(n: usize) -> Mesh {
        let mut tris = Vec::new();
        let s = |i: usize| -1.0 + 2.0 * i as f32 / n as f32;
        for axis in 0..3 {
            for side in [-1.0f32, 1.0] {
                let (u, w) = ((axis + 1) % 3, (axis + 2) % 3);
                let p = |i: usize, j: usize| {
                    let mut p = [0.0; 3];
                    p[axis] = side;
                    p[u] = s(i);
                    p[w] = s(j);
                    p
                };
                for i in 0..n {
                    for j in 0..n {
                        let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
                        if side > 0.0 {
                            tris.push([a, b, c]);
                            tris.push([a, c, d]);
                        } else {
                            tris.push([a, c, b]);
                            tris.push([a, d, c]);
                        }
                    }
                }
            }
        }
        Mesh::from_triangles(&tris)
    }

    /// Every directed edge appears once and its reverse once: closed and consistently wound.
    fn closed(m: &Mesh) -> bool {
        let mut e: HashMap<(u32, u32), u32> = HashMap::new();
        for t in &m.triangles {
            for k in 0..3 {
                *e.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        e.iter()
            .all(|(&(a, b), &n)| n == 1 && e.get(&(b, a)) == Some(&1))
    }

    /// Signed volume (positive for outward winding).
    fn volume(m: &Mesh) -> f64 {
        m.triangles
            .iter()
            .map(|t| {
                let p = t.map(|i| m.vertices[i as usize].map(f64::from));
                dot(p[0], cross(p[1], p[2])) / 6.0
            })
            .sum()
    }

    #[test]
    fn a_sphere_stays_closed_round_and_outward() {
        let mut m = sphere(10.0, 4);
        assert_eq!(m.triangles.len(), 5120);
        assert!(closed(&m));
        let v0 = volume(&m);
        simplify(&mut m, 1000, &all()).unwrap();
        let n = m.triangles.len();
        assert!((900..=1000).contains(&n), "{n} triangles");
        assert!(closed(&m), "not closed after simplify");
        for p in &m.vertices {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 10.0).abs() < 0.3, "vertex off the sphere: r = {r}");
        }
        let v1 = volume(&m);
        assert!(v1 > 0.0 && (v1 - v0).abs() / v0 < 0.03, "{v0} -> {v1}");
        // Euler characteristic of a sphere.
        let e = n * 3 / 2;
        assert_eq!(m.vertices.len() as i64 - e as i64 + n as i64, 2);
    }

    #[test]
    fn a_flat_faced_cube_keeps_its_shape_exactly() {
        let mut m = grid_cube(10);
        assert_eq!(m.triangles.len(), 1200);
        simplify(&mut m, 100, &all()).unwrap();
        assert!(m.triangles.len() <= 100, "{}", m.triangles.len());
        assert!(closed(&m));
        let b = m.bounds().unwrap();
        for k in 0..3 {
            assert!((b.min[k] + 1.0).abs() < 1e-4 && (b.max[k] - 1.0).abs() < 1e-4);
        }
        for p in &m.vertices {
            let on = p.iter().map(|c| c.abs()).fold(0.0f32, f32::max);
            assert!((on - 1.0).abs() < 1e-4, "vertex off the cube: {p:?}");
        }
        assert!((volume(&m) - 8.0).abs() < 1e-3);
    }

    #[test]
    fn a_mesh_at_or_under_the_budget_is_unchanged() {
        let m0 = sphere(1.0, 2);
        let mut m = m0.clone();
        simplify(&mut m, 320, &all()).unwrap();
        assert_eq!(m, m0);
        simplify(&mut m, 100_000, &all()).unwrap();
        assert_eq!(m, m0);
    }

    #[test]
    fn groups_follow_their_triangles_and_an_open_edge_stays_put() {
        // A sphere's lower half: an open rim at z = 0, two groups split by x.
        let s = sphere(5.0, 4);
        let keep: Vec<usize> = (0..s.triangles.len())
            .filter(|&t| s.triangle(t).iter().all(|p| p[2] <= 1e-4))
            .collect();
        let mut m = s.subset(keep);
        m.compact();
        let groups: Vec<u32> = (0..m.triangles.len())
            .map(|t| u32::from(m.triangle(t)[0][0] > 0.0))
            .collect();
        m.groups = Some(groups);
        m.labels = vec!["a".into(), "b".into()];
        let rim = |m: &Mesh| m.vertices.iter().filter(|p| p[2].abs() < 1e-4).count();
        let n0 = m.triangles.len();
        simplify(&mut m, (n0 / 4) as u32, &all()).unwrap();
        assert!(
            m.triangles.len() <= n0 / 4 + n0 / 20,
            "{}",
            m.triangles.len()
        );
        assert_eq!(m.groups.as_ref().unwrap().len(), m.triangles.len());
        assert!(m.groups.as_ref().unwrap().contains(&0));
        assert!(m.groups.as_ref().unwrap().contains(&1));
        assert!(rim(&m) >= 3);
        // The rim stays on z = 0 (no vertex pulled above it).
        assert!(m.vertices.iter().all(|p| p[2] <= 1e-3));
    }

    #[test]
    fn small_open_pieces_are_never_erased() {
        // A lone triangle and a quad beside a sphere that has to shrink a lot.
        let mut m = sphere(1.0, 4);
        let lone = Mesh::from_triangles(&[[[5.0, 0.0, 0.0], [6.0, 0.0, 0.0], [5.0, 1.0, 0.0]]]);
        let quad = Mesh::from_triangles(&[
            [[8.0, 0.0, 0.0], [9.0, 0.0, 0.0], [9.0, 1.0, 0.0]],
            [[8.0, 0.0, 0.0], [9.0, 1.0, 0.0], [8.0, 1.0, 0.0]],
        ]);
        m.append(&lone);
        m.append(&quad);
        simplify(&mut m, 100, &all()).unwrap();
        let near = |x: f32| (0..m.triangles.len()).any(|t| m.triangle(t)[0][0] >= x - 0.01);
        assert!(near(8.0), "quad erased");
        let lone_left = (0..m.triangles.len())
            .any(|t| m.triangle(t).iter().all(|p| (5.0..=6.0).contains(&p[0])));
        assert!(lone_left, "lone triangle erased");
    }

    #[test]
    fn non_manifold_edges_never_move_and_cancel_is_honoured() {
        // Two spheres sharing nothing but a fin: a third triangle on one edge.
        let mut m = sphere(1.0, 3);
        let t0 = m.triangles[0];
        m.vertices.push([3.0, 3.0, 3.0]);
        let x = (m.vertices.len() - 1) as u32;
        m.triangles.push([t0[0], t0[1], x]);
        let fixed = [t0[0], t0[1]].map(|i| m.vertices[i as usize]);
        simplify(&mut m, 200, &all()).unwrap();
        for p in fixed {
            assert!(m.vertices.contains(&p));
        }
        let mut m = sphere(1.0, 5);
        let e = simplify(&mut m, 1000, &|_, _| false).unwrap_err();
        assert_eq!(e, AssetError::cancelled());
    }
}
