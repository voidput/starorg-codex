/**
 * The JSON tree of the Developer mode screens (addendum 7.4, 7.8), as pure functions: visible
 * lines for a set of open nodes (the component renders only the lines in view), the tree keys
 * (Up/Down, Right/Left, Home/End, `*`, Enter on a reference), search inside the value, and
 * splicing in a node that was cut by the backend (`{"_cut": {...}}`) once it is loaded.
 */

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

export interface RefInfo {
  guid: string;
  name: string | null;
  type: string | null;
  path: string | null;
}

export interface JsonLine {
  /** JSON pointer of the node; `''` is the root. */
  pointer: string;
  depth: number;
  /** Property name or array index; null for the root. */
  key: string | null;
  kind: 'object' | 'array' | 'value' | 'ref' | 'cut';
  value: Json;
  open: boolean;
  /** Children (containers) or the cut node's length. */
  size: number;
  ref: RefInfo | null;
  cutKind: 'object' | 'array' | null;
  parent: string | null;
}

const isObj = (v: unknown): v is { [k: string]: Json } => typeof v === 'object' && v !== null && !Array.isArray(v);
export const esc = (k: string) => k.replace(/~/g, '~0').replace(/\//g, '~1');
export const unesc = (k: string) => k.replace(/~1/g, '/').replace(/~0/g, '~');

export function refOf(v: Json): RefInfo | null {
  if (!isObj(v) || typeof v['_ref'] !== 'string') return null;
  const s = (x: Json | undefined) => (typeof x === 'string' ? x : null);
  return { guid: v['_ref'], name: s(v['name']), type: s(v['type']), path: s(v['path']) };
}

export function cutOf(v: Json): { kind: 'object' | 'array'; len: number } | null {
  if (!isObj(v) || Object.keys(v).length !== 1 || !isObj(v['_cut'])) return null;
  const c = v['_cut'];
  return c['kind'] === 'object' || c['kind'] === 'array' ? { kind: c['kind'], len: Number(c['len'] ?? 0) } : null;
}

function entries(v: Json): [string, Json][] {
  if (Array.isArray(v)) return v.map((x, i) => [String(i), x]);
  if (isObj(v)) return Object.entries(v);
  return [];
}

function lineOf(v: Json, pointer: string, depth: number, key: string | null, parent: string | null, open: Set<string>): JsonLine {
  const ref = refOf(v);
  const cut = cutOf(v);
  const kind: JsonLine['kind'] = ref ? 'ref' : cut ? 'cut' : Array.isArray(v) ? 'array' : isObj(v) ? 'object' : 'value';
  const size = cut ? cut.len : kind === 'array' || kind === 'object' ? entries(v).length : 0;
  return { pointer, depth, key, kind, value: v, open: (kind === 'object' || kind === 'array') && size > 0 && open.has(pointer), size, ref, cutKind: cut?.kind ?? null, parent };
}

/** Lines shown for the open set (the root is always open). */
export function visibleLines(root: Json, open: Set<string>): JsonLine[] {
  const out: JsonLine[] = [];
  const all = new Set(open).add('');
  const go = (v: Json, pointer: string, depth: number, key: string | null, parent: string | null) => {
    const l = lineOf(v, pointer, depth, key, parent, all);
    out.push(l);
    if (!l.open) return;
    for (const [k, x] of entries(v)) go(x, `${pointer}/${esc(k)}`, depth + 1, k, pointer);
  };
  go(root, '', 0, null, null);
  return out;
}

/** Pointers of every container down to `levels` levels (the first two levels open, 7.4). */
export function initialOpen(root: Json, levels = 2): Set<string> {
  const out = new Set<string>();
  const go = (v: Json, pointer: string, depth: number) => {
    if (depth >= levels || refOf(v) || cutOf(v)) return;
    if (Array.isArray(v) || isObj(v)) {
      out.add(pointer);
      for (const [k, x] of entries(v)) go(x, `${pointer}/${esc(k)}`, depth + 1);
    }
  };
  go(root, '', 0);
  return out;
}

export function at(root: Json, pointer: string): Json | undefined {
  if (pointer === '') return root;
  let cur: Json | undefined = root;
  for (const raw of pointer.slice(1).split('/')) {
    const k = unesc(raw);
    if (Array.isArray(cur)) cur = cur[Number(k)];
    else if (isObj(cur)) cur = cur[k];
    else return undefined;
    if (cur === undefined) return undefined;
  }
  return cur;
}

/** Replace the node at `pointer` (a cut stub) with the loaded value. Returns a new root. */
export function splice(root: Json, pointer: string, value: Json): Json {
  if (pointer === '') return value;
  const parts = pointer.slice(1).split('/').map(unesc);
  const go = (v: Json, i: number): Json => {
    const k = parts[i]!;
    const last = i === parts.length - 1;
    if (Array.isArray(v)) {
      const c = v.slice();
      c[Number(k)] = last ? value : go(v[Number(k)] ?? null, i + 1);
      return c;
    }
    if (isObj(v)) return { ...v, [k]: last ? value : go(v[k] ?? null, i + 1) };
    return v;
  };
  return go(root, 0);
}

export function ancestors(pointer: string): string[] {
  const out = [''];
  const parts = pointer.split('/').slice(1);
  for (let i = 1; i < parts.length; i++) out.push('/' + parts.slice(0, i).join('/'));
  return pointer === '' ? [] : out;
}

/** Display text of a scalar: strings quoted as JSON. */
export function scalarText(v: Json): string {
  return typeof v === 'string' ? JSON.stringify(v) : String(v);
}

/** Text Ctrl+C copies: a scalar as is, a container as compact JSON. */
export function copyText(v: Json): string {
  return typeof v === 'string' ? v : JSON.stringify(v);
}

/** Search keys and scalar values (case-insensitive); returns pointers in document order. */
export function searchJson(root: Json, query: string, limit = 1000): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const out: string[] = [];
  const go = (v: Json, pointer: string, key: string | null) => {
    if (out.length >= limit) return;
    const hit = (key !== null && key.toLowerCase().includes(q)) || (!Array.isArray(v) && !isObj(v) && String(v).toLowerCase().includes(q));
    if (hit) out.push(pointer);
    for (const [k, x] of entries(v)) go(x, `${pointer}/${esc(k)}`, Array.isArray(v) ? null : k);
  };
  go(root, '', null);
  return out;
}

export type TreeKeyAction =
  | { kind: 'focus'; pointer: string }
  | { kind: 'open'; pointer: string }
  | { kind: 'close'; pointer: string }
  | { kind: 'open-siblings'; pointers: string[] }
  | { kind: 'follow'; ref: RefInfo }
  | { kind: 'load'; pointer: string }
  | null;

/** WAI-ARIA tree keys over the visible lines (7.8). */
export function jsonKey(lines: JsonLine[], current: string, key: string): TreeKeyAction {
  const i = lines.findIndex((l) => l.pointer === current);
  if (i < 0) return lines[0] ? { kind: 'focus', pointer: lines[0].pointer } : null;
  const l = lines[i]!;
  const container = l.kind === 'object' || l.kind === 'array';
  switch (key) {
    case 'ArrowDown':
      return lines[i + 1] ? { kind: 'focus', pointer: lines[i + 1]!.pointer } : null;
    case 'ArrowUp':
      return i > 0 ? { kind: 'focus', pointer: lines[i - 1]!.pointer } : null;
    case 'Home':
      return { kind: 'focus', pointer: lines[0]!.pointer };
    case 'End':
      return { kind: 'focus', pointer: lines[lines.length - 1]!.pointer };
    case 'ArrowRight':
      if (l.kind === 'cut') return { kind: 'load', pointer: l.pointer };
      if (container && l.size > 0 && !l.open) return { kind: 'open', pointer: l.pointer };
      if (container && l.open && lines[i + 1]) return { kind: 'focus', pointer: lines[i + 1]!.pointer };
      return null;
    case 'ArrowLeft':
      if (container && l.open && l.pointer !== '') return { kind: 'close', pointer: l.pointer };
      return l.parent !== null ? { kind: 'focus', pointer: l.parent } : null;
    case 'Enter':
      if (l.kind === 'ref' && l.ref) return { kind: 'follow', ref: l.ref };
      if (l.kind === 'cut') return { kind: 'load', pointer: l.pointer };
      if (container && l.size > 0) return l.open ? { kind: 'close', pointer: l.pointer } : { kind: 'open', pointer: l.pointer };
      return null;
    case '*': {
      const sibs = lines.filter((x) => x.parent === l.parent && (x.kind === 'object' || x.kind === 'array') && x.size > 0 && !x.open).map((x) => x.pointer);
      return sibs.length ? { kind: 'open-siblings', pointers: sibs } : null;
    }
    default:
      return null;
  }
}
