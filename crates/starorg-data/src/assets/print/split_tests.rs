//! Split for the bed: the fewest pieces that each fit, cut through thin sections, pin holes
//! across the cuts, every piece a closed mesh.

use super::super::voxel::extract;
use super::super::{Mesh, PrintOptions, ScaleMode, apply, write_output};
use super::*;
use std::collections::HashMap;

fn all() -> impl Fn(f32, &str) -> bool + Sync {
    |_, _| true
}

/// A grid of `dims` cells of `voxel`, solid where `solid(x, y, z)`.
fn grid(voxel: f32, dims: [u32; 3], solid: impl Fn(u32, u32, u32) -> bool) -> VoxelGrid {
    let mut g = VoxelGrid::new([0.0; 3], voxel, dims).unwrap();
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                if solid(x, y, z) {
                    g.set(x, y, z, 1);
                }
            }
        }
    }
    g
}

/// A closed box, outward.
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

/// Every edge used once each way (closed, consistently oriented).
fn assert_closed(m: &Mesh) {
    assert!(!m.triangles.is_empty());
    let mut e: HashMap<(u32, u32), u32> = HashMap::new();
    for t in &m.triangles {
        for k in 0..3 {
            *e.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    for (&(a, b), &n) in &e {
        assert_eq!(n, 1, "edge {a}-{b} used {n} times the same way");
        assert_eq!(e.get(&(b, a)), Some(&1), "edge {a}-{b} has no twin");
    }
}

#[test]
fn a_solid_that_fits_is_left_whole() {
    let g = grid(1.0, [50, 20, 20], |_, _, _| true);
    assert!(plan(&g, [100.0; 3]).is_empty());
    let p = split(g.clone(), &[]);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].name, "");
    assert_eq!(p[0].grid, g);
    assert!(summary(&[], &p).contains("no cut needed"));
}

#[test]
fn a_long_box_is_cut_into_the_fewest_pieces_that_fit() {
    let g = grid(1.0, [250, 30, 20], |_, _, _| true);
    let bed = [100.0, 100.0, 100.0];
    let cuts = plan(&g, bed);
    assert_eq!(cuts.len(), 2, "{cuts:?}");
    assert!(cuts.iter().all(|c| c.axis == 0), "{cuts:?}");
    let pieces = split(g, &cuts);
    let names: Vec<&str> = pieces.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["part1", "part2", "part3"]);
    let total: u64 = pieces.iter().map(|p| p.grid.count_solid()).sum();
    assert_eq!(total, 250 * 30 * 20, "no cell lost or doubled");
    let mut right = f32::NEG_INFINITY;
    for p in &pieces {
        let size = p.grid.dims.map(|d| d as f32 * p.grid.voxel);
        assert!((0..3).all(|a| size[a] <= bed[a]), "{size:?}");
        // Side by side, apart, low y lined up.
        assert!(p.grid.origin[0] >= right + LAYOUT_GAP - 1e-3);
        assert_eq!(p.grid.origin[1], 0.0);
        right = p.grid.origin[0] + size[0];
        assert_closed(&extract(&p.grid, &all()).unwrap());
    }
    assert!(summary(&cuts, &pieces).starts_with("Split into 3 parts"));
}

#[test]
fn a_tall_or_wide_solid_is_cut_on_the_axes_that_need_it() {
    let g = grid(1.0, [40, 150, 130], |_, _, _| true);
    let cuts = plan(&g, [100.0, 100.0, 100.0]);
    let mut axes: Vec<usize> = cuts.iter().map(|c| c.axis).collect();
    axes.sort();
    assert_eq!(axes, [1, 2]);
    assert_eq!(split(g, &cuts).len(), 4);
}

#[test]
fn the_cut_goes_through_the_thin_neck() {
    // Two blocks joined by a 2 x 2 neck from x 40 to 60.
    let g = grid(1.0, [100, 20, 20], |x, y, z| {
        !(40..60).contains(&x) || ((9..11).contains(&y) && (9..11).contains(&z))
    });
    let cuts = plan(&g, [80.0, 100.0, 100.0]);
    assert_eq!(cuts.len(), 1);
    assert!(
        cuts[0].axis == 0 && cuts[0].at > 40.0 && cuts[0].at < 60.0,
        "{cuts:?}"
    );
}

#[test]
fn empty_lattice_cells_are_left_out() {
    // An L: a bar along x and one along y; a 2 x 2 lattice leaves one corner empty.
    let g = grid(1.0, [100, 100, 10], |x, y, _| x < 20 || y < 20);
    let cuts = plan(&g, [60.0, 60.0, 60.0]);
    assert_eq!(cuts.len(), 2);
    let pieces = split(g, &cuts);
    assert_eq!(pieces.len(), 3);
    for p in &pieces {
        assert!(p.grid.dims.iter().all(|d| *d as f32 * p.grid.voxel <= 60.0));
    }
}

#[test]
fn pin_holes_go_across_the_cut_into_both_sides() {
    // 40 x 30 x 20 mm at 0.5 mm, cut at x 20: room for two 2 mm pins, 4 mm each side.
    let mut g = grid(0.5, [80, 60, 40], |_, _, _| true);
    let cut = Cut { axis: 0, at: 20.0 };
    let mut notes = Vec::new();
    pins(&mut g, &[cut], PIN_MM, PIN_DEPTH_MM, &mut notes);
    assert_eq!(notes.len(), 1);
    assert!(
        notes[0].starts_with("Cut 1 (x 20.0): 2 alignment pin holes of 2 mm, 4 mm into each side"),
        "{notes:?}"
    );
    // 2 holes, about pi * 2^2 cells each, 8 cells deep on each side.
    let gone = 80 * 60 * 40 - g.count_solid();
    assert!((2 * 2 * 8 * 9..2 * 2 * 8 * 16).contains(&gone), "{gone}");
    let pieces = split(g, &[cut]);
    assert_eq!(pieces.len(), 2);
    for p in &pieces {
        assert_eq!(p.grid.dims[0], 40);
        assert_closed(&extract(&p.grid, &all()).unwrap());
    }
    // A face too small for a pin: a note, nothing drilled.
    let mut g = grid(0.5, [80, 4, 4], |_, _, _| true);
    let mut notes = Vec::new();
    pins(&mut g, &[cut], PIN_MM, PIN_DEPTH_MM, &mut notes);
    assert!(notes[0].contains("no room"), "{notes:?}");
    assert_eq!(g.count_solid(), 80 * 4 * 4);
}

#[test]
fn the_pipeline_splits_a_long_ship_into_closed_parts_that_fit() {
    let t = tempfile::tempdir().unwrap();
    // 2.5 m x 0.3 m x 0.2 m, printed 250 mm long on a 100 mm bed.
    let o = PrintOptions {
        scale: ScaleMode::FitMm(250.0),
        bed_mm: [100.0; 3],
        solid: true,
        solid_res_mm: 0.4,
        split: true,
        ..Default::default()
    };
    let out = apply(cube([0.0; 3], [2.5, 0.3, 0.2]), &o, &all()).unwrap();
    let names: Vec<&str> = out.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["part1", "part2", "part3"]);
    let mut right = f32::NEG_INFINITY;
    for p in &out.parts {
        assert_closed(&p.mesh);
        let b = p.mesh.bounds().unwrap();
        assert!(b.size().iter().all(|s| *s <= 100.0), "{b:?}");
        assert!(b.min[2].abs() < 1e-4, "sits on the bed: {b:?}");
        assert!(b.min[0] > right, "side by side");
        right = b.max[0];
    }
    let pins = out
        .notes
        .iter()
        .filter(|n| n.contains("2 alignment pin holes"))
        .count();
    assert_eq!(pins, 2, "{:?}", out.notes);
    assert!(
        out.notes
            .iter()
            .any(|n| n.starts_with("Split into 3 parts")),
        "{:?}",
        out.notes
    );
    let files = write_output(&out, &t.path().join("ship.glb"), true, true).unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|f| {
            std::path::Path::new(&f.path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        names,
        [
            "ship-part1.stl",
            "ship-part2.stl",
            "ship-part3.stl",
            "ship.3mf"
        ]
    );
    let model = {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::fs::File::open(&files[3].path).unwrap()).unwrap();
        let mut s = String::new();
        z.by_name("3D/3dmodel.model")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        s
    };
    assert_eq!(
        model.matches("<item ").count(),
        3,
        "one build item per part"
    );
    assert!(model.contains(r#"name="part2""#));
}

#[test]
fn split_needs_the_solid() {
    let o = PrintOptions {
        scale: ScaleMode::FitMm(250.0),
        split: true,
        ..Default::default()
    };
    let e = apply(cube([0.0; 3], [1.0; 3]), &o, &all()).unwrap_err();
    assert_eq!(e.code, "invalid");
    assert!(e.message.contains("--split needs --solid"), "{}", e.message);
}

#[test]
fn split_parts_stay_side_by_side_centred_and_colour_parts_stay_with_their_piece() {
    let o = PrintOptions {
        scale: ScaleMode::FitMm(250.0),
        bed_mm: [100.0; 3],
        solid: true,
        solid_res_mm: 0.4,
        split: true,
        multicolor: true,
        ..Default::default()
    };
    let out = apply(cube([0.0; 3], [2.5, 0.3, 0.2]), &o, &all()).unwrap();
    assert_eq!(
        out.parts.len(),
        3,
        "{:?}",
        out.parts.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    let b: Vec<_> = out.parts.iter().map(|p| p.mesh.bounds().unwrap()).collect();
    for w in b.windows(2) {
        assert!(w[1].min[0] > w[0].max[0], "side by side: {b:?}");
    }
    let (lo, hi) = (b[0].min[0], b[b.len() - 1].max[0]);
    assert!((lo + hi).abs() < 1e-3, "centred on x: {lo} {hi}");
    assert!(b.iter().all(|x| x.min[2].abs() < 1e-4));
}

#[test]
fn pegs_without_split_give_only_the_stand_hole() {
    let o = PrintOptions {
        scale: ScaleMode::FitMm(60.0),
        solid: true,
        solid_res_mm: 0.4,
        peg_mm: Some(5.0),
        ..Default::default()
    };
    let out = apply(cube([0.0; 3], [1.0, 0.6, 0.4]), &o, &all()).unwrap();
    assert_eq!(out.parts.len(), 1);
    assert!(
        out.notes
            .iter()
            .any(|n| n.starts_with("Stand hole of 5 mm")),
        "{:?}",
        out.notes
    );
}
