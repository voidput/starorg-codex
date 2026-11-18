---
title: "Model export: 3D print processing (experimental)"
tags: [docs, plans, specs]
status: active
updated: 2026-11-17
---

# Model export: 3D print processing (experimental)

Experimental processing of the **STL/3MF** written next to a **Print shell** GLB
([addendum 7.16](starorg-codex-v1-addendum.md)). The GLB stays as exported (glTF Y-up,
metres); the STL/3MF are Z-up. Every option is **off by default** and labelled experimental
(UI badge "Experimental", CLI help "(experimental)"); the model always sits on the bed.
Only `no_weapons` changes the GLB (it is part of the cache key: `print-lod0-noweapons`).

## Options

`assets::print::PrintOptions` (serde, every field defaulted, one nested `print` object in
`AssetExportModelRequest` and `ModelOptions`; mirrored in `app/src/lib/contract/assets.ts`,
defaults and checks in `app/src/lib/printoptions.ts`).

| Option | Type, default | CLI | Needs |
|---|---|---|---|
| `scale` | `none` \| `fit-mm` (f32, UI default 150) \| `ratio` (u32, 1:N) | `--fit-mm N` \| `--scale-ratio N` (exclusive) | |
| `bed_mm` | `[f32; 3]`, `[256, 256, 256]` (remembered by the UI) | `--bed XxYxZ` | |
| `shrink_to_bed` | bool | `--shrink-to-bed` | |
| `drop_small_mm` | `Option<f32>` (1.0 when on) | `--drop-small MM` | |
| `solid` + `solid_res_mm` | bool + 0.1 \| 0.2 \| 0.4 (0.2) | `--solid [RES]` | a scale or `shrink_to_bed` |
| `thicken_mm` | `Option<f32>` (0.8) | `--thicken MM` | `solid` |
| `hollow` | `Option<{wall_mm 2.0, drain_mm 3.0}>` | `--hollow WALL[,DRAIN]` | `solid` |
| `peg_mm` | `Option<f32>`: 3 \| 5 \| 6, 8 mm deep | `--peg MM` | `split` |
| `split` | bool | `--split` | `solid` |
| `multicolor` | bool | `--multicolor` | |
| `simplify_tris` | `Option<u32>` (200000, at least 1000) | `--simplify N` | |
| `no_weapons` | bool (changes the GLB) | `--no-weapons` | |

`PrintOptions::validate` refuses values out of range and options without what they need; the
same messages come from the CLI (exit 2), the backends (`invalid`) and the TS check (the
dialog disables "Save as…"). Print options with `--preset full` are refused by the CLI and
ignored by the backends (as a Print shell ignores paints).

## Pipeline

`print::apply(mesh, &PrintOptions, progress) -> AssetResult<PrintOutput>`, fixed order:

1. classify groups (multicolor only; otherwise groups are dropped)
2. drop tiny parts (measured at print size)
3. scale (`scale::factor`: real size stays metres, `unit="meter"`; any scale or shrink -> mm)
4. sit on the bed (min z = 0)
5. bed check / shrink to the bed, sit on the bed
6. voxel solid, 7. thicken, 8. hollow + drain, 9. pegs (at `split::plan`'s cuts)
10. split into pieces
11. extract per group (`voxel::extract`, then `color::by_group`)
12. simplify (the triangle budget shared by part size)
13. sit on the bed again (each split piece alone, else all together)
14. write: one 3MF with every part (`color::layout`: one assembly for multicolor without
    split, else one build item per part; display colours as `basematerials`); one STL, or one
    STL per part for separate parts (`ship-piece-1.stl`)

Input: `Mesh::from_glb` (shared vertices, degenerate triangles dropped, each triangle's group is
its GLB node path such as `geo_body` or `hardpoint_weapon_left/mesh_gun`).

## Module ownership

Each feature agent edits **only its file** (and may add `#[cfg(test)] mod tests` inside it);
signatures below are final. Shared files (`print/mod.rs`, `threemf.rs`, `stl.rs`, the contract,
CLI, UI) belong to the foundation; ask before changing them.

| File | Owner | Implement |
|---|---|---|
| `crates/starorg-data/src/assets/print/scale.rs` | scale | `drop_tiny(&mut Mesh, min_size: f32) -> usize`, `bed_check(&mut Mesh, &PrintOptions, Unit, &mut Vec<String>) -> AssetResult<()>` (done: `factor`, `scale`, `sit_on_bed`) |
| `crates/starorg-data/src/assets/print/voxel.rs` | voxel | `voxelize(&Mesh, res, Progress) -> AssetResult<VoxelGrid>`, `thicken(&mut VoxelGrid, min, Progress) -> AssetResult<()>`, `hollow(&mut VoxelGrid, wall, drain, &mut Vec<String>, Progress) -> AssetResult<()>`, `peg(&mut VoxelGrid, &[Cut], diameter, depth, &mut Vec<String>) -> AssetResult<()>`, `extract(&VoxelGrid, Progress) -> AssetResult<Mesh>`; keeps the `VoxelGrid` API |
| `crates/starorg-data/src/assets/print/split.rs` | split | `plan(&VoxelGrid, bed: [f32; 3]) -> Vec<Cut>`, `split(VoxelGrid, &[Cut]) -> Vec<Piece>` |
| `crates/starorg-data/src/assets/print/color.rs` | color | `classify(&mut Mesh)`, `by_group(&Mesh) -> Vec<(String, Mesh)>`, `color_of(&str) -> Option<[u8; 4]>` (done: `layout`) |
| `crates/starorg-data/src/assets/print/simplify.rs` | simplify | `simplify(&mut Mesh, target: u32, Progress) -> AssetResult<()>` |
| `crates/starorg-data/src/assets/model.rs` (`strip_weapons` only) | weapons | `strip_weapons(&Database<'static>, &mut LoadoutTree) -> usize` |

Lengths passed to the steps are in mesh units (after step 3: millimetres, or metres at real
size); the pipeline converts the options' millimetres. `Progress` is
`&(dyn Fn(f32, &str) -> bool + Sync)`; false cancels (`AssetError::cancelled`).

Shared types (`print/mod.rs`): `Mesh { vertices, triangles, groups: Option<Vec<u32>>, labels }`
with `from_triangles`, `from_labeled`, `from_glb`, `bounds`, `soup`, `subset`,
`retain_triangles`, `compact`, `append`, `map_vertices`; `Aabb`; `Cut { axis, at }`;
`Part { name, mesh, color }`; `PrintOutput { parts, unit, layout, notes }`. `VoxelGrid`
(`voxel.rs`): `new`, `get`/`set`/`is_solid`, `center`, `cell_at`, `crop`, `cells(_mut)`,
`count_solid`; a cell is 0 (empty) or 1 + its group. `threemf::write_3mf_objects(&[Object],
Unit, Layout, path)`.

## Voxel solid (`voxel.rs`)

Robust over pretty: the output is always a closed, consistently outward 2-manifold.

- **Voxelize** (`voxelize`): every triangle marks the cells its slightly enlarged box overlaps
  (Akenine-Möller separating axes; per z-slab in parallel), so the surface has no 6-connected
  gaps. The outside is flood-filled (span fill on bitsets) from the padded border after a
  closing of `SEAL_CELLS` (2 voxels: sub-millimetre cracks between panels are sealed). Game
  hulls have larger openings, so a flood-reached cell stays outside only when it is in plain
  view along at least `OUTSIDE_VOTES` (3) of the six axis directions (ray stabbing); the inside
  of a hull with holes is seen through one or two. Trade-off: deep exterior pockets seen from
  fewer than three directions (between landing gear legs, bores) fill in. The solid is about
  half a voxel larger on each side. Grids above `CELL_BUDGET` (2^29 cells) are coarsened, with
  a note (`coarsened`). Each cell keeps its closest triangle's group (`1 + group`, at most 255).
- **Extract** (`extract`, `extract_where` for one group as its own closed mesh): the cells are
  made well-composed (no two solid or two empty cells meeting only at an edge or corner; fixed
  by adding cells), then each solid/empty face is a quad on shared grid corners; a clamped
  Laplacian smoothing (each vertex within 0.4 voxel of its corner, undone where a triangle would
  turn over) softens the staircase without changing topology. Triangle counts are high
  (millions at 0.2 mm); simplify is a separate step.
- **Thicken** (`thicken`): what an opening of radius t/2 removes, away from the rounded edges
  of thick parts, is dilated by t/2, so sheets become at least t thick; thin parts grow outwards
  on both sides and gaps narrower than t between them fill in.
- **Hollow** (`hollow`): cavities are the solid eroded by the wall (a discrete ball, chained in
  4-voxel steps); cavities under 1 mm³ stay solid; each cavity drains through a vertical hole
  from its lowest point (nearest its centre) down through the bottom wall, plus a second for a
  cavity wider than eight holes. The note lists cavities and hole positions.
- **Pegs** (`peg`): per cut, a hole at the most solid point (2D distance transform) of each piece
  of the cut face (up to 4; a second on a large face), `depth` into each side; plus a stand
  hole into the underside nearest the x/y centre of mass with room. Every hole is clamped to
  leave `PEG_CAP_MM` (0.6) of material, with a note when clamped, when the wall around is
  under `PEG_WALL_MM` (0.8), or when there is no room.
- Helpers for the split and colour steps: `Bits` (bitset with `dilate`/`erode`/`flood`/
  `components`), `VoxelGrid::bits`, `solid_bounds`, `grown`.

Real data (release, 32 threads; `--fit-mm 150`): Skylark LOD 4 (98 376 triangles in) at 0.2 mm:
0.56 s, 407 MB peak, 3.09 M triangles, watertight (every edge once each way), 223 cm³; 0.1 mm is
coarsened to 0.13 mm: 1.6 s, 1.15 GB, 7.6 M triangles; with `--thicken 0.8 --hollow 2,3`:
0.94 s, 9 cavities, 10 drain holes, 69 cm³, watertight. Gladius LOD 2 at 0.2 mm: 0.52 s,
278 MB, 1.83 M triangles, watertight.

## Split for the bed (`split.rs`)

Needs `--solid` (refused otherwise: "--split needs --solid (experimental)").

- **Plan** (`plan`): whole-grid axis-aligned planes, so the pieces are the cells of a lattice
  (the `Cut { axis, at }` the pegs also read). Each axis gets the fewest planes its solid
  extent needs for slabs of at most the bed (less half a voxel); pieces are not turned. Where
  the planes go is searched: per axis the positions of least total cut area (dynamic
  programming over the cut-face area: through thin sections) and the slabs slid from the low
  end to the high end (33 steps); every combination is scored when there are up to 4096, else
  axis by axis in each of the six axis orders. Score: fewest non-empty pieces, then fewest
  slivers (a piece under 2 % of the solid), then least cut area. Pieces are counted from
  per-row solid runs, so a plan scores in about a millisecond.
- **Pins** (`pins`, when `--peg` is not given; with `--peg` the pegs do this): per cut,
  `PIN_MM` (2 mm) holes `PIN_DEPTH_MM` (4 mm) into each side, two in the largest piece of the
  cut face (the most solid point by a chamfer distance transform, the second as far from it as
  the face allows, at least three diameters apart) and one in each of the next two pieces;
  only where the face has room, never through (`PEG_CAP_MM` left). One note per cut.
- **Split** (`split`): each lattice cell cropped to its solid cells, empty ones left out,
  named `part1`, `part2`, ... (x, then y, then z) and laid out side by side along x
  (`LAYOUT_GAP`, 5 mm), low y lined up; the pipeline then sits each on the bed. The STL is
  `<name>-part1.stl`, ... (one `MeshFile` each), the 3MF one object and build item per part.
  Without a needed cut: one unnamed piece, unchanged output. `summary` gives the note.

Real data (release, 32 threads): Gladius Print shell LOD 2 at 1:100, `--bed 120x120x120
--solid 0.2 --split`: 4.9 s in all for `dev print-glb` (STL + 3MF), 552 MB peak; cuts at
x -16.6 and y 24.2, 4 parts (70 x 109 x 51, 20 x 70 x 26, 103 x 109 x 57, 53 x 104 x 42 mm),
each watertight and within the bed, 2 pin holes per cut; with `--peg 5` the same cuts with
5 mm pegs and a stand hole; the 3MF has 4 build items.

## Testing on real GLBs

`starorg-codex dev print-glb <in.glb> --out <dir> [print flags] [--stl] [--3mf]` runs the
pipeline on an existing GLB (both files without `--stl`/`--3mf`), reading no game files; e.g.
on a Skylark LOD 4 Print shell (98 376 triangles) `--fit-mm 150` takes about 1 s in a debug build.
Notes from the steps are printed (and in `--json`, the app's export note).

## Status

Foundation done: options, contract in all four layers, CLI flags and `dev print-glb`, the
dialog's "3D print (experimental)" section, the writer (units, several objects, assemblies,
colours), `factor`/`scale`/`sit_on_bed`, `layout`. Stubs: the rest of the table above
(`voxelize`/`extract` refuse "not available in this build yet"; the others pass through).
