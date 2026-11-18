//! GLB -> 3MF (3D Manufacturing Format, core spec) in pure Rust, for slicers (PrusaSlicer,
//! Bambu Studio, Cura). The triangles are [`super::stl::glb_triangles`]'s (node transforms
//! baked, Z-up); vertices are shared (exact `f32` equality) so the mesh stays connected.
//! `unit="meter"`: StarBreaker's GLB is in game units, which are metres, so a slicer shows the
//! real size. The package is a deflated zip with `[Content_Types].xml`, `_rels/.rels` and
//! `3D/3dmodel.model`; the model XML is streamed into the zip, never held whole.

use super::{AssetError, AssetResult};
use std::collections::HashMap;
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

const MODEL_HEAD: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<model unit="meter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">"#,
    "\n",
    r#"<metadata name="Application">StarOrg Codex (StarBreaker GLB, Z-up)</metadata>"#,
    "\n<resources>\n",
    r#"<object id="1" type="model">"#,
    "\n<mesh>\n<vertices>\n"
);

const MODEL_TAIL: &str = concat!(
    "</triangles>\n</mesh>\n</object>\n</resources>\n",
    "<build>\n",
    r#"<item objectid="1"/>"#,
    "\n</build>\n</model>\n"
);

/// A shared-vertex mesh: unique vertices and triangles indexing them.
pub struct IndexedMesh {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Share vertices by exact `f32` bits (`-0.0` counts as `0.0`). Triangles with a non-finite
/// coordinate or two equal corners are dropped (3MF requires three distinct vertices).
pub fn index_mesh(tris: &[[[f32; 3]; 3]]) -> IndexedMesh {
    let mut ids: HashMap<[u32; 3], u32> = HashMap::with_capacity(tris.len());
    let mut vertices = Vec::with_capacity(tris.len());
    let mut triangles = Vec::with_capacity(tris.len());
    for t in tris {
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
        }
    }
    IndexedMesh {
        vertices,
        triangles,
    }
}

/// Write a 3MF package of `tris`; returns (triangles written, bytes).
pub fn write_3mf(tris: &[[[f32; 3]; 3]], out: &Path) -> AssetResult<(u64, u64)> {
    let mesh = index_mesh(tris);
    if mesh.triangles.is_empty() {
        return Err(AssetError::invalid("the model has no triangles"));
    }
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
        let guess = 64 * (mesh.vertices.len() as u64 + mesh.triangles.len() as u64);
        z.start_file("3D/3dmodel.model", opts.large_file(guess > u32::MAX as u64))
            .map_err(zerr)?;
        {
            let mut w = std::io::BufWriter::with_capacity(1 << 16, &mut z);
            w.write_all(MODEL_HEAD.as_bytes()).map_err(io)?;
            for [x, y, z] in &mesh.vertices {
                writeln!(w, r#"<vertex x="{x}" y="{y}" z="{z}"/>"#).map_err(io)?;
            }
            w.write_all(b"</vertices>\n<triangles>\n").map_err(io)?;
            for [a, b, c] in &mesh.triangles {
                writeln!(w, r#"<triangle v1="{a}" v2="{b}" v3="{c}"/>"#).map_err(io)?;
            }
            w.write_all(MODEL_TAIL.as_bytes()).map_err(io)?;
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
    Ok((mesh.triangles.len() as u64, r?))
}

/// GLB bytes -> 3MF file; returns (triangles written, bytes).
pub fn glb_file_to_3mf(glb: &[u8], out: &Path) -> AssetResult<(u64, u64)> {
    write_3mf(&super::stl::glb_mesh(glb)?, out)
}
