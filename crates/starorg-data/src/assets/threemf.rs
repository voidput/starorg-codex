//! GLB -> 3MF (3D Manufacturing Format, core spec) in pure Rust, for slicers (PrusaSlicer,
//! Bambu Studio, Cura). The triangles are [`super::stl::glb_triangles`]'s (node transforms
//! baked, Z-up); vertices are shared (exact `f32` equality) so the mesh stays connected.
//! `unit="meter"` by default: StarBreaker's GLB is in game units, which are metres, so a
//! slicer shows the real size; a scaled print ([`super::print`]) is written in millimetres.
//! Several objects are written as separate build items or as the components of one
//! ([`Layout`]), optionally with a display colour each. The package is a deflated zip with
//! `[Content_Types].xml`, `_rels/.rels` and `3D/3dmodel.model`; the model XML is streamed into
//! the zip, never held whole.

use super::print::Mesh;
use super::{AssetError, AssetResult};
use std::io::Write;
use std::path::Path;

const CONTENT_TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>"#,
    "</Types>\n"
);

const RELS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>"#,
    "</Relationships>\n"
);

/// The length unit of a 3MF model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Unit {
    /// StarBreaker's game units, real size (the default, untouched export).
    #[default]
    Meter,
    /// A scaled print ([`super::print`]).
    Millimeter,
}

impl Unit {
    /// The `unit` attribute value.
    pub fn xml(self) -> &'static str {
        match self {
            Unit::Meter => "meter",
            Unit::Millimeter => "millimeter",
        }
    }

    /// Millimetres in one unit (1000 for metres).
    pub fn mm(self) -> f32 {
        match self {
            Unit::Meter => 1000.0,
            Unit::Millimeter => 1.0,
        }
    }
}

/// How several objects are placed in the build.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Layout {
    /// One build item per object (split pieces: each printed on its own).
    #[default]
    Separate,
    /// One build item made of every object as a component (multicolor parts of one model,
    /// which slicers show as one object with parts, one filament each).
    Assembly,
}

/// One object of a 3MF: a mesh with an optional name and display colour (`RGBA`).
#[derive(Debug, Clone, Copy)]
pub struct Object<'a> {
    pub name: &'a str,
    pub mesh: &'a Mesh,
    pub color: Option<[u8; 4]>,
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => o.push(' '),
            c => o.push(c),
        }
    }
    o
}

/// A shared-vertex mesh: unique vertices and triangles indexing them.
pub type IndexedMesh = Mesh;

/// Share vertices by exact `f32` bits (`-0.0` counts as `0.0`). Triangles with a non-finite
/// coordinate or two equal corners are dropped (3MF requires three distinct vertices).
pub fn index_mesh(tris: &[[[f32; 3]; 3]]) -> Mesh {
    Mesh::from_triangles(tris)
}

/// Write a 3MF package of `tris` (one object, metres); returns (triangles written, bytes).
pub fn write_3mf(tris: &[[[f32; 3]; 3]], out: &Path) -> AssetResult<(u64, u64)> {
    let mesh = index_mesh(tris);
    write_3mf_objects(
        &[Object {
            name: "",
            mesh: &mesh,
            color: None,
        }],
        Unit::Meter,
        Layout::Separate,
        out,
    )
}

/// Write a 3MF package of `objects` in `unit`, placed by `layout`; empty objects are left out.
/// Colours become one `basematerials` group (`displaycolor`). Returns (triangles, bytes).
pub fn write_3mf_objects(
    objects: &[Object<'_>],
    unit: Unit,
    layout: Layout,
    out: &Path,
) -> AssetResult<(u64, u64)> {
    let objects: Vec<&Object> = objects
        .iter()
        .filter(|o| !o.mesh.triangles.is_empty())
        .collect();
    if objects.is_empty() {
        return Err(AssetError::invalid("the model has no triangles"));
    }
    let n = objects.len() as u32;
    let tris: u64 = objects.iter().map(|o| o.mesh.triangles.len() as u64).sum();
    let verts: u64 = objects.iter().map(|o| o.mesh.vertices.len() as u64).sum();
    let assembly = layout == Layout::Assembly && n > 1;
    let colored = objects.iter().any(|o| o.color.is_some());
    // Object ids 1..=n, the assembly n + 1, the materials n + 2 (written first).
    let mat_id = n + 2;
    let io = |e: std::io::Error| AssetError::internal(format!("{}: {e}", out.display()));
    let zerr = |e: zip::result::ZipError| AssetError::internal(format!("{}: {e}", out.display()));
    let (part, file) = super::new_part(out)?;
    let r = (|| -> AssetResult<u64> {
        let mut z = zip::ZipWriter::new(std::io::BufWriter::new(file));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        z.start_file("[Content_Types].xml", opts).map_err(zerr)?;
        z.write_all(CONTENT_TYPES.as_bytes()).map_err(io)?;
        z.start_file("_rels/.rels", opts).map_err(zerr)?;
        z.write_all(RELS.as_bytes()).map_err(io)?;
        // About 45 bytes per vertex and 40 per triangle: Zip64 only when that may pass 4 GB.
        let guess = 64 * (verts + tris);
        z.start_file("3D/3dmodel.model", opts.large_file(guess > u32::MAX as u64))
            .map_err(zerr)?;
        {
            let mut w = std::io::BufWriter::with_capacity(1 << 16, &mut z);
            write!(
                w,
                concat!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                    "\n",
                    r#"<model unit="{}" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">"#,
                    "\n",
                    r#"<metadata name="Application">StarOrg Codex (StarBreaker GLB, Z-up)</metadata>"#,
                    "\n<resources>\n",
                ),
                unit.xml()
            )
            .map_err(io)?;
            if colored {
                writeln!(w, r#"<basematerials id="{mat_id}">"#).map_err(io)?;
                for (i, o) in objects.iter().enumerate() {
                    let [r, g, b, a] = o.color.unwrap_or([0x9a, 0x9a, 0x9a, 0xff]);
                    let name = if o.name.is_empty() {
                        format!("part{}", i + 1)
                    } else {
                        esc(o.name)
                    };
                    writeln!(
                        w,
                        r##"<base name="{name}" displaycolor="#{r:02X}{g:02X}{b:02X}{a:02X}"/>"##
                    )
                    .map_err(io)?;
                }
                w.write_all(b"</basematerials>\n").map_err(io)?;
            }
            for (i, o) in objects.iter().enumerate() {
                let id = i as u32 + 1;
                write!(w, r#"<object id="{id}" type="model""#).map_err(io)?;
                if !o.name.is_empty() {
                    write!(w, r#" name="{}""#, esc(o.name)).map_err(io)?;
                }
                if colored {
                    write!(w, r#" pid="{mat_id}" pindex="{i}""#).map_err(io)?;
                }
                w.write_all(b">\n<mesh>\n<vertices>\n").map_err(io)?;
                for [x, y, z] in &o.mesh.vertices {
                    writeln!(w, r#"<vertex x="{x}" y="{y}" z="{z}"/>"#).map_err(io)?;
                }
                w.write_all(b"</vertices>\n<triangles>\n").map_err(io)?;
                for [a, b, c] in &o.mesh.triangles {
                    writeln!(w, r#"<triangle v1="{a}" v2="{b}" v3="{c}"/>"#).map_err(io)?;
                }
                w.write_all(b"</triangles>\n</mesh>\n</object>\n")
                    .map_err(io)?;
            }
            if assembly {
                writeln!(w, r#"<object id="{}" type="model">"#, n + 1).map_err(io)?;
                w.write_all(b"<components>\n").map_err(io)?;
                for id in 1..=n {
                    writeln!(w, r#"<component objectid="{id}"/>"#).map_err(io)?;
                }
                w.write_all(b"</components>\n</object>\n").map_err(io)?;
            }
            w.write_all(b"</resources>\n<build>\n").map_err(io)?;
            if assembly {
                writeln!(w, r#"<item objectid="{}"/>"#, n + 1).map_err(io)?;
            } else {
                for id in 1..=n {
                    writeln!(w, r#"<item objectid="{id}"/>"#).map_err(io)?;
                }
            }
            w.write_all(b"</build>\n</model>\n").map_err(io)?;
            w.flush().map_err(io)?;
        }
        let f = z
            .finish()
            .map_err(zerr)?
            .into_inner()
            .map_err(|e| io(e.into_error()))?;
        let len = f.metadata().map_err(io)?.len();
        drop(f);
        std::fs::rename(&part, out).map_err(io)?;
        Ok(len)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    Ok((tris, r?))
}

/// GLB bytes -> 3MF file; returns (triangles written, bytes).
pub fn glb_file_to_3mf(glb: &[u8], out: &Path) -> AssetResult<(u64, u64)> {
    write_3mf(&super::stl::glb_mesh(glb)?, out)
}
