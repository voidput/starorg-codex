/**
 * The experimental 3D print options of a Print shell export (`assets::print::PrintOptions`):
 * their defaults and the same checks as `PrintOptions::validate` (same messages), for the
 * export dialog and the TS mock. Pure: no imports but types.
 */
import type { PrintOptions, ScaleMode } from './contract/assets';

export const DEFAULT_FIT_MM = 150;
export const DEFAULT_BED_MM: [number, number, number] = [256, 256, 256];
export const DEFAULT_DROP_SMALL_MM = 1;
export const SOLID_RES_MM = [0.1, 0.2, 0.4] as const;
export const DEFAULT_SOLID_RES_MM = 0.2;
export const DEFAULT_THICKEN_MM = 0.8;
export const DEFAULT_WALL_MM = 2;
export const DEFAULT_DRAIN_MM = 3;
export const PEG_MM = [3, 5, 6] as const;
export const DEFAULT_PEG_MM = 5;
export const PEG_DEPTH_MM = 8;
export const DEFAULT_SIMPLIFY_TRIS = 200000;
export const MIN_SIMPLIFY_TRIS = 1000;
export const MAX_MM = 10000;

export function defaultPrintOptions(): PrintOptions {
  return {
    scale: { mode: 'none' },
    bed_mm: [...DEFAULT_BED_MM],
    shrink_to_bed: false,
    drop_small_mm: null,
    solid: false,
    solid_res_mm: DEFAULT_SOLID_RES_MM,
    thicken_mm: null,
    hollow: null,
    peg_mm: null,
    split: false,
    multicolor: false,
    simplify_tris: null,
    no_weapons: false,
  };
}

const sameScale = (a: ScaleMode, b: ScaleMode) => a.mode === b.mode && (a.mode === 'none' || (b.mode !== 'none' && a.value === b.value));

/** Whether every option is off (the request may leave `print` out). */
export function isDefaultPrint(p: PrintOptions): boolean {
  const d = defaultPrintOptions();
  return (
    sameScale(p.scale, d.scale) &&
    p.bed_mm.every((v, i) => v === d.bed_mm[i]) &&
    p.shrink_to_bed === d.shrink_to_bed &&
    p.drop_small_mm === null &&
    p.solid === d.solid &&
    Math.abs(p.solid_res_mm - d.solid_res_mm) < 1e-6 &&
    p.thicken_mm === null &&
    p.hollow === null &&
    p.peg_mm === null &&
    p.split === d.split &&
    p.multicolor === d.multicolor &&
    p.simplify_tris === null &&
    p.no_weapons === d.no_weapons
  );
}

const positive = (v: number) => Number.isFinite(v) && v > 0;
const oneOf = (v: number, set: readonly number[]) => set.some((s) => Math.abs(v - s) < 1e-4);
/** Rust's `{v}` of an f32 (whole numbers without a fraction). */
const num = (v: number) => String(Math.round(v * 1e4) / 1e4);

/** `PrintOptions::validate`: null when the options are fine, else why not (Rust's wording). */
export function printProblem(p: PrintOptions): string | null {
  const s = p.scale;
  if (s.mode === 'fit-mm' && !(positive(s.value) && s.value <= MAX_MM)) return `fit size ${num(s.value)} mm is out of range (more than 0, at most ${MAX_MM})`;
  if (s.mode === 'ratio' && !(Number.isInteger(s.value) && s.value >= 1)) return 'scale ratio must be 1 or more (1:N)';
  if (!p.bed_mm.every((v) => positive(v) && v <= MAX_MM)) return `bed size [${p.bed_mm.map((v) => (Number.isInteger(v) ? `${v}.0` : num(v))).join(', ')}] mm is out of range (each side more than 0, at most ${MAX_MM})`;
  if (p.drop_small_mm !== null && !positive(p.drop_small_mm)) return `drop-small size ${num(p.drop_small_mm)} mm must be more than 0`;
  if (!oneOf(p.solid_res_mm, SOLID_RES_MM)) return `solid resolution ${num(p.solid_res_mm)} mm must be 0.1, 0.2 or 0.4`;
  if (p.thicken_mm !== null && !positive(p.thicken_mm)) return `thicken size ${num(p.thicken_mm)} mm must be more than 0`;
  if (p.hollow) {
    if (!positive(p.hollow.wall_mm)) return `hollow wall ${num(p.hollow.wall_mm)} mm must be more than 0`;
    if (!(Number.isFinite(p.hollow.drain_mm) && p.hollow.drain_mm >= 0)) return `drain hole ${num(p.hollow.drain_mm)} mm must be 0 (none) or more`;
  }
  if (p.peg_mm !== null && !oneOf(p.peg_mm, PEG_MM)) return `peg size ${num(p.peg_mm)} mm must be 3, 5 or 6`;
  if (p.simplify_tris !== null && p.simplify_tris < MIN_SIMPLIFY_TRIS) return `simplify to ${p.simplify_tris} triangles: at least ${MIN_SIMPLIFY_TRIS}`;
  if (!p.solid) {
    const flag = p.thicken_mm !== null ? '--thicken' : p.hollow ? '--hollow' : p.peg_mm !== null ? '--peg' : p.split ? '--split' : null;
    if (flag) return `${flag} needs --solid (experimental)`;
  }
  if (p.solid && s.mode === 'none' && !p.shrink_to_bed) return '--solid needs --fit-mm, --scale-ratio or --shrink-to-bed (a real-size ship is metres long)';
  return null;
}

/** Where the export dialog remembers the bed size (per viewer). */
export const BED_KEY = 'starorg.print.bed.v1';

/** `"256x256x256"` -> [256, 256, 256]; null when not three positive sizes up to MAX_MM. */
export function parseBed(raw: string | null): [number, number, number] | null {
  if (!raw) return null;
  const v = raw.split('x').map(Number);
  if (v.length !== 3 || !v.every((n) => Number.isFinite(n) && n > 0 && n <= MAX_MM)) return null;
  return [v[0]!, v[1]!, v[2]!];
}

/** The remembered bed size, else the default (blocked storage reads as empty). */
export function loadBed(): [number, number, number] {
  try {
    return parseBed(localStorage.getItem(BED_KEY)) ?? [...DEFAULT_BED_MM];
  } catch {
    return [...DEFAULT_BED_MM];
  }
}

export function saveBed(bed: readonly number[]) {
  try {
    localStorage.setItem(BED_KEY, bed.join('x'));
  } catch {
    /* storage blocked */
  }
}
