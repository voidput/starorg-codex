//! Multicolor (experimental): the triangles grouped by what they are (hull, glass, engines,
//! ...) from their GLB node paths, one 3MF object per group with a display colour, and how the
//! 3MF places the objects. Pipeline steps 1, 11 and 14 of [`super::apply`].
//!
//! Owner: the color agent. [`layout`] is done; [`classify`], [`by_group`] and [`color_of`]
//! are pass-through stubs.

use super::{Mesh, PrintOptions};
use crate::assets::threemf::Layout;

/// TODO(color agent): replace the mesh's groups (node paths from the GLB, e.g.
/// `geo_body`, `geo_cockpit_glass_upper_ext`) with colour groups and their labels (`hull`,
/// `glass`, ...). Stub: keeps the node groups.
pub fn classify(mesh: &mut Mesh) {
    let _ = mesh;
}

/// TODO(color agent): the mesh split by group, `(label, mesh)` per non-empty group (use
/// [`Mesh::subset`]). Stub: the whole mesh as one unlabelled part.
pub fn by_group(mesh: &Mesh) -> Vec<(String, Mesh)> {
    vec![(String::new(), mesh.clone())]
}

/// TODO(color agent): the display colour (RGBA) of a group label. Stub: none.
pub fn color_of(label: &str) -> Option<[u8; 4]> {
    let _ = label;
    None
}

/// How the 3MF places the parts: one assembly (one object with a part per colour) for
/// multicolor without split, else one build item per part (split pieces print apart).
pub fn layout(o: &PrintOptions) -> Layout {
    if o.multicolor && !o.split {
        Layout::Assembly
    } else {
        Layout::Separate
    }
}
