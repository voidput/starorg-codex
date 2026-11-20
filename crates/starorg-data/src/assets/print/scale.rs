//! Print size (experimental): the scale mode, sitting on the bed, the bed check (or shrinking
//! to the bed) and leaving out tiny parts. Pipeline steps 2-5 and 13 of [`super::apply`].
//!
//! Owner: the scale agent.

use super::{Aabb, Mesh, PrintOptions, ScaleMode};
use crate::assets::AssetResult;
use crate::assets::threemf::Unit;
use std::collections::{HashMap, HashSet};

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

/// Move `meshes` together so the lowest point of all of them is at z = 0 and the middle of
/// their combined footprint at x = y = 0, the middle of the bed (always applied).
pub fn sit_on_bed(meshes: &mut [&mut Mesh]) {
    let Some(b) = meshes
        .iter()
        .filter_map(|m| m.bounds())
        .reduce(|a, b| Aabb {
            min: [0, 1, 2].map(|i| a.min[i].min(b.min[i])),
            max: [0, 1, 2].map(|i| a.max[i].max(b.max[i])),
        })
    else {
        return;
    };
    let d = [
        (b.min[0] + b.max[0]) / 2.0,
        (b.min[1] + b.max[1]) / 2.0,
        b.min[2],
    ];
    if d.iter().all(|c| c.is_finite()) && d != [0.0; 3] {
        for m in meshes.iter_mut() {
            m.map_vertices(|[x, y, z]| [x - d[0], y - d[1], z - d[2]]);
        }
    }
}

/// Corners closer than this fraction of `min_size` count as one when finding loose parts.
const WELD_FRACTION: f32 = 1e-3;

/// Leave out loose parts whose bounding-box diagonal is under `min_size` (mesh units; the
/// pipeline converts `drop_small_mm` at print size). A loose part is a set of triangles joined
/// by shared corners, after welding corners closer than [`WELD_FRACTION`] of `min_size` (GLB
/// nodes often repeat the corners along a seam). A small part inside or touching a larger one
/// without sharing a corner is still loose. Returns how many parts were left out.
pub fn drop_tiny(mesh: &mut Mesh, min_size: f32) -> usize {
    if !(min_size.is_finite() && min_size > 0.0) || mesh.is_empty() {
        return 0;
    }
    let mut sets = Sets::new(mesh.vertices.len());
    weld(&mesh.vertices, min_size * WELD_FRACTION, &mut sets);
    for t in &mesh.triangles {
        sets.join(t[0], t[1]);
        sets.join(t[0], t[2]);
    }
    let roots: Vec<u32> = mesh.triangles.iter().map(|t| sets.root(t[0])).collect();
    // The bounds of each part, by its root vertex.
    let mut parts: HashMap<u32, Aabb> = HashMap::new();
    for (t, root) in mesh.triangles.iter().zip(&roots) {
        for &i in t {
            let p = mesh.vertices[i as usize];
            let b = parts.entry(*root).or_insert(Aabb { min: p, max: p });
            for (k, c) in p.iter().enumerate() {
                b.min[k] = b.min[k].min(*c);
                b.max[k] = b.max[k].max(*c);
            }
        }
    }
    let small: HashSet<u32> = parts
        .iter()
        .filter(|(_, b)| diagonal(b) < min_size)
        .map(|(r, _)| *r)
        .collect();
    if !small.is_empty() {
        mesh.retain_triangles(|t| !small.contains(&roots[t]));
    }
    small.len()
}

fn diagonal(b: &Aabb) -> f32 {
    b.size().iter().map(|s| s * s).sum::<f32>().sqrt()
}

/// Join every pair of vertices closer than `eps` (a hash grid of `eps` cells, each vertex
/// compared with those in the 27 cells around its own).
fn weld(vertices: &[[f32; 3]], eps: f32, sets: &mut Sets) {
    if !(eps.is_finite() && eps > 0.0) {
        return;
    }
    let cell = |p: [f32; 3]| p.map(|c| (c / eps).floor() as i64);
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::with_capacity(vertices.len());
    let eps2 = eps * eps;
    for (i, &p) in vertices.iter().enumerate() {
        let c = cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(near) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else {
                        continue;
                    };
                    for &j in near {
                        let q = vertices[j as usize];
                        let d2: f32 = (0..3).map(|k| (p[k] - q[k]).powi(2)).sum();
                        if d2 <= eps2 {
                            sets.join(i as u32, j);
                        }
                    }
                }
            }
        }
        grid.entry(c).or_default().push(i as u32);
    }
}

/// Union-find over vertex indices.
struct Sets(Vec<u32>);

impl Sets {
    fn new(n: usize) -> Self {
        Self((0..n as u32).collect())
    }

    fn root(&mut self, mut i: u32) -> u32 {
        while self.0[i as usize] != i {
            let up = self.0[self.0[i as usize] as usize];
            self.0[i as usize] = up;
            i = up;
        }
        i
    }

    fn join(&mut self, a: u32, b: u32) {
        let (a, b) = (self.root(a), self.root(b));
        if a != b {
            self.0[a.max(b) as usize] = a.min(b);
        }
    }
}

/// Room left for `f32` rounding when comparing with the bed, millimetres.
const BED_SLACK_MM: f32 = 1e-3;

/// Compare the mesh (in `unit`, sitting on the bed) with `o.bed_mm`; nothing at real size
/// (metres: no print size was chosen, a ship never fits). When it fits only turned
/// 90 degrees about Z, turn it. When it fits neither way: with `o.shrink_to_bed`, shrink it
/// (turned when that leaves it larger) so it does; otherwise add a note that it does not fit
/// (with `o.split`, the split cuts it into pieces that do). Each change is noted.
pub fn bed_check(
    mesh: &mut Mesh,
    o: &PrintOptions,
    unit: Unit,
    notes: &mut Vec<String>,
) -> AssetResult<()> {
    let Some(b) = mesh.bounds().filter(|_| unit == Unit::Millimeter) else {
        return Ok(());
    };
    let s = b.size();
    let bed = o.bed_mm;
    let fits = |x: f32, y: f32| {
        x <= bed[0] + BED_SLACK_MM && y <= bed[1] + BED_SLACK_MM && s[2] <= bed[2] + BED_SLACK_MM
    };
    if fits(s[0], s[1]) {
        return Ok(());
    }
    let bed_txt = size(bed);
    if fits(s[1], s[0]) {
        turn(mesh);
        notes.push(format!(
            "Turned 90 degrees about Z to fit the {bed_txt} bed (experimental)."
        ));
        return Ok(());
    }
    if !o.shrink_to_bed {
        let then = if o.split {
            "the split cuts it into pieces that fit"
        } else {
            "pick a smaller scale, shrink to the bed or split it"
        };
        notes.push(format!(
            "At {} the model is larger than the {bed_txt} bed: {then} (experimental).",
            size(s)
        ));
        return Ok(());
    }
    let room = |x: f32, y: f32| {
        ratio(bed[0], x)
            .min(ratio(bed[1], y))
            .min(ratio(bed[2], s[2]))
    };
    let (k0, k90) = (room(s[0], s[1]), room(s[1], s[0]));
    let turned = k90 > k0;
    let k = k0.max(k90);
    if turned {
        turn(mesh);
    }
    scale(mesh, k);
    let now = if turned { [s[1], s[0], s[2]] } else { s }.map(|v| v * k);
    notes.push(format!(
        "Shrunk to {}% to fit the {bed_txt} bed{}, now {} (experimental).",
        pct(k),
        if turned {
            ", turned 90 degrees about Z"
        } else {
            ""
        },
        size(now),
    ));
    Ok(())
}

/// How much `have` must be scaled to fit in `room` (at most 1; 1 when `have` is 0).
fn ratio(room: f32, have: f32) -> f32 {
    if have > 0.0 {
        (room / have).min(1.0)
    } else {
        1.0
    }
}

/// Turn 90 degrees about Z (counter-clockwise from above: x, y -> -y, x).
fn turn(mesh: &mut Mesh) {
    mesh.map_vertices(|[x, y, z]| [-y, x, z]);
}

/// Millimetres to one decimal, without a trailing `.0`.
fn mm(v: f32) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r:.1}")
    }
}

/// A size in millimetres as text.
fn size(s: [f32; 3]) -> String {
    format!("{} x {} x {} mm", mm(s[0]), mm(s[1]), mm(s[2]))
}

fn pct(k: f32) -> String {
    let p = k * 100.0;
    if p >= 10.0 {
        format!("{p:.0}")
    } else {
        format!("{p:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::print::apply;

    /// A closed box from `min` to `max`, 12 triangles.
    fn cube(min: [f32; 3], max: [f32; 3]) -> Mesh {
        let c = |i: usize| {
            [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ]
        };
        let faces = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let mut tris = Vec::new();
        for [a, b, cc, d] in faces {
            tris.push([c(a), c(b), c(cc)]);
            tris.push([c(a), c(cc), c(d)]);
        }
        Mesh::from_triangles(&tris)
    }

    fn joined(parts: &[Mesh]) -> Mesh {
        let mut m = Mesh::default();
        for p in parts {
            m.append(p);
        }
        m
    }

    fn opts(bed: [f32; 3], shrink: bool, split: bool) -> PrintOptions {
        PrintOptions {
            bed_mm: bed,
            shrink_to_bed: shrink,
            split,
            ..Default::default()
        }
    }

    fn all() -> impl Fn(f32, &str) -> bool + Sync {
        |_, _| true
    }

    #[test]
    fn sitting_on_the_bed_centres_x_y_and_puts_the_bottom_at_zero() {
        let mut a = cube([2.0, 3.0, -1.5], [4.0, 5.0, 0.5]);
        let mut b = cube([10.0, 3.0, 1.0], [12.0, 9.0, 2.0]);
        sit_on_bed(&mut [&mut a, &mut b]);
        let (a, b) = (a.bounds().unwrap(), b.bounds().unwrap());
        assert_eq!((a.min, a.max), ([-5.0, -3.0, 0.0], [-3.0, -1.0, 2.0]));
        assert_eq!((b.min, b.max), ([3.0, -3.0, 2.5], [5.0, 3.0, 3.5]));
        // No triangles: nothing to move.
        sit_on_bed(&mut [&mut Mesh::default()]);
    }

    #[test]
    fn tiny_loose_parts_are_left_out_by_their_diagonal() {
        let mut m = joined(&[
            cube([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]),
            // Diagonal 0.866: left out at 1.0.
            cube([20.0, 0.0, 0.0], [20.5, 0.5, 0.5]),
            // Diagonal 1.039 (0.6 a side): kept.
            cube([30.0, 0.0, 0.0], [30.6, 0.6, 0.6]),
            // Inside the big box but loose: a part of its own, left out.
            cube([5.0, 5.0, 5.0], [5.1, 5.1, 5.1]),
        ]);
        assert_eq!(m.triangles.len(), 48);
        assert_eq!(drop_tiny(&mut m, 1.0), 2);
        assert_eq!(m.triangles.len(), 24);
        assert_eq!(m.vertices.len(), 16);
        let b = m.bounds().unwrap();
        assert_eq!((b.min, b.max), ([0.0, 0.0, 0.0], [30.6, 10.0, 10.0]));
        // Nothing smaller left; a bad size does nothing.
        assert_eq!(drop_tiny(&mut m, 1.0), 0);
        assert_eq!(drop_tiny(&mut m, 0.0), 0);
        assert_eq!(drop_tiny(&mut m, f32::NAN), 0);
        assert_eq!(m.triangles.len(), 24);
    }

    #[test]
    fn corners_repeated_along_a_seam_are_welded_into_one_part() {
        // Two halves of a 0.8 bar meeting at a seam, their corners off by a hair: one part
        // (diagonal 1.15, kept), not two (0.82 each, both left out).
        let mut m = joined(&[
            cube([0.0, 0.0, 0.0], [0.4, 0.6, 0.6]),
            cube([0.400_000_6, 0.0, 0.0], [0.8, 0.6, 0.6]),
        ]);
        assert_eq!(drop_tiny(&mut m, 1.0), 0);
        assert_eq!(m.triangles.len(), 24);
        // A gap: two parts.
        let mut m = joined(&[
            cube([0.0, 0.0, 0.0], [0.4, 0.6, 0.6]),
            cube([0.41, 0.0, 0.0], [0.8, 0.6, 0.6]),
        ]);
        assert_eq!(drop_tiny(&mut m, 1.0), 2);
        assert!(m.is_empty());
    }

    #[test]
    fn groups_stay_with_the_triangles_kept() {
        let mut big = cube([0.0; 3], [10.0; 3]);
        big.groups = Some(vec![0; 12]);
        big.labels = vec!["hull".into()];
        let mut small = cube([20.0; 3], [20.1; 3]);
        small.groups = Some(vec![0; 12]);
        small.labels = vec!["glass".into()];
        let mut m = joined(&[big, small]);
        assert_eq!(drop_tiny(&mut m, 1.0), 1);
        assert_eq!(m.groups.as_ref().map(Vec::len), Some(12));
        assert!((0..12).all(|t| m.label(m.group(t)) == "hull"));
    }

    #[test]
    fn a_model_that_fits_is_left_alone() {
        let mut m = cube([-100.0, -50.0, 0.0], [100.0, 50.0, 80.0]);
        let before = m.clone();
        let mut notes = Vec::new();
        bed_check(
            &mut m,
            &opts([256.0; 3], true, false),
            Unit::Millimeter,
            &mut notes,
        )
        .unwrap();
        assert_eq!(m, before);
        // Exactly the bed fits too.
        let mut m = cube([-128.0, -128.0, 0.0], [128.0, 128.0, 256.0]);
        bed_check(
            &mut m,
            &opts([256.0; 3], false, false),
            Unit::Millimeter,
            &mut notes,
        )
        .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        // Real size (metres) is not checked.
        let mut m = cube([0.0; 3], [0.1, 0.2, 0.25]);
        bed_check(
            &mut m,
            &opts([256.0; 3], false, false),
            Unit::Meter,
            &mut notes,
        )
        .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_model_that_fits_turned_is_turned() {
        // 150 long in y on a 200 x 100 bed.
        let mut m = cube([-40.0, -75.0, 0.0], [40.0, 75.0, 50.0]);
        let mut notes = Vec::new();
        let o = opts([200.0, 100.0, 100.0], false, false);
        bed_check(&mut m, &o, Unit::Millimeter, &mut notes).unwrap();
        let b = m.bounds().unwrap();
        assert_eq!((b.min, b.max), ([-75.0, -40.0, 0.0], [75.0, 40.0, 50.0]));
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("Turned 90 degrees"), "{notes:?}");
        assert!(notes[0].contains("200 x 100 x 100 mm"), "{notes:?}");
    }

    #[test]
    fn a_model_too_large_gets_a_note_or_is_shrunk_to_fit() {
        let big = || cube([-200.0, -50.0, 0.0], [200.0, 50.0, 60.0]);
        let bed = [256.0; 3];
        let mut m = big();
        let mut notes = Vec::new();
        bed_check(
            &mut m,
            &opts(bed, false, false),
            Unit::Millimeter,
            &mut notes,
        )
        .unwrap();
        assert_eq!(m, big());
        assert!(notes[0].contains("At 400 x 100 x 60 mm"), "{notes:?}");
        assert!(notes[0].contains("shrink to the bed"), "{notes:?}");
        notes.clear();
        bed_check(
            &mut m,
            &opts(bed, false, true),
            Unit::Millimeter,
            &mut notes,
        )
        .unwrap();
        assert!(notes[0].contains("the split cuts it"), "{notes:?}");
        // Real size (metres, no print size chosen): not checked, not shrunk.
        let ship = || cube([-10.0, -4.0, 0.0], [10.0, 4.0, 5.0]);
        let mut m20 = ship();
        notes.clear();
        bed_check(&mut m20, &opts(bed, true, false), Unit::Meter, &mut notes).unwrap();
        assert_eq!(m20, ship());
        assert!(notes.is_empty(), "{notes:?}");
        // Shrunk: 400 -> 256 (64%).
        notes.clear();
        bed_check(
            &mut m,
            &opts(bed, true, false),
            Unit::Millimeter,
            &mut notes,
        )
        .unwrap();
        let s = m.bounds().unwrap().size();
        assert!(
            (s[0] - 256.0).abs() < 1e-3 && (s[1] - 64.0).abs() < 1e-3,
            "{s:?}"
        );
        assert!(notes[0].contains("Shrunk to 64%"), "{notes:?}");
        assert!(notes[0].contains("now 256 x 64 x 38.4 mm"), "{notes:?}");
        assert!(!notes[0].contains("turned"), "{notes:?}");
    }

    #[test]
    fn shrinking_turns_the_model_when_that_keeps_it_larger() {
        // 300 x 150 on a 100 x 200 bed: 1/3 as it is, 2/3 turned.
        let mut m = cube([-150.0, -75.0, 0.0], [150.0, 75.0, 10.0]);
        let mut notes = Vec::new();
        let o = opts([100.0, 200.0, 100.0], true, false);
        bed_check(&mut m, &o, Unit::Millimeter, &mut notes).unwrap();
        let s = m.bounds().unwrap().size();
        assert!(
            (s[0] - 100.0).abs() < 1e-3 && (s[1] - 200.0).abs() < 1e-3,
            "{s:?}"
        );
        assert!(notes[0].contains("Shrunk to 67%"), "{notes:?}");
        assert!(notes[0].contains("turned 90 degrees"), "{notes:?}");
        // Too tall: z sets the shrink.
        let mut m = cube([-10.0, -10.0, 0.0], [10.0, 10.0, 500.0]);
        notes.clear();
        bed_check(
            &mut m,
            &opts([256.0; 3], true, false),
            Unit::Millimeter,
            &mut notes,
        )
        .unwrap();
        assert!((m.bounds().unwrap().size()[2] - 256.0).abs() < 1e-3);
    }

    #[test]
    fn the_pipeline_drops_tiny_parts_at_print_size_and_shrinks_to_the_bed() {
        // A 20 m hull and a loose 0.1 m box 30 m out: at fit 150 mm over 30.1 m (k 4.98) the
        // box is 0.86 mm across.
        let m = joined(&[
            cube([0.0, 0.0, 0.0], [20.0, 8.0, 5.0]),
            cube([30.0, 0.0, 0.0], [30.1, 0.1, 0.1]),
        ]);
        let o = PrintOptions {
            scale: ScaleMode::FitMm(150.0),
            drop_small_mm: Some(0.8),
            ..Default::default()
        };
        let out = apply(m.clone(), &o, &all()).unwrap();
        assert_eq!(out.triangles(), 24, "{:?}", out.notes);
        let o = PrintOptions {
            drop_small_mm: Some(1.0),
            ..o
        };
        let out = apply(m.clone(), &o, &all()).unwrap();
        assert_eq!(out.triangles(), 12);
        assert!(
            out.notes.iter().any(|n| n.contains("Left out 1 parts")),
            "{:?}",
            out.notes
        );
        // The fit is the longest side of what is left (not 30.1 m with the box), centred.
        let b = out.parts[0].mesh.bounds().unwrap();
        assert!((b.size()[0] - 150.0).abs() < 1e-3, "{b:?}");
        assert!((b.min[0] + b.max[0]).abs() < 1e-3 && (b.min[1] + b.max[1]).abs() < 1e-3);
        assert_eq!(b.min[2], 0.0);
        // Real size in mm, shrunk to the bed.
        let o = PrintOptions {
            shrink_to_bed: true,
            ..Default::default()
        };
        let out = apply(cube([0.0; 3], [20.0, 8.0, 5.0]), &o, &all()).unwrap();
        assert_eq!(out.unit, Unit::Millimeter);
        let b = out.parts[0].mesh.bounds().unwrap();
        assert!((b.size()[0] - 256.0).abs() < 1e-2, "{b:?}");
        assert!(
            out.notes.iter().any(|n| n.contains("Shrunk to 1.28%")),
            "{:?}",
            out.notes
        );
    }
}
