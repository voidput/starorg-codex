//! GLB -> binary STL in pure Rust: every triangle primitive of every node in the default
//! scene, with the node transforms baked in (what `sc-ship-models`' `glb_to_stl_trimesh.py`
//! does with trimesh). glTF's Y-up axes are turned to STL's usual Z-up.

use super::{AssetError, AssetResult};
use std::io::Write;
use std::path::Path;

type Mat = [[f32; 4]; 4];

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut m = [[0f32; 4]; 4];
    for (c, col) in m.iter_mut().enumerate() {
        for (r, v) in col.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

fn apply(m: &Mat, p: [f32; 3]) -> [f32; 3] {
    let x = m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0];
    let y = m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1];
    let z = m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2];
    // Y-up -> Z-up.
    [x, -z, y]
}

const IDENTITY: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn tri_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 0.0 {
        [n[0] / l, n[1] / l, n[2] / l]
    } else {
        [0.0; 3]
    }
}

/// Triangles of a GLB, transforms baked.
pub fn glb_triangles(glb: &[u8]) -> AssetResult<Vec<[[f32; 3]; 3]>> {
    let mut tris = Vec::new();
    walk_glb(glb, |_, t| tris.push(t))?;
    Ok(tris)
}

/// The triangles of a GLB with the node each comes from ([`glb_triangles`]' order).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabeledTriangles {
    pub triangles: Vec<[[f32; 3]; 3]>,
    /// Per triangle, its node in `nodes`.
    pub node: Vec<u32>,
    /// Node paths from the scene root, `/`-joined node (else mesh) names (`geo_body`,
    /// `hardpoint_weapon_left/mesh_gun`); one per node that has triangles, in first-seen order.
    pub nodes: Vec<String>,
}

/// [`glb_triangles`] with the node path of every triangle (for grouping, e.g. multicolor).
pub fn glb_labeled_triangles(glb: &[u8]) -> AssetResult<LabeledTriangles> {
    let mut out = LabeledTriangles::default();
    let mut ids: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut last: Option<(String, u32)> = None;
    walk_glb(glb, |path, t| {
        let id = match &last {
            Some((p, id)) if p == path => *id,
            _ => {
                let id = *ids.entry(path.to_string()).or_insert_with(|| {
                    out.nodes.push(path.to_string());
                    (out.nodes.len() - 1) as u32
                });
                last = Some((path.to_string(), id));
                id
            }
        };
        out.triangles.push(t);
        out.node.push(id);
    })?;
    Ok(out)
}

/// Every triangle of the default scene's triangle primitives, transforms baked, Z-up, with
/// the path of the node it belongs to.
fn walk_glb(glb: &[u8], mut f: impl FnMut(&str, [[f32; 3]; 3])) -> AssetResult<()> {
    let g = gltf::Gltf::from_slice(glb).map_err(|e| AssetError::invalid(format!("GLB: {e}")))?;
    let blob = g.blob.as_deref();
    let mut stack: Vec<(gltf::Node, Mat, String)> = Vec::new();
    let roots: Vec<gltf::Node> = match g.default_scene().or_else(|| g.scenes().next()) {
        Some(s) => s.nodes().collect(),
        None => g.nodes().collect(),
    };
    let name = |n: &gltf::Node| -> String {
        n.name()
            .or_else(|| n.mesh().and_then(|m| m.name()))
            .map(str::to_string)
            .unwrap_or_else(|| format!("node{}", n.index()))
    };
    for n in roots {
        let p = name(&n);
        stack.push((n, IDENTITY, p));
    }
    while let Some((node, parent, path)) = stack.pop() {
        let world = mul(&parent, &node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = prim.reader(|b| match b.source() {
                    gltf::buffer::Source::Bin => blob,
                    gltf::buffer::Source::Uri(_) => None,
                });
                let Some(pos) = reader.read_positions() else {
                    continue;
                };
                let pos: Vec<[f32; 3]> = pos.map(|p| apply(&world, p)).collect();
                let idx: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..pos.len() as u32).collect(),
                };
                for t in idx.as_chunks::<3>().0 {
                    let get = |i: u32| pos.get(i as usize).copied();
                    if let (Some(a), Some(b), Some(c)) = (get(t[0]), get(t[1]), get(t[2])) {
                        f(&path, [a, b, c]);
                    }
                }
            }
        }
        for c in node.children() {
            let p = format!("{path}/{}", name(&c));
            stack.push((c, world, p));
        }
    }
    Ok(())
}

/// Triangles of a GLB without reading its geometry: the index (or vertex) counts of every
/// node's triangle primitives, from the JSON chunk only. `None` when it is not a GLB.
pub fn glb_triangle_count(glb: &[u8]) -> Option<u64> {
    let g = gltf::Glb::from_slice(glb).ok()?;
    json_triangle_count(&g.json)
}

/// [`glb_triangle_count`] of a GLB file, reading its header and JSON chunk only.
pub fn glb_file_triangle_count(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 20];
    f.read_exact(&mut head).ok()?;
    let word = |i: usize| u32::from_le_bytes(head[i..i + 4].try_into().unwrap_or_default());
    if &head[0..4] != b"glTF" || word(4) != 2 || &head[16..20] != b"JSON" {
        return None;
    }
    let len = word(12) as u64;
    if len > word(8) as u64 {
        return None;
    }
    let mut json = Vec::new();
    f.take(len).read_to_end(&mut json).ok()?;
    json_triangle_count(&json)
}

fn json_triangle_count(json: &[u8]) -> Option<u64> {
    let doc =
        gltf::Document::from_json_without_validation(gltf::json::Root::from_slice(json).ok()?);
    let roots: Vec<gltf::Node> = match doc.default_scene().or_else(|| doc.scenes().next()) {
        Some(s) => s.nodes().collect(),
        None => doc.nodes().collect(),
    };
    let mut stack = roots;
    let mut n = 0u64;
    while let Some(node) = stack.pop() {
        if let Some(mesh) = node.mesh() {
            for p in mesh.primitives() {
                if p.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let count = match p.indices() {
                    Some(i) => i.count(),
                    None => p.get(&gltf::Semantic::Positions).map_or(0, |a| a.count()),
                };
                n += count as u64 / 3;
            }
        }
        stack.extend(node.children());
    }
    Some(n)
}

/// Write a binary STL; returns (triangles, bytes).
pub fn write_stl(tris: &[[[f32; 3]; 3]], out: &Path) -> AssetResult<(u64, u64)> {
    let io = |e: std::io::Error| AssetError::internal(format!("{}: {e}", out.display()));
    let (part, file) = super::new_part(out)?;
    let r = (|| {
        let mut w = std::io::BufWriter::new(file);
        let mut header = [0u8; 80];
        let text = b"StarOrg Codex STL (StarBreaker GLB, Z-up)";
        header[..text.len()].copy_from_slice(text);
        w.write_all(&header).map_err(io)?;
        w.write_all(&(tris.len() as u32).to_le_bytes())
            .map_err(io)?;
        for t in tris {
            let n = tri_normal(t[0], t[1], t[2]);
            for v in std::iter::once(&n).chain(t.iter()) {
                for c in v {
                    w.write_all(&c.to_le_bytes()).map_err(io)?;
                }
            }
            w.write_all(&[0, 0]).map_err(io)?;
        }
        w.flush().map_err(io)?;
        drop(w);
        std::fs::rename(&part, out).map_err(io)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r?;
    Ok((tris.len() as u64, 84 + 50 * tris.len() as u64))
}

/// [`glb_triangles`], refusing a model without any (what the STL and 3MF writers start from).
pub fn glb_mesh(glb: &[u8]) -> AssetResult<Vec<[[f32; 3]; 3]>> {
    let tris = glb_triangles(glb)?;
    if tris.is_empty() {
        return Err(AssetError::invalid("the model has no triangles"));
    }
    Ok(tris)
}

/// GLB bytes -> STL file.
pub fn glb_file_to_stl(glb: &[u8], out: &Path) -> AssetResult<(u64, u64)> {
    write_stl(&glb_mesh(glb)?, out)
}
