/**
 * The Explorer's Model tab (addendum 7.20), the parts without three.js: the detail slider
 * (left = least detail, LOD 4; right = most, LOD 0), the words for each step, fetching the GLB
 * from the asset cache in chunks, and the readouts. `ModelViewer.svelte` uses these; tests run
 * them in node (`tests/modelview.test.ts`).
 */
import type { AssetModelChunkRequest, AssetModelInfo, AssetModelLod, AssetModelView } from './contract/assets';
import { gbText } from './memory.ts';

export const MAX_LOD = 4;

/** Slider position (0 = least detail) -> LOD (4 = least detail). */
export const sliderToLod = (v: number): number => MAX_LOD - Math.min(MAX_LOD, Math.max(0, Math.round(v)));
export const lodToSlider = (lod: number): number => MAX_LOD - Math.min(MAX_LOD, Math.max(0, lod));

/** "LOD 4 · least detail", "LOD 2", "LOD 0 · most detail". */
export function lodLabel(lod: number): string {
  if (lod === MAX_LOD) return `LOD ${lod} · least detail`;
  if (lod === 0) return 'LOD 0 · most detail';
  return `LOD ${lod}`;
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
  return `The webview has no WebGL here (${m}), so it cannot draw the model. With --safe-gpu or WEBKIT_DISABLE_COMPOSITING_MODE=1 the Linux webview runs without 3D; start the app without them, or use "Export .glb" and open the file in Blender.`;
}
