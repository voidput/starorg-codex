//! The print pipeline's shared parts: options (defaults, serde, checks), the mesh type, the
//! fixed pipeline with its stubs, the writer (units, objects, layouts) and the cache key.

use super::*;
use crate::assets::threemf::{Layout, Object, Unit, write_3mf_objects};

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

fn all() -> impl Fn(f32, &str) -> bool + Sync {
    |_, _| true
}

#[test]
fn print_options_default_to_off_and_read_partial_json() {
    let d = PrintOptions::default();
    assert!(d.is_default());
    assert_eq!(d.scale, ScaleMode::None);
    assert_eq!(d.bed_mm, [256.0; 3]);
    assert_eq!(d.solid_res_mm, 0.2);
    assert_eq!(d.validate(), Ok(()));
    let j = serde_json::to_value(&d).unwrap();
    assert_eq!(j["scale"], serde_json::json!({ "mode": "none" }));
    let p: PrintOptions = serde_json::from_value(serde_json::json!({
        "scale": { "mode": "ratio", "value": 72 },
        "hollow": { "wall_mm": 1.5 },
        "no_weapons": true
    }))
    .unwrap();
    assert_eq!(p.scale, ScaleMode::Ratio(72));
    assert_eq!(
        p.hollow,
        Some(Hollow {
            wall_mm: 1.5,
            drain_mm: 3.0
        })
    );
    assert!(p.no_weapons && !p.solid);
    let back: PrintOptions = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
    assert_eq!(back, p);
}

#[test]
fn options_without_what_they_need_or_out_of_range_are_refused() {
    let d = PrintOptions::default;
    let fit = ScaleMode::FitMm(150.0);
    let err = |p: PrintOptions| p.validate().unwrap_err();
    // The same wording as app/tests/printoptions.test.ts.
    let cases: Vec<(PrintOptions, &str)> = vec![
        (
            PrintOptions { split: true, ..d() },
            "--split needs --solid (experimental)",
        ),
        (
            PrintOptions {
                thicken_mm: Some(0.8),
                ..d()
            },
            "--thicken needs --solid (experimental)",
        ),
        (
            PrintOptions {
                hollow: Some(Hollow::default()),
                ..d()
            },
            "--hollow needs --solid (experimental)",
        ),
        (
            PrintOptions { solid: true, ..d() },
            "--solid needs --fit-mm, --scale-ratio or --shrink-to-bed (a real-size ship is metres long)",
        ),
        (
            PrintOptions {
                solid: true,
                scale: fit,
                split: true,
                peg_mm: Some(4.0),
                ..d()
            },
            "peg size 4 mm must be 3, 5 or 6",
        ),
        (
            PrintOptions {
                solid_res_mm: 0.3,
                ..d()
            },
            "solid resolution 0.3 mm must be 0.1, 0.2 or 0.4",
        ),
        (
            PrintOptions {
                scale: ScaleMode::Ratio(0),
                ..d()
            },
            "scale ratio must be 1 or more (1:N)",
        ),
        (
            PrintOptions {
                simplify_tris: Some(10),
                ..d()
            },
            "simplify to 10 triangles: at least 1000",
        ),
        (
            PrintOptions {
                drop_small_mm: Some(0.0),
                ..d()
            },
            "drop-small size 0 mm must be more than 0",
        ),
    ];
    for (p, want) in cases {
        assert_eq!(err(p), want);
    }
    assert!(
        err(PrintOptions {
            scale: ScaleMode::FitMm(f32::NAN),
            ..d()
        })
        .contains("out of range")
    );
    assert!(
        err(PrintOptions {
            bed_mm: [256.0, 0.0, 256.0],
            ..d()
        })
        .contains("bed size")
    );
    // Every option on goes together; shrink-to-bed alone lets the solid through.
    let every = PrintOptions {
        scale: fit,
        shrink_to_bed: true,
        drop_small_mm: Some(1.0),
        solid: true,
        solid_res_mm: 0.1,
        thicken_mm: Some(0.8),
        hollow: Some(Hollow::default()),
        peg_mm: Some(6.0),
        split: true,
        multicolor: true,
        simplify_tris: Some(200_000),
        no_weapons: true,
        ..d()
    };
    assert_eq!(every.validate(), Ok(()));
    assert_eq!(
        PrintOptions {
            solid: true,
            shrink_to_bed: true,
            ..d()
        }
        .validate(),
        Ok(())
    );
}

#[test]
fn no_weapons_is_in_the_cache_key_of_a_print_shell_only() {
    use crate::assets::model::{ModelOptions, ModelPreset};
    let o = |preset, print: PrintOptions| ModelOptions {
        preset,
        lod: 0,
        print,
        ..Default::default()
    };
    let nw = PrintOptions {
        no_weapons: true,
        ..Default::default()
    };
    assert_eq!(
        o(ModelPreset::PrintShell, nw.clone()).key(),
        "print-lod0-noweapons"
    );
    assert_eq!(o(ModelPreset::Full, nw).key(), "full-lod0");
    // The STL/3MF options leave the GLB (and its key) alone.
    let solid = PrintOptions {
        solid: true,
        scale: ScaleMode::FitMm(100.0),
        split: true,
        ..Default::default()
    };
    assert_eq!(o(ModelPreset::PrintShell, solid).key(), "print-lod0");
    // A Print shell's ModelOptions travel to the asset worker with their print options.
    let m = o(
        ModelPreset::PrintShell,
        PrintOptions {
            multicolor: true,
            ..Default::default()
        },
    );
    let j = serde_json::to_value(&m).unwrap();
    assert_eq!(j["print"]["multicolor"], true);
    let back: ModelOptions = serde_json::from_value(j).unwrap();
    assert_eq!(back, m);
    let plain = serde_json::to_value(o(ModelPreset::PrintShell, PrintOptions::default())).unwrap();
    assert!(
        plain.get("print").is_none(),
        "default print options are left out"
    );
}

#[test]
fn mesh_helpers_keep_groups_with_their_triangles() {
    let labeled = crate::assets::stl::LabeledTriangles {
        triangles: vec![
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
        ],
        node: vec![0, 0, 1],
        nodes: vec!["geo_body".into(), "geo_glass".into()],
    };
    let mut m = Mesh::from_labeled(&labeled);
    // The degenerate triangle is dropped with its group.
    assert_eq!(m.triangles.len(), 2);
    assert_eq!(m.groups, Some(vec![0, 1]));
    assert_eq!(m.label(m.group(1)), "geo_glass");
    let b = m.bounds().unwrap();
    assert_eq!((b.min, b.max), ([0.0; 3], [1.0, 1.0, 0.0]));
    assert_eq!(b.longest(), 1.0);
    let glass = m.subset([1]);
    assert_eq!(glass.vertices.len(), 3);
    assert_eq!(glass.groups, Some(vec![1]));
    m.retain_triangles(|t| t == 0);
    assert_eq!(m.triangles, vec![[0, 1, 2]]);
    assert_eq!(m.vertices.len(), 3);
    assert_eq!(m.groups, Some(vec![0]));
    let mut a = cube([0.0; 3], [1.0; 3]);
    a.append(&glass);
    assert_eq!(a.triangles.len(), 13);
    assert_eq!(a.label(a.group(12)), "geo_glass");
    assert_eq!(a.group(0), 0);
    assert_eq!(a.soup().len(), 13);
}

#[test]
fn the_default_pipeline_only_sits_the_model_on_the_bed_in_metres() {
    let out = apply(
        cube([2.0, 3.0, -1.5], [4.0, 5.0, 0.5]),
        &PrintOptions::default(),
        &all(),
    )
    .unwrap();
    assert_eq!(out.unit, Unit::Meter);
    assert_eq!(out.layout, Layout::Separate);
    assert_eq!(out.parts.len(), 1);
    assert!(out.notes.is_empty());
    let b = out.parts[0].mesh.bounds().unwrap();
    assert_eq!((b.min, b.max), ([-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]));
    assert_eq!(out.triangles(), 12);
}

#[test]
fn fit_and_ratio_scale_to_millimetres() {
    // A 20 m ship, 5 m tall.
    let ship = || cube([0.0, 0.0, 1.0], [20.0, 8.0, 6.0]);
    let fit = apply(
        ship(),
        &PrintOptions {
            scale: ScaleMode::FitMm(150.0),
            ..Default::default()
        },
        &all(),
    )
    .unwrap();
    assert_eq!(fit.unit, Unit::Millimeter);
    let b = fit.parts[0].mesh.bounds().unwrap();
    assert!((b.longest() - 150.0).abs() < 1e-3, "{b:?}");
    assert_eq!(b.min[2], 0.0);
    let ratio = apply(
        ship(),
        &PrintOptions {
            scale: ScaleMode::Ratio(100),
            ..Default::default()
        },
        &all(),
    )
    .unwrap();
    let b = ratio.parts[0].mesh.bounds().unwrap();
    assert!((b.size()[0] - 200.0).abs() < 1e-3, "{b:?}");
    // Real size in mm when shrinking to the bed (the shrink itself is scale.rs's).
    let s = scale::factor(
        &ship().bounds().unwrap(),
        &PrintOptions {
            shrink_to_bed: true,
            ..Default::default()
        },
    );
    assert_eq!((s.k, s.unit), (1000.0, Unit::Millimeter));
}

#[test]
fn the_pipeline_refuses_bad_options_cancels_and_reports_the_solid_stub() {
    let bad = PrintOptions {
        split: true,
        ..Default::default()
    };
    let e = apply(cube([0.0; 3], [1.0; 3]), &bad, &all()).unwrap_err();
    assert_eq!(e.code, "invalid");
    let e = apply(
        cube([0.0; 3], [1.0; 3]),
        &PrintOptions {
            multicolor: true,
            ..Default::default()
        },
        &|_, _| false,
    )
    .unwrap_err();
    assert_eq!(e.code, "cancelled");
    assert!(apply(Mesh::default(), &PrintOptions::default(), &all()).is_err());
    // Multicolor with the stubs: one part, one assembly.
    let mc = apply(
        cube([0.0; 3], [1.0; 3]),
        &PrintOptions {
            multicolor: true,
            ..Default::default()
        },
        &all(),
    )
    .unwrap();
    assert_eq!((mc.parts.len(), mc.layout), (1, Layout::Assembly));
}

#[test]
fn a_voxel_grid_indexes_crops_and_finds_cells() {
    let mut g = voxel::VoxelGrid::new([10.0, 0.0, 0.0], 0.5, [4, 3, 2]).unwrap();
    assert_eq!(g.len(), 24);
    g.set(3, 2, 1, 1);
    g.set(9, 9, 9, 1); // outside: ignored
    assert!(g.is_solid(3, 2, 1) && !g.is_solid(0, 0, 0));
    assert_eq!(g.get(99, 0, 0), 0);
    assert_eq!(g.count_solid(), 1);
    assert_eq!(g.center(0, 0, 0), [10.25, 0.25, 0.25]);
    assert_eq!(g.cell_at([11.9, 1.4, 0.9]), Some([3, 2, 1]));
    assert_eq!(g.cell_at([9.9, 0.0, 0.0]), None);
    let c = g.crop([2, 1, 1], [9, 9, 9]).unwrap();
    assert_eq!(c.dims, [2, 2, 1]);
    assert_eq!(c.origin, [11.0, 0.5, 0.5]);
    assert!(c.is_solid(1, 1, 0));
    assert!(voxel::VoxelGrid::new([0.0; 3], 0.1, [100_000, 100_000, 100_000]).is_err());
}

#[test]
fn a_3mf_holds_several_objects_as_items_or_one_assembly_in_millimetres() {
    let t = tempfile::tempdir().unwrap();
    let a = cube([0.0; 3], [1.0; 3]);
    let b = cube([2.0, 0.0, 0.0], [3.0, 1.0, 1.0]);
    let objs = [
        Object {
            name: "hull",
            mesh: &a,
            color: Some([0x80, 0x80, 0x80, 0xff]),
        },
        Object {
            name: "glass & <canopy>",
            mesh: &b,
            color: None,
        },
    ];
    let model = |p: &std::path::Path| {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
        let mut s = String::new();
        z.by_name("3D/3dmodel.model")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        s
    };
    let sep = t.path().join("sep.3mf");
    let (n, _) = write_3mf_objects(&objs, Unit::Millimeter, Layout::Separate, &sep).unwrap();
    assert_eq!(n, 24);
    let m = model(&sep);
    assert!(m.contains(r#"<model unit="millimeter""#));
    assert!(m.contains(r#"<basematerials id="4">"#));
    assert!(m.contains(r##"<base name="hull" displaycolor="#808080FF"/>"##));
    assert!(m.contains(r#"<object id="1" type="model" name="hull" pid="4" pindex="0">"#));
    assert!(m.contains(r#"name="glass &amp; &lt;canopy&gt;" pid="4" pindex="1""#));
    assert!(m.contains(r#"<item objectid="1"/>"#) && m.contains(r#"<item objectid="2"/>"#));
    assert!(!m.contains("<components>"));
    let asm = t.path().join("asm.3mf");
    write_3mf_objects(&objs, Unit::Meter, Layout::Assembly, &asm).unwrap();
    let m = model(&asm);
    assert!(m.contains(r#"<model unit="meter""#));
    assert!(m.contains(r#"<object id="3" type="model">"#));
    assert!(
        m.contains(r#"<component objectid="1"/>"#) && m.contains(r#"<component objectid="2"/>"#)
    );
    let items: Vec<&str> = m.lines().filter(|l| l.starts_with("<item ")).collect();
    assert_eq!(items, [r#"<item objectid="3"/>"#]);
    // Empty objects are left out; none at all is refused.
    let empty = Mesh::default();
    let none = [Object {
        name: "",
        mesh: &empty,
        color: None,
    }];
    assert!(
        write_3mf_objects(
            &none,
            Unit::Meter,
            Layout::Separate,
            &t.path().join("e.3mf")
        )
        .is_err()
    );
}

#[test]
fn separate_parts_get_one_stl_each_and_one_3mf() {
    let t = tempfile::tempdir().unwrap();
    let out = PrintOutput {
        parts: vec![
            Part {
                name: "piece-1".into(),
                mesh: cube([0.0; 3], [1.0; 3]),
                color: None,
            },
            Part {
                name: "piece-2 glass".into(),
                mesh: cube([2.0, 0.0, 0.0], [3.0, 1.0, 1.0]),
                color: None,
            },
        ],
        unit: Unit::Millimeter,
        layout: Layout::Separate,
        notes: vec![],
    };
    let glb = t.path().join("ship.glb");
    let files = write_output(&out, &glb, true, true).unwrap();
    let names: Vec<(String, u64)> = files
        .iter()
        .map(|f| {
            (
                std::path::Path::new(&f.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                f.triangles,
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("ship-piece-1.stl".to_string(), 12),
            ("ship-piece-2_glass.stl".to_string(), 12),
            ("ship.3mf".to_string(), 24)
        ]
    );
    // One assembly: one STL of everything.
    let asm = PrintOutput {
        layout: Layout::Assembly,
        ..out
    };
    let files = write_output(&asm, &t.path().join("b.glb"), true, false).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].triangles, 24);
    assert!(files[0].path.ends_with("b.stl"));
}
