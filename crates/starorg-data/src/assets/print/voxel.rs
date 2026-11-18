//! Voxel solid (experimental): the model rebuilt as one closed, printable solid from voxels,
//! then thickened, hollowed (with a drain hole) and given peg holes across the split cuts;
//! [`extract`] turns a grid back into a mesh. Pipeline steps 6-9 and 11 of [`super::apply`].
//!
//! Owner: the voxel agent. [`VoxelGrid`] is the shared API (split and peg code use it; keep
//! its methods, change the storage if needed). [`voxelize`] and [`extract`] are stubs that
//! refuse; [`thicken`], [`hollow`] and [`peg`] are pass-through stubs.

use super::{Cut, Mesh, Progress};
use crate::assets::{AssetError, AssetResult};

/// The most cells a grid may have (dense `u8`: about 1 GB).
pub const MAX_CELLS: u64 = 1 << 30;

/// A dense grid of cubic cells. A cell holds 0 (empty) or `1 + group` (solid, of the group
/// `labels[group]`; 1 when the mesh had no groups).
#[derive(Debug, Clone, PartialEq)]
pub struct VoxelGrid {
    /// The low corner of cell (0, 0, 0), mesh units.
    pub origin: [f32; 3],
    /// The edge of one cell, mesh units.
    pub voxel: f32,
    pub dims: [u32; 3],
    /// The group names cells refer to (the source mesh's `labels`).
    pub labels: Vec<String>,
    cells: Vec<u8>,
}

impl VoxelGrid {
    /// An empty grid; refused above [`MAX_CELLS`].
    pub fn new(origin: [f32; 3], voxel: f32, dims: [u32; 3]) -> AssetResult<Self> {
        let n = dims.iter().map(|d| *d as u64).product::<u64>();
        if n > MAX_CELLS {
            return Err(AssetError::invalid(format!(
                "the solid would need {n} voxels (at most {MAX_CELLS}): use a coarser resolution or a smaller size"
            )));
        }
        if !(voxel.is_finite() && voxel > 0.0) {
            return Err(AssetError::invalid("the voxel size must be more than 0"));
        }
        Ok(Self {
            origin,
            voxel,
            dims,
            labels: Vec::new(),
            cells: vec![0; n as usize],
        })
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The index of cell (x, y, z) in [`VoxelGrid::cells`] (x fastest).
    pub fn index(&self, x: u32, y: u32, z: u32) -> usize {
        (x as usize) + self.dims[0] as usize * (y as usize + self.dims[1] as usize * z as usize)
    }

    /// The cell (0 outside the grid).
    pub fn get(&self, x: u32, y: u32, z: u32) -> u8 {
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] {
            return 0;
        }
        self.cells[self.index(x, y, z)]
    }

    /// Set a cell (ignored outside the grid).
    pub fn set(&mut self, x: u32, y: u32, z: u32, v: u8) {
        if x < self.dims[0] && y < self.dims[1] && z < self.dims[2] {
            let i = self.index(x, y, z);
            self.cells[i] = v;
        }
    }

    pub fn is_solid(&self, x: u32, y: u32, z: u32) -> bool {
        self.get(x, y, z) != 0
    }

    pub fn cells(&self) -> &[u8] {
        &self.cells
    }

    pub fn cells_mut(&mut self) -> &mut [u8] {
        &mut self.cells
    }

    /// Solid cells.
    pub fn count_solid(&self) -> u64 {
        self.cells.iter().filter(|c| **c != 0).count() as u64
    }

    /// The centre of cell (x, y, z), mesh units.
    pub fn center(&self, x: u32, y: u32, z: u32) -> [f32; 3] {
        let c = [x, y, z];
        [0, 1, 2].map(|i| self.origin[i] + (c[i] as f32 + 0.5) * self.voxel)
    }

    /// The cell holding point `p` (`None` outside).
    pub fn cell_at(&self, p: [f32; 3]) -> Option<[u32; 3]> {
        let mut c = [0u32; 3];
        for i in 0..3 {
            let f = ((p[i] - self.origin[i]) / self.voxel).floor();
            if !(f >= 0.0 && f < self.dims[i] as f32) {
                return None;
            }
            c[i] = f as u32;
        }
        Some(c)
    }

    /// The cells `min..max` (exclusive, clamped) as a grid of their own, same labels.
    pub fn crop(&self, min: [u32; 3], max: [u32; 3]) -> AssetResult<VoxelGrid> {
        let max = [0, 1, 2].map(|i| max[i].min(self.dims[i]));
        let min = [0, 1, 2].map(|i| min[i].min(max[i]));
        let dims = [0, 1, 2].map(|i| max[i] - min[i]);
        let origin = [0, 1, 2].map(|i| self.origin[i] + min[i] as f32 * self.voxel);
        let mut g = VoxelGrid::new(origin, self.voxel, dims)?;
        g.labels = self.labels.clone();
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let v = self.get(min[0] + x, min[1] + y, min[2] + z);
                    if v != 0 {
                        g.set(x, y, z, v);
                    }
                }
            }
        }
        Ok(g)
    }
}

/// TODO(voxel agent): fill the inside of `mesh` (mesh units, millimetres) with cells of edge
/// `res`, labelled by the mesh's groups (most covering triangle); open and self-intersecting
/// ship meshes must still give a closed solid. Stub: refused.
pub fn voxelize(mesh: &Mesh, res: f32, progress: Progress<'_>) -> AssetResult<VoxelGrid> {
    let _ = (mesh, res, progress);
    Err(AssetError::invalid(
        "the solid (experimental) is not available in this build yet",
    ))
}

/// TODO(voxel agent): grow the solid so no part is thinner than `min` (mesh units). Stub: no-op.
pub fn thicken(grid: &mut VoxelGrid, min: f32, progress: Progress<'_>) -> AssetResult<()> {
    let _ = (grid, min, progress);
    Ok(())
}

/// TODO(voxel agent): hollow the solid, keeping walls `wall` thick, with a drain hole of
/// diameter `drain` (0 = none) through the bottom of each cavity (say where in `notes`).
/// Stub: no-op.
pub fn hollow(
    grid: &mut VoxelGrid,
    wall: f32,
    drain: f32,
    notes: &mut Vec<String>,
    progress: Progress<'_>,
) -> AssetResult<()> {
    let _ = (grid, wall, drain, notes, progress);
    Ok(())
}

/// TODO(voxel agent): peg holes of diameter `diameter`, `depth` deep into each side, across
/// every cut of `cuts` (from [`super::split::plan`]), where the cut face is solid enough (a
/// note per cut without room). Stub: no-op.
pub fn peg(
    grid: &mut VoxelGrid,
    cuts: &[Cut],
    diameter: f32,
    depth: f32,
    notes: &mut Vec<String>,
) -> AssetResult<()> {
    let _ = (grid, cuts, diameter, depth, notes);
    Ok(())
}

/// TODO(voxel agent): the surface of the solid cells as a closed, manifold mesh (mesh units),
/// with `groups`/`labels` from the cells' groups. Stub: refused.
pub fn extract(grid: &VoxelGrid, progress: Progress<'_>) -> AssetResult<Mesh> {
    let _ = (grid, progress);
    Err(AssetError::invalid(
        "the solid (experimental) is not available in this build yet",
    ))
}
