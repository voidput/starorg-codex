//! Simplify (experimental): reduce a part's triangles to a budget, keeping its shape (and its
//! groups). Pipeline step 12 of [`super::apply`], which shares `simplify_tris` among the parts
//! by their triangle counts.
//!
//! Owner: the simplify agent. [`simplify`] is a pass-through stub.

use super::{Mesh, Progress};
use crate::assets::AssetResult;

/// TODO(simplify agent): reduce `mesh` to about `target` triangles (never more than it has),
/// keeping it closed where it was. Stub: no-op.
pub fn simplify(mesh: &mut Mesh, target: u32, progress: Progress<'_>) -> AssetResult<()> {
    let _ = (mesh, target, progress);
    Ok(())
}
