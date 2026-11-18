//! The STL and 3MF written next to a Print shell GLB (addendum 7.16): the GLB's triangles
//! ([`super::stl::glb_mesh`]) written as [`super::stl`] and [`super::threemf`] files.

use super::AssetResult;
use super::model::ModelOptions;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One STL or 3MF file written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshFile {
    /// `stl` or `3mf`.
    pub kind: String,
    pub path: String,
    pub bytes: u64,
    pub triangles: u64,
}

/// The files written next to a GLB.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meshes {
    pub files: Vec<MeshFile>,
}

impl Meshes {
    /// Files written and their bytes.
    pub fn files_bytes(&self) -> (u32, u64) {
        files_bytes(&self.files)
    }
}

/// Count and bytes of `files`.
pub fn files_bytes(files: &[MeshFile]) -> (u32, u64) {
    (files.len() as u32, files.iter().map(|f| f.bytes).sum())
}

/// Where the STL and 3MF of the GLB `out` go (next to it), when asked.
pub fn mesh_outputs(out: &Path, o: &ModelOptions) -> [Option<PathBuf>; 2] {
    [
        o.stl.then(|| out.with_extension("stl")),
        o.three_mf.then(|| out.with_extension("3mf")),
    ]
}

/// Write the STL and/or 3MF `o` asks for next to the GLB `out`, from its bytes (the triangles
/// are read once for both).
pub fn write_meshes(glb: &[u8], out: &Path, o: &ModelOptions) -> AssetResult<Meshes> {
    let [stl, three_mf] = mesh_outputs(out, o);
    let mut m = Meshes::default();
    if stl.is_none() && three_mf.is_none() {
        return Ok(m);
    }
    let tris = super::stl::glb_mesh(glb)?;
    let file = |kind: &str, p: &Path, (triangles, bytes): (u64, u64)| MeshFile {
        kind: kind.into(),
        path: p.display().to_string(),
        bytes,
        triangles,
    };
    if let Some(p) = stl {
        let r = super::stl::write_stl(&tris, &p)?;
        m.files.push(file("stl", &p, r));
    }
    if let Some(p) = three_mf {
        let r = super::threemf::write_3mf(&tris, &p)?;
        m.files.push(file("3mf", &p, r));
    }
    Ok(m)
}
