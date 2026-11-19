//! The voxel solid: closed, outward, manifold surfaces from messy input; thicken, hollow and
//! pegs keep them so.

use super::*;
use std::collections::HashMap;

fn all() -> impl Fn(f32, &str) -> bool + Sync {
    |_, _| true
}

/// Two triangles per quad `a b c d` (counter-clockwise seen from outside).
fn quad(t: &mut Vec<[[f32; 3]; 3]>, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
    t.push([a, b, c]);
    t.push([a, c, d]);
}

/// A box from `lo` to `hi` as six separate quads, each `grow` larger than its face (so no
/// edge is shared: an unwelded, overlapping soup).
fn loose_box(t: &mut Vec<[[f32; 3]; 3]>, lo: [f32; 3], hi: [f32; 3], grow: f32) {
    let (l, h) = (lo.map(|v| v - grow), hi.map(|v| v + grow));
    // -z, +z, -y, +y, -x, +x
    quad(
        t,
        [l[0], l[1], lo[2]],
        [l[0], h[1], lo[2]],
        [h[0], h[1], lo[2]],
        [h[0], l[1], lo[2]],
    );
    quad(
        t,
        [l[0], l[1], hi[2]],
        [h[0], l[1], hi[2]],
        [h[0], h[1], hi[2]],
        [l[0], h[1], hi[2]],
    );
    quad(
        t,
        [l[0], lo[1], l[2]],
        [h[0], lo[1], l[2]],
        [h[0], lo[1], h[2]],
        [l[0], lo[1], h[2]],
    );
    quad(
        t,
        [l[0], hi[1], l[2]],
        [l[0], hi[1], h[2]],
        [h[0], hi[1], h[2]],
        [h[0], hi[1], l[2]],
    );
    quad(
        t,
        [lo[0], l[1], l[2]],
        [lo[0], l[1], h[2]],
        [lo[0], h[1], h[2]],
        [lo[0], h[1], l[2]],
    );
    quad(
        t,
        [hi[0], l[1], l[2]],
        [hi[0], h[1], l[2]],
        [hi[0], h[1], h[2]],
        [hi[0], l[1], h[2]],
    );
}

/// Every directed edge once and its reverse once: closed, 2-manifold at edges, consistently
/// oriented. Also no degenerate triangle.
fn assert_closed(m: &Mesh) {
    assert!(!m.triangles.is_empty());
    let mut e: HashMap<(u32, u32), u32> = HashMap::new();
    for t in &m.triangles {
        assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2], "{t:?}");
        for k in 0..3 {
            *e.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    for (&(a, b), &n) in &e {
        assert_eq!(n, 1, "edge {a}-{b} used {n} times the same way");
        assert_eq!(e.get(&(b, a)), Some(&1), "edge {a}-{b} has no twin");
    }
    // Manifold vertices: the triangles around each vertex form one fan.
    let mut around: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    for t in &m.triangles {
        for k in 0..3 {
            around
                .entry(t[k])
                .or_default()
                .push((t[(k + 1) % 3], t[(k + 2) % 3]));
        }
    }
    for (v, fan) in around {
        let next: HashMap<u32, u32> = fan.iter().copied().collect();
        let mut at = fan[0].0;
        let mut steps = 0;
        loop {
            at = next[&at];
            steps += 1;
            if at == fan[0].0 {
                break;
            }
            assert!(steps <= fan.len(), "vertex {v} is not a manifold fan");
        }
        assert_eq!(steps, fan.len(), "vertex {v}: more than one fan");
    }
}

fn volume(m: &Mesh) -> f32 {
    (0..m.triangles.len())
        .map(|t| {
            let [a, b, c] = m.triangle(t);
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
}

/// Empty cells all reach the border (no sealed cavity).
fn empty_is_connected(g: &VoxelGrid) -> bool {
    let grown = g.grown(1).unwrap();
    let empty = grown.bits(&|c| c == 0);
    let mut seen = Bits::new(empty.dims());
    let n = empty.flood(&mut seen, [0, 0, 0], |_, _, _, _| {});
    n == empty.count()
}

#[test]
fn a_loose_box_with_an_overlapping_box_becomes_one_closed_outward_solid() {
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [20.0; 3], 0.05);
    loose_box(&mut t, [5.0; 3], [25.0; 3], 0.05);
    let mesh = Mesh::from_triangles(&t);
    let g = voxelize(&mesh, 0.2, &all()).unwrap();
    assert!(g.labels.is_empty());
    let m = extract(&g, &all()).unwrap();
    assert_closed(&m);
    assert!(m.groups.is_none());
    // Union: 8000 + 8000 - 15^3 = 12625 mm^3; about half a voxel larger on every side.
    let v = volume(&m);
    assert!((v - 12625.0).abs() / 12625.0 < 0.04, "volume {v}");
    let b = m.bounds().unwrap();
    for i in 0..3 {
        assert!(
            (b.min[i] - 0.0).abs() < 0.3 && (b.max[i] - 25.0).abs() < 0.3,
            "{b:?}"
        );
    }
}

#[test]
fn open_and_single_sided_surfaces_still_give_a_closed_shell() {
    // A box missing its top face (a hole wider than the seal) and a lone plate.
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [10.0; 3], 0.0);
    t.drain(2..4);
    quad(
        &mut t,
        [20.0, 0.0, 0.0],
        [30.0, 0.0, 0.0],
        [30.0, 10.0, 0.0],
        [20.0, 10.0, 0.0],
    );
    let g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let m = extract(&g, &all()).unwrap();
    assert_closed(&m);
    // The open box is filled (its inside is seen from above only), the plate is a thin slab.
    let v = volume(&m);
    assert!(v > 1000.0 && v < 1000.0 * 1.15 + 100.0 * 0.8, "{v}");
    let c = g.cell_at([5.0, 5.0, 5.0]).unwrap();
    assert!(g.is_solid(c[0], c[1], c[2]));
}

#[test]
fn a_cell_is_outside_when_seen_from_enough_directions() {
    // A cup open at +z: its inside is seen from one direction, the space beside it from five.
    let mut s = Bits::new([9, 9, 9]);
    for z in 2..7 {
        for y in 2..7 {
            for x in 2..7 {
                let wall = x == 2 || x == 6 || y == 2 || y == 6 || z == 2;
                s.set(x, y, z, wall);
            }
        }
    }
    let seen = seen_from(&s, 3);
    assert!(!seen.get(4, 4, 4), "inside the cup");
    assert!(seen.get(0, 4, 4) && seen.get(4, 4, 8) && seen.get(4, 4, 0));
    assert!(seen_from(&s, 1).get(4, 4, 4));
    assert!(!seen_from(&s, 2).get(4, 4, 4));
    // Beside the cup, at its height: blocked along x only.
    assert!(seen_from(&s, 5).get(0, 4, 4) && !seen_from(&s, 6).get(0, 4, 4));
}

#[test]
fn cells_touching_only_at_an_edge_or_corner_still_extract_manifold() {
    let mut g = VoxelGrid::new([0.0; 3], 1.0, [4, 4, 4]).unwrap();
    g.set(1, 1, 1, 1);
    g.set(2, 2, 1, 1); // edge
    g.set(2, 1, 2, 0);
    g.set(1, 2, 2, 0);
    g.set(3, 3, 2, 1); // corner of (2, 2, 1)
    let m = extract(&g, &all()).unwrap();
    assert_closed(&m);
    // The complement case: a solid block with two empty cells meeting at a corner.
    let mut g = VoxelGrid::new([0.0; 3], 1.0, [4, 4, 4]).unwrap();
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..4 {
                g.set(x, y, z, 1);
            }
        }
    }
    g.set(1, 1, 1, 0);
    g.set(2, 2, 2, 0);
    let m = extract(&g, &all()).unwrap();
    assert_closed(&m);
}

#[test]
fn groups_label_cells_and_extract_one_closed_mesh_per_group() {
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [10.0; 3], 0.0);
    let n = t.len();
    loose_box(&mut t, [12.0, 0.0, 0.0], [20.0, 10.0, 10.0], 0.0);
    let labeled = crate::assets::stl::LabeledTriangles {
        triangles: t.clone(),
        node: (0..t.len()).map(|i| (i >= n) as u32).collect(),
        nodes: vec!["hull".into(), "glass".into()],
    };
    let mesh = Mesh::from_labeled(&labeled);
    let g = voxelize(&mesh, 0.4, &all()).unwrap();
    assert_eq!(g.labels, ["hull", "glass"]);
    assert_eq!(g.get(5, 12, 12), 1, "inside the first box");
    let m = extract(&g, &all()).unwrap();
    assert_closed(&m);
    let groups = m.groups.as_ref().unwrap();
    assert!(groups.contains(&0) && groups.contains(&1));
    for v in [1u8, 2] {
        let one = extract_where(&g, &move |c| c == v, &all()).unwrap();
        assert_closed(&one);
    }
}

#[test]
fn bits_dilate_erode_flood_and_components() {
    let mut b = Bits::new([20, 20, 20]);
    b.set(10, 10, 10, true);
    let d = b.dilate(2.0);
    assert!(d.get(12, 10, 10) && d.get(10, 8, 10) && !d.get(12, 12, 10));
    assert_eq!(d.erode(2.0).count(), 1);
    // A ball radius 6 (two chained passes) and back.
    let big = b.dilate(6.0);
    assert!(big.get(16, 10, 10) && !big.get(17, 10, 10));
    let mut two = Bits::new([130, 3, 3]);
    two.set(1, 1, 1, true);
    two.set(2, 1, 1, true);
    two.set(100, 1, 1, true);
    let mut n = Vec::new();
    two.components(|c, runs| n.push((c, runs.len())));
    assert_eq!(n, [(2, 1), (1, 1)]);
    assert_eq!(two.not().count(), 130 * 9 - 3);
}

#[test]
fn thicken_grows_a_thin_plate_to_the_minimum() {
    let mut t = Vec::new();
    quad(
        &mut t,
        [0.0, 0.0, 0.0],
        [20.0, 0.0, 0.0],
        [20.0, 20.0, 0.0],
        [0.0, 20.0, 0.0],
    );
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.1, &all()).unwrap();
    let thick = |g: &VoxelGrid| {
        let (lo, hi) = g.solid_bounds().unwrap();
        (hi[2] - lo[2]) as f32 * g.voxel
    };
    assert!(thick(&g) < 0.35, "{}", thick(&g));
    thicken(&mut g, 0.8, &all()).unwrap();
    assert!(thick(&g) >= 0.79, "{}", thick(&g));
    assert_closed(&extract(&g, &all()).unwrap());
    // A thick block is left alone.
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [10.0; 3], 0.0);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let before = g.count_solid();
    thicken(&mut g, 0.8, &all()).unwrap();
    assert_eq!(g.count_solid(), before);
}

#[test]
fn hollow_keeps_the_walls_and_drains_through_the_bottom() {
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [30.0, 30.0, 20.0], 0.0);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let full = g.count_solid();
    let mut notes = Vec::new();
    hollow(&mut g, 2.0, 3.0, &mut notes, &all()).unwrap();
    assert!(g.count_solid() < full / 2, "{} of {full}", g.count_solid());
    assert!(
        notes[0].contains("1 cavity") && notes[0].contains("drain hole"),
        "{notes:?}"
    );
    assert!(empty_is_connected(&g), "the cavity drains");
    let m = extract(&g, &all()).unwrap();
    assert_closed(&m);
    let v = volume(&m);
    assert!(v > 0.0 && v < 0.5 * 30.0 * 30.0 * 20.0, "{v}");
    // Walls stay: a cell 1 mm inside the side is solid, the centre is not.
    let c = g.cell_at([1.0, 15.0, 10.0]).unwrap();
    assert!(g.is_solid(c[0], c[1], c[2]));
    let c = g.cell_at([15.0, 15.0, 10.0]).unwrap();
    assert!(!g.is_solid(c[0], c[1], c[2]));
    // No drain: a sealed cavity, still a closed (two-shell) mesh.
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let mut notes = Vec::new();
    hollow(&mut g, 2.0, 0.0, &mut notes, &all()).unwrap();
    assert!(!empty_is_connected(&g));
    assert_closed(&extract(&g, &all()).unwrap());
    // Too thin to hollow.
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [3.0, 30.0, 30.0], 0.0);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let mut notes = Vec::new();
    hollow(&mut g, 2.0, 3.0, &mut notes, &all()).unwrap();
    assert!(notes[0].starts_with("Not hollowed"), "{notes:?}");
    // A wall thinner than one voxel (asked below the resolution, or the grid coarsened past
    // it) leaves the model whole instead of clearing it as one big cavity.
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [30.0, 30.0, 20.0], 0.0);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let full = g.count_solid();
    let mut notes = Vec::new();
    hollow(&mut g, 0.3, 0.0, &mut notes, &all()).unwrap();
    assert_eq!(g.count_solid(), full);
    assert!(notes[0].contains("thinner than the"), "{notes:?}");
}

#[test]
fn pegs_go_across_cuts_and_into_the_underside_without_breaking_through() {
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [40.0, 20.0, 20.0], 0.0);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let full = g.count_solid();
    let mut notes = Vec::new();
    let cuts = [Cut { axis: 0, at: 20.0 }];
    peg(&mut g, &cuts, 5.0, 8.0, &mut notes).unwrap();
    assert!(g.count_solid() < full);
    assert!(
        notes[0].starts_with("Cut 1 (x 20.0): 1 peg hole of 5 mm at y 10.0 z 10.0."),
        "{notes:?}"
    );
    // Beside the peg hole (full depth, off the centre of mass).
    assert!(
        notes[1].starts_with("Stand hole of 5 mm, 8.0 mm deep"),
        "{notes:?}"
    );
    // The peg hole is at the middle of the cut face, 8 mm into both sides.
    for x in [12.5, 17.0, 23.0, 27.5] {
        let c = g.cell_at([x, 10.0, 10.0]).unwrap();
        assert!(!g.is_solid(c[0], c[1], c[2]), "x {x}");
    }
    for x in [10.0, 30.0] {
        let c = g.cell_at([x, 10.0, 10.0]).unwrap();
        assert!(g.is_solid(c[0], c[1], c[2]), "x {x}");
    }
    // (The peg hole is sealed inside until the split opens it.)
    assert!(!empty_is_connected(&g));
    assert_closed(&extract(&g, &all()).unwrap());
    // Without cuts: the stand hole at the centre of mass, open at the bottom only.
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let mut notes = Vec::new();
    peg(&mut g, &[], 5.0, 8.0, &mut notes).unwrap();
    assert!(
        notes[0].starts_with("Stand hole of 5 mm, 8.0 mm deep, in the underside at x 20.0 y 10.0"),
        "{notes:?}"
    );
    for z in [0.1, 7.0] {
        let c = g.cell_at([20.0, 10.0, z]).unwrap();
        assert!(!g.is_solid(c[0], c[1], c[2]), "z {z}");
    }
    for z in [9.0, 19.8] {
        let c = g.cell_at([20.0, 10.0, z]).unwrap();
        assert!(g.is_solid(c[0], c[1], c[2]), "z {z}");
    }
    assert_closed(&extract(&g, &all()).unwrap());
    // A thin part: clamped, never through.
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [20.0, 20.0, 4.0], 0.0);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let mut notes = Vec::new();
    peg(&mut g, &[], 5.0, 8.0, &mut notes).unwrap();
    assert!(notes[0].contains("the part is thin there"), "{notes:?}");
    let c = g.cell_at([10.0, 10.0, 3.9]).unwrap();
    assert!(g.is_solid(c[0], c[1], c[2]), "the top is kept");
    assert!(empty_is_connected(&g));
    assert_closed(&extract(&g, &all()).unwrap());
    // No room on a cut face narrower than the peg.
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [20.0, 3.0, 3.0], 0.0);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    let mut notes = Vec::new();
    peg(&mut g, &[Cut { axis: 0, at: 10.0 }], 5.0, 8.0, &mut notes).unwrap();
    assert!(
        notes.iter().any(|n| n.contains("no room for a 5 mm peg")),
        "{notes:?}"
    );
}

#[test]
fn hollow_and_pegs_together_stay_manifold() {
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [40.0, 30.0, 25.0], 0.02);
    loose_box(&mut t, [10.0, 10.0, 20.0], [30.0, 20.0, 30.0], 0.02);
    let mut g = voxelize(&Mesh::from_triangles(&t), 0.4, &all()).unwrap();
    thicken(&mut g, 0.8, &all()).unwrap();
    let mut notes = Vec::new();
    hollow(&mut g, 2.0, 3.0, &mut notes, &all()).unwrap();
    peg(&mut g, &[Cut { axis: 0, at: 20.0 }], 3.0, 8.0, &mut notes).unwrap();
    assert_closed(&extract(&g, &all()).unwrap());
}

#[test]
fn the_grid_is_coarsened_to_the_budget_and_cancelling_stops() {
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [100.0, 100.0, 100.0], 0.0);
    let mesh = Mesh::from_triangles(&t);
    // 1000^3 / 0.1 mm would be 10^12 cells.
    let ([a, b, c], res) = plan_grid([100.0; 3], 0.1, 4);
    assert!(a as u64 * b as u64 * c as u64 <= CELL_BUDGET && res > 0.1);
    let g = VoxelGrid::new([0.0; 3], res, [1, 1, 1]).unwrap();
    assert!(coarsened(&g, 0.1).unwrap().contains("instead of 0.100 mm"));
    assert_eq!(coarsened(&g, res), None);
    let e = voxelize(&mesh, 0.4, &|_, _| false).unwrap_err();
    assert_eq!(e.code, "cancelled");
}

#[test]
fn the_pipeline_builds_a_closed_solid() {
    use super::super::{PrintOptions, ScaleMode, apply};
    let mut t = Vec::new();
    loose_box(&mut t, [0.0; 3], [2.0, 1.0, 0.5], 0.0); // metres
    let out = apply(
        Mesh::from_triangles(&t),
        &PrintOptions {
            scale: ScaleMode::FitMm(40.0),
            solid: true,
            solid_res_mm: 0.4,
            thicken_mm: Some(0.8),
            hollow: Some(Default::default()),
            ..Default::default()
        },
        &all(),
    )
    .unwrap();
    assert_eq!(out.parts.len(), 1);
    let m = &out.parts[0].mesh;
    assert_closed(m);
    assert!(m.bounds().unwrap().min[2].abs() < 1e-4, "sits on the bed");
    assert!(
        out.notes.iter().any(|n| n.starts_with("Hollowed")),
        "{:?}",
        out.notes
    );
}
