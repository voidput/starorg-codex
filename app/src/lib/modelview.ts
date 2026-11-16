/**
 * The Explorer's Model tab (addendum 7.20), the parts without three.js: the detail slider
 * (left = least detail, right = LOD 0; see `sliderSteps`), the words for each step, fetching the GLB
 * from the asset cache in chunks, and the readouts. `ModelViewer.svelte` uses these; tests run
 * them in node (`tests/modelview.test.ts`).
 */
import type { AssetModelChunkRequest, AssetModelInfo, AssetModelLod, AssetModelView } from './contract/assets';
import { gbText } from './memory.ts';

export const MAX_LOD = 4;

/** The words for each LOD (the slider's labels). */
export const LOD_NAMES = ['Full', 'High', 'Medium', 'Low', 'Lowest'] as const;

/** "Low · LOD 3", "Full · LOD 0". */
export function lodLabel(lod: number): string {
  return `${LOD_NAMES[Math.min(MAX_LOD, Math.max(0, lod))]} · LOD ${lod}`;
}

/**
 * The slider's steps, least detail first. LOD 4 is not reliably lighter: StarBreaker loads LOD 0
 * for any part without a `_lod4` file, so on LIVE 4.10 a Gladius has 104k triangles at LOD 4
 * and 86k at LOD 3 (Carrack 402k vs 232k). The slider therefore runs LOD 3 -> LOD 0, and shows
 * LOD 4 only when the mesh has it and both GLBs are known with LOD 4 the lighter. A step the mesh
 * lacks (it shows the LOD below it) is left out once the LODs are known. Never empty.
 */
export function sliderSteps(info: Pick<AssetModelInfo, 'lods' | 'available'>): AssetModelLod[] {
  const lods = Array.isArray(info.lods) ? info.lods : [];
  const at = (l: number) => lods.find((x) => x.lod === l);
  const known = Array.isArray(info.available) && info.available.length > 0;
  const steps = lods.filter((s) => s.lod <= 3 && (!known || s.uses === s.lod));
  const four = at(4);
  const three = at(3);
  if (
    four &&
    three &&
    known &&
    info.available.includes(4) &&
    four.uses === 4 &&
    four.triangles !== null &&
    three.triangles !== null &&
    four.triangles < three.triangles
  )
    steps.push(four);
  if (steps.length === 0 && lods[0]) steps.push(lods[0]);
  return steps.sort((a, b) => b.lod - a.lod);
}

/** The slider position of `lod` among `steps` (the nearest step with at least its detail). */
export function stepIndex(steps: Pick<AssetModelLod, 'lod'>[], lod: number): number {
  const i = steps.findIndex((s) => s.lod <= lod);
  return i < 0 ? Math.max(0, steps.length - 1) : i;
}

/** "86k", "1.07M" triangles, short (under the slider). */
export function trianglesShort(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1000) return `${Math.round(n / 1000)}k`;
  return String(n);
}

/**
 * Whether this webview can draw WebGL, checked on a throwaway canvas before anything is exported
 * (the probe's context is released at once). `make` creates the canvas (tests pass a fake).
 */
export function webglAvailable(make: () => HTMLCanvasElement = () => document.createElement('canvas')): boolean {
  try {
    const c = make();
    const gl = (c.getContext('webgl2') ?? c.getContext('webgl')) as WebGLRenderingContext | null;
    if (!gl) return false;
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    return true;
  } catch {
    return false;
  }
}

/** Bytes as "4.6 MB" (0.1 MB steps under 10 MB). */
export function mbText(n: number): string {
  if (n < 1024 * 1024) return `${Math.max(1, Math.round(n / 1024))} KB`;
  const mb = n / (1024 * 1024);
  return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
}

export type StepState = 'cached' | 'export' | 'ask' | 'refused' | 'too-big';

/** What choosing this step does: show it from the cache, export it, ask first, or refuse. */
export function stepState(s: AssetModelLod, maxBytes: number): StepState {
  if (s.cached) return s.bytes !== null && s.bytes > maxBytes ? 'too-big' : 'cached';
  if (s.fit === 'over-limit') return 'refused';
  if (s.fit === 'low-free') return 'ask';
  return 'export';
}

/** One line under the slider for a step: memory, cache, the LOD it really shows. */
export function stepNote(s: AssetModelLod, info: Pick<AssetModelInfo, 'limit' | 'max_bytes'>): string {
  const parts: string[] = [];
  if (s.uses !== s.lod) parts.push(`the mesh has no LOD ${s.lod}: shows LOD ${s.uses}`);
  switch (stepState(s, info.max_bytes)) {
    case 'cached':
      parts.push(`in the cache${s.bytes !== null ? ` (${mbText(s.bytes)})` : ''}: no export needed`);
      break;
    case 'too-big':
      parts.push('too large for the viewer');
      break;
    case 'refused':
      parts.push(`needs about ${gbText(s.need)}, more than the${info.limit !== null ? ` ${gbText(info.limit)}` : ''} memory limit allows`);
      break;
    case 'ask':
      parts.push(`needs about ${gbText(s.need)}; not much memory is free now`);
      break;
    case 'export':
      parts.push(`exports in its own process, about ${gbText(s.need)} of memory for a second`);
      break;
  }
  const t = parts.join('; ');
  return t.charAt(0).toUpperCase() + t.slice(1) + '.';
}

/** "85,907 triangles", "1.07 M triangles". */
export function trianglesText(n: number | null): string {
  if (n === null) return 'triangles unknown';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)} M triangles`;
  return `${n.toLocaleString('en-US')} triangles`;
}

/** glTF is Y-up: x = width, y = height, z = length. "17.4 × 5.8 × 21.0 m (W × H × L)". */
export function sizeText(size: [number, number, number]): string {
  const f = (v: number) => (v >= 100 ? Math.round(v).toString() : v.toFixed(1));
  return `${f(size[0])} × ${f(size[1])} × ${f(size[2])} m`;
}

export interface FetchProgress {
  done: number;
  total: number;
}

export class FetchCancelled extends Error {
  constructor() {
    super('cancelled');
    this.name = 'FetchCancelled';
  }
}

/**
 * The view's GLB, chunk by chunk, into one buffer (no copies beyond the chunks). `isCancelled`
 * is checked between chunks; a short or empty chunk before the end is an error (the cache
 * changed under us).
 */
export async function fetchGlb(
  view: Pick<AssetModelView, 'what' | 'lod' | 'bytes' | 'chunk'>,
  target: AssetModelChunkRequest['target'],
  chunk: (req: AssetModelChunkRequest) => Promise<ArrayBuffer>,
  onProgress: (p: FetchProgress) => void = () => {},
  isCancelled: () => boolean = () => false,
): Promise<ArrayBuffer> {
  const out = new Uint8Array(view.bytes);
  let done = 0;
  onProgress({ done, total: view.bytes });
  while (done < view.bytes) {
    if (isCancelled()) throw new FetchCancelled();
    const part = new Uint8Array(await chunk({ target, what: view.what, lod: view.lod, offset: done, len: Math.max(1, view.chunk) }));
    if (part.byteLength === 0 || done + part.byteLength > view.bytes) throw new Error(`The cached model changed while it was read (${done} of ${view.bytes} bytes); load it again.`);
    out.set(part, done);
    done += part.byteLength;
    onProgress({ done, total: view.bytes });
  }
  if (isCancelled()) throw new FetchCancelled();
  return out.buffer;
}

/** Why there is no 3D view, in words, from what WebGL creation threw. */
export function webglProblem(e: unknown): string {
  const m = e instanceof Error ? e.message : String(e);
  return `The webview has no WebGL here (${m}), so it cannot draw the model. On some older WebKitGTK builds --safe-gpu or WEBKIT_DISABLE_COMPOSITING_MODE=1 turn 3D off; if you use them, try the app without them, or use "Export .glb" and open the file in Blender.`;
}
