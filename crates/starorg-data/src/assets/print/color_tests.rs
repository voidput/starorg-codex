//! Multicolor: node paths to groups, parts per group, and one closed mesh per group of a solid.

use super::*;
use crate::assets::print::{PrintOptions, ScaleMode, apply, write_output};
use crate::assets::stl::LabeledTriangles;
use std::collections::HashMap;

fn all() -> impl Fn(f32, &str) -> bool + Sync {
    |_, _| true
}

/// A closed box from `min` to `max`, 12 triangles, outward.
fn box_tris(min: [f32; 3], max: [f32; 3]) -> Vec<[[f32; 3]; 3]> {
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
    let mut t = Vec::new();
    for [a, b, cc, d] in faces {
        t.push([c(a), c(b), c(cc)]);
        t.push([c(a), c(cc), c(d)]);
    }
    t
}

/// Boxes labelled with node paths.
fn ship(parts: &[(&str, [f32; 3], [f32; 3])]) -> Mesh {
    let mut l = LabeledTriangles::default();
    for (i, (path, lo, hi)) in parts.iter().enumerate() {
        l.nodes.push(path.to_string());
        for t in box_tris(*lo, *hi) {
            l.triangles.push(t);
            l.node.push(i as u32);
        }
    }
    Mesh::from_labeled(&l)
}

/// Every directed edge once and its reverse once (closed, consistently oriented).
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

fn volume(m: &Mesh) -> f32 {
    (0..m.triangles.len())
        .map(|t| {
            let [a, b, c] = m.triangle(t);
            let cr = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            (a[0] * cr[0] + a[1] * cr[1] + a[2] * cr[2]) / 6.0
        })
        .sum()
}

#[test]
fn node_paths_from_real_ships_name_their_group() {
    let cases = [
        // Skylark (Print shell, LOD 4)
        ("CryEngine_Z_up/rsi_skylark/geo_body", HULL),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/lower_cockpit_frame/geo_cockpit_glass_lower_ext",
            GLASS,
        ),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/anim_cockpit_frame/anim_cockpit_glass_upper_hinge/geo_cockpit_glass_upper_ext",
            GLASS,
        ),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/anim_cockpit_frame",
            HULL,
        ),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/hardpoint_thruster_main_left/RSI_Skylark_Thruster_Main_Left/rsi_skylark_thruster_main_left/geo_thruster_main_left",
            ENGINE,
        ),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/hardpoint_thruster_retro_left/RSI_Skylark_Thruster_Retro_Left/rsi_skylark_thruster_retro_left/geo_skylark_thruster_retro_left",
            HULL,
        ),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/hardpoint_thruster_mav_top_front_left/RSI_Skylark_Thruster_Mav_Top/rsi_skylark_thruster_mav/geo_thruster_mav",
            HULL,
        ),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/anim_component_racking/hardpoint_relay/RELAY_2slot_slim/relay_demo_2fuse_s01/mesh_relay01/LinkedGlow_Fuse",
            HULL,
        ),
        (
            "CryEngine_Z_up/rsi_skylark/geo_body/hardpoint_fuelport_ground/RSI_Skylark_Ground_Refueling/rsi_skylark_fuelport_ground/anim_fuelport_ground_housing/anim_fuelport_ground_nozzle",
            HULL,
        ),
        // Gladius
        ("CryEngine_Z_up/AEGS_Gladius/Body/Nose/Canopy", GLASS),
        (
            "CryEngine_Z_up/AEGS_Gladius/Body/hardpoint_thruster_main_left/AEGS_Gladius_Thruster_Main/AEGS_Gladius_Thruster_Main/Gladius_Thruster_Main",
            ENGINE,
        ),
        (
            "CryEngine_Z_up/AEGS_Gladius/Body/Nose/MAYA_thruster_front_left_top_CTRL/hardpoint_thruster_front_left_top/AEGS_Gladius_Thruster_Mav_Joint_01/AEGS_Gladius_Thruster_Mav_Joint_01/YAW_CONTROL/PITCH_CONTROL/Gladius_Thruster_Mav_Joint_01",
            HULL,
        ),
        // Made up: a canopy frame, a VTOL, an engine without "thruster".
        ("ship/body/canopy_frame", HULL),
        ("ship/body/hardpoint_thruster_vtol_left/geo", ENGINE),
        ("ship/body/engine_left/geo_exhaust", ENGINE),
        ("", HULL),
    ];
    for (path, want) in cases {
        assert_eq!(group_of(path), want, "{path}");
    }
}

#[test]
fn classify_groups_by_node_and_by_group_gives_one_part_per_group_with_its_colour() {
    let mut m = ship(&[
        ("ship/geo_body", [0.0; 3], [10.0, 4.0, 2.0]),
        ("ship/geo_body/geo_canopy", [2.0, 1.0, 2.0], [4.0, 3.0, 3.0]),
        (
            "ship/geo_body/hardpoint_thruster_main/geo",
            [10.0, 1.0, 0.5],
            [11.0, 3.0, 1.5],
        ),
        (
            "ship/geo_body/hardpoint_thruster_mav/geo",
            [5.0, 4.0, 1.0],
            [6.0, 5.0, 2.0],
        ),
    ]);
    classify(&mut m);
    assert_eq!(m.labels, GROUPS);
    let parts = by_group(&m);
    let names: Vec<&str> = parts.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, [HULL, ENGINE, GLASS]);
    let tris: Vec<usize> = parts.iter().map(|(_, p)| p.triangles.len()).collect();
    assert_eq!(tris, [24, 12, 12], "the mav thruster is hull");
    let colors: Vec<[u8; 4]> = names.iter().map(|n| color_of(n).unwrap()).collect();
    assert!(colors[0] != colors[1] && colors[1] != colors[2] && colors[0] != colors[2]);
    assert_eq!(color_of("piece-1"), None);
    // No groups at all: everything is hull.
    let mut plain = Mesh::from_triangles(&box_tris([0.0; 3], [1.0; 3]));
    classify(&mut plain);
    let p = by_group(&plain);
    assert_eq!((p.len(), p[0].0.as_str()), (1, HULL));
    assert_eq!(
        by_group(&Mesh::from_triangles(&box_tris([0.0; 3], [1.0; 3])))[0].0,
        ""
    );
}

/// A solid block of hull with the given outer faces painted (labels `GROUPS`).
fn block(n: u32) -> VoxelGrid {
    let mut g = VoxelGrid::new([0.0; 3], 1.0, [n + 4, n + 4, n + 4]).unwrap();
    g.labels = GROUPS.iter().map(|s| s.to_string()).collect();
    for z in 2..n + 2 {
        for y in 2..n + 2 {
            for x in 2..n + 2 {
                g.set(x, y, z, 1);
            }
        }
    }
    g
}

#[test]
fn paint_grows_glass_and_engine_inwards_and_glass_wins_where_they_meet() {
    let n = 10;
    let mut g = block(n);
    let (top, side) = (n + 1, n + 1);
    // +x face engine (value 2), then the top face glass (value 3), edge included.
    for z in 2..n + 2 {
        for y in 2..n + 2 {
            g.set(side, y, z, 2);
        }
    }
    for y in 2..n + 2 {
        for x in 2..n + 2 {
            g.set(x, y, top, 3);
        }
    }
    // Inside cells start with arbitrary labels (voxelize fills along x): they become hull.
    g.set(5, 5, 5, 3);
    let hull = paint(&mut g, 2.0, &all()).unwrap();
    assert_eq!(hull, 1);
    assert_eq!(g.get(5, 6, top), 3, "the glass surface stays");
    assert_eq!(g.get(5, 6, top - 1), 3, "one layer in");
    assert_eq!(g.get(5, 6, top - 2), 3, "two layers in");
    assert_eq!(g.get(5, 6, top - 3), 1, "past the depth: hull");
    assert_eq!(g.get(side - 2, 6, 5), 2, "engine two layers in");
    assert_eq!(g.get(5, 5, 5), 1, "the inside is hull");
    assert_eq!(g.get(side - 1, 6, top - 1), 3, "glass wins over engine");
    assert_eq!(g.get(2, 6, 5), 1, "the -x surface is hull");
    // A speck of glass under MIN_GROUP_CELLS becomes hull.
    let mut s = block(6);
    s.set(4, 4, 7, 3);
    paint(&mut s, 1.0, &all()).unwrap();
    assert_eq!(s.get(4, 4, 7), 1);
    // Cancelling stops.
    assert_eq!(
        paint(&mut block(4), 1.0, &|_, _| false).unwrap_err().code,
        "cancelled"
    );
}

#[test]
fn extract_groups_gives_one_closed_mesh_per_group_in_priority_order() {
    let mesh = {
        let mut m = ship(&[
            ("ship/geo_body", [0.0; 3], [20.0, 10.0, 6.0]),
            ("ship/geo_canopy", [4.0, 3.0, 6.0], [10.0, 7.0, 9.0]),
            (
                "ship/hardpoint_thruster_main/geo",
                [20.0, 3.0, 1.0],
                [24.0, 7.0, 5.0],
            ),
        ]);
        classify(&mut m);
        m
    };
    let grid = super::super::voxel::voxelize(&mesh, 0.5, &all()).unwrap();
    let parts = extract_groups(&grid, 1.0, &all()).unwrap();
    let names: Vec<&str> = parts.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, [HULL, ENGINE, GLASS]);
    let mut total = 0.0;
    for (name, m) in &parts {
        assert_closed(m);
        let v = volume(m);
        assert!(v > 0.0, "{name} is outward");
        total += v;
    }
    let whole = volume(&super::super::voxel::extract(&grid, &all()).unwrap());
    assert!(
        (total - whole).abs() < 0.1 * whole,
        "the groups add up to the solid: {total} vs {whole}"
    );
    // An unlabelled grid: one unlabelled mesh.
    let plain = super::super::voxel::voxelize(
        &Mesh::from_triangles(&box_tris([0.0; 3], [4.0; 3])),
        0.5,
        &all(),
    )
    .unwrap();
    let one = extract_groups(&plain, 1.0, &all()).unwrap();
    assert_eq!((one.len(), one[0].0.as_str()), (1, ""));
}

fn model_xml(p: &std::path::Path) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut s = String::new();
    z.by_name("3D/3dmodel.model")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

fn little_ship() -> Mesh {
    // Metres, as from the GLB.
    ship(&[
        ("ship/geo_body", [0.0; 3], [2.0, 1.0, 0.6]),
        (
            "ship/geo_body/geo_cockpit_glass",
            [0.4, 0.3, 0.6],
            [1.0, 0.7, 0.9],
        ),
        (
            "ship/hardpoint_thruster_main/geo",
            [2.0, 0.3, 0.1],
            [2.4, 0.7, 0.5],
        ),
    ])
}

#[test]
fn multicolor_writes_one_3mf_assembly_with_coloured_parts_and_one_stl() {
    let t = tempfile::tempdir().unwrap();
    let glb = t.path().join("ship.glb");
    for solid in [false, true] {
        let o = PrintOptions {
            multicolor: true,
            solid,
            scale: if solid {
                ScaleMode::FitMm(30.0)
            } else {
                ScaleMode::None
            },
            solid_res_mm: 0.4,
            ..Default::default()
        };
        let out = apply(little_ship(), &o, &all()).unwrap();
        assert_eq!(out.layout, Layout::Assembly);
        let names: Vec<&str> = out.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, [HULL, ENGINE, GLASS], "solid {solid}");
        for p in &out.parts {
            assert_eq!(p.color, color_of(&p.name));
            if solid {
                assert_closed(&p.mesh);
            }
        }
        let files = write_output(&out, &glb, true, true).unwrap();
        let kinds: Vec<&str> = files.iter().map(|f| f.kind.as_str()).collect();
        assert_eq!(kinds, ["stl", "3mf"], "one STL (a single mesh) and one 3MF");
        let m = model_xml(&glb.with_extension("3mf"));
        assert!(m.contains(r#"<basematerials id="5">"#), "{solid}");
        assert!(m.contains(r##"<base name="glass" displaycolor="#3A6EA5FF"/>"##));
        assert!(m.contains(r#"name="engine" pid="5" pindex="1""#));
        assert_eq!(m.matches("<component objectid=").count(), 3);
        let items: Vec<&str> = m.lines().filter(|l| l.starts_with("<item ")).collect();
        assert_eq!(items, [r#"<item objectid="4"/>"#]);
    }
}
