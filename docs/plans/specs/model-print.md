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

### Scale, bed and small parts (`scale.rs`)

- **Scale**: none keeps real size in metres (3MF `unit="meter"`); fit-mm makes the longest
  side N mm; ratio 1:N gives mm = m × 1000 / N; shrink-to-bed alone starts from real size in
  mm. Any of these writes millimetres (3MF `unit="millimeter"`, STL numbers in mm). Fit-mm
  measures what is left after small parts are dropped.
- **Sit on the bed** (always, steps 4 and 13): lowest z = 0 and the middle of the footprint at
  x = y = 0, all parts moved together (each split piece on its own).
- **Bed check** (scaled models only; real size in metres is never checked): fits as is → no
  change; fits only turned 90° about Z → turned, with a note; fits neither way → with
  shrink-to-bed, scaled down (turned when that keeps it larger) with a note giving the
  percentage and the new size, else a note with the size and what to do (`split` → "the split
  cuts it into pieces that fit"). 1 µm slack for f32 rounding.
- **Drop small parts**: corners closer than 1/1000 of the size are welded (union-find over a
  hash grid), triangles joined by corners form a loose part, and parts whose bounding-box
  **diagonal** at print size is under N mm are left out ("Left out K parts smaller than N mm.").
  A small part inside or touching a bigger one without sharing a corner counts as loose. Real
  data: Skylark LOD 4 (98 376 tris) at fit 150 mm drops 655 parts (→ 94 844 tris) at 1 mm,
  906 at 3 mm; Carrack LOD 4 (401 933 tris) at fit 250 mm drops 16 020 parts (→ 278 622) at
  0.3 mm; 0.15 s and about 1 s in a release build.

## Pipeline

`print::apply(mesh, &PrintOptions, progress) -> AssetResult<PrintOutput>`, fixed order:

1. classify groups (multicolor only; otherwise groups are dropped)
2. drop tiny parts (measured at print size)
3. scale (`scale::factor`: real size stays metres, `unit="meter"`; any scale or shrink -> mm)
4. sit on the bed (min z = 0, x/y centred)
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
