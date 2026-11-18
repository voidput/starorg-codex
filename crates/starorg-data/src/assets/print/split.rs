//! Split (experimental): cut the voxel solid into pieces that each fit the bed. Pipeline
//! steps 9 (the cuts, planned before the pegs) and 10 of [`super::apply`].
//!
//! Owner: the split agent. [`plan`] and [`split`] are pass-through stubs (no cuts, one piece).

use super::Cut;
use super::voxel::VoxelGrid;

/// One piece of the split solid.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// `piece-1`, `piece-2`, ... (empty for an unsplit solid).
    pub name: String,
    pub grid: VoxelGrid,
}

/// TODO(split agent): the cuts that make every piece fit `bed` (mesh units, x/y/z), preferring
/// few cuts through thin sections. Stub: none.
pub fn plan(grid: &VoxelGrid, bed: [f32; 3]) -> Vec<Cut> {
    let _ = (grid, bed);
    Vec::new()
}

/// TODO(split agent): `grid` cut at `cuts` into named pieces (each cropped to its cells; empty
/// pieces left out). Stub: the whole grid as one unnamed piece.
pub fn split(grid: VoxelGrid, cuts: &[Cut]) -> Vec<Piece> {
    let _ = cuts;
    vec![Piece {
        name: String::new(),
        grid,
    }]
}
