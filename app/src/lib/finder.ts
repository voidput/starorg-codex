/**
 * The Explorer's item finder (addendum 7.17), the pure parts: the view state (mode, node,
 * search, filters, sort, grouping) and how it rides in the record history so Back/Forward
 * restores it; filters remembered per section, saved presets and recent searches in the
 * viewer's local storage (every access guarded: storage can be missing or blocked); active
 * filter chips; search-word highlighting; the stat columns of a page; the variants of a
 * folded row.
 */
import type { Filters, ListGroup, ListItem, SortKey, Stat } from './contract/dev';

export const NO_FILTERS: Filters = { sections: [], kinds: [], sub_types: [], sizes: [], manufacturers: [], grades: [], classes: [], tags: [], model: false, sounds: false, player_only: false, changes: [] };

export type Mode = 'items' | 'assets' | 'raw';

export interface FinderView {
  mode: Mode;
  /** Sections-view node of the list (`''` = every record). */
  node: string;
  query: string;
  filters: Filters;
  sort: SortKey;
  group: ListGroup;
  collapsed: string[];
  /** Assets mode: search sound events (ATL triggers) instead of records. */
  sounds: boolean;
  /** One row (and tree leaf) per readable name in a section; the AI, PU and template copies ride along as variants. */
  fold: boolean;
}

export const DEFAULT_VIEW: FinderView = { mode: 'items', node: '', query: '', filters: NO_FILTERS, sort: 'name', group: 'none', collapsed: [], sounds: false, fold: true };

const KEY = 'starorg.dev.finder';
export const MAX_RECENT = 10;
export const MAX_PRESETS = 20;

export function storage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

function read<T>(s: Storage | null, key: string, fallback: T): T {
  try {
    const v = s?.getItem(key);
    return v ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}

function write(s: Storage | null, key: string, v: unknown): void {
  try {
    s?.setItem(key, JSON.stringify(v));
  } catch {
    /* blocked or full: the setting just is not kept */
  }
}

/** Fill a partial or older filter object (storage, presets) to the full shape. */
export function fullFilters(f: Partial<Filters> | null | undefined): Filters {
  const out: Filters = { ...NO_FILTERS };
  if (!f || typeof f !== 'object') return out;
  for (const k of Object.keys(NO_FILTERS) as (keyof Filters)[]) {
    const v = f[k];
    const d = NO_FILTERS[k];
    if (Array.isArray(d) && Array.isArray(v)) (out as unknown as Record<string, unknown>)[k] = v.filter((x) => typeof x === (k === 'sizes' ? 'number' : 'string'));
    else if (typeof d === 'boolean' && typeof v === 'boolean') (out as unknown as Record<string, unknown>)[k] = v;
  }
  return out;
}

/** The section a node belongs to (`ships`), or `''` (every record, or a media root). */
export function sectionOf(node: string): string {
  const json = node.startsWith('s:') ? node.slice(2) : node.startsWith('m:models:') || node.startsWith('m:sounds:') ? node.slice(9) : null;
  if (json === null) return '';
  try {
    const p = JSON.parse(json) as unknown;
    return Array.isArray(p) && typeof p[0] === 'string' ? p[0] : '';
  } catch {
    return '';
  }
}

/** Filters remembered per section (`''` = everywhere). */
export function loadFilters(s: Storage | null, section: string): Filters | null {
  const all = read<Record<string, Partial<Filters>>>(s, `${KEY}.filters`, {});
  return section in all ? fullFilters(all[section]) : null;
}

export function saveFilters(s: Storage | null, section: string, f: Filters): void {
  const all = read<Record<string, Partial<Filters>>>(s, `${KEY}.filters`, {});
  all[section] = f;
  write(s, `${KEY}.filters`, all);
}

export function loadRecent(s: Storage | null): string[] {
  const v = read<unknown>(s, `${KEY}.recent`, []);
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string').slice(0, MAX_RECENT) : [];
}

/** Newest first, no duplicates (case-insensitive), at most [`MAX_RECENT`]. */
export function pushRecent(s: Storage | null, q: string): string[] {
  const t = q.trim();
  const list = loadRecent(s);
  if (t.length < 2) return list;
  const next = [t, ...list.filter((x) => x.toLowerCase() !== t.toLowerCase())].slice(0, MAX_RECENT);
  write(s, `${KEY}.recent`, next);
  return next;
}

export interface Preset {
  name: string;
  query: string;
  filters: Filters;
  sort: SortKey;
  group: ListGroup;
  node: string;
  /** Fold variants; presets saved before it existed fold. */
  fold: boolean;
}

export function loadPresets(s: Storage | null): Preset[] {
  const v = read<unknown>(s, `${KEY}.presets`, []);
  if (!Array.isArray(v)) return [];
  return v
    .filter((p): p is Preset => !!p && typeof p === 'object' && typeof (p as Preset).name === 'string')
    .map((p) => ({ name: p.name, query: String(p.query ?? ''), filters: fullFilters(p.filters), sort: p.sort ?? 'name', group: p.group ?? 'none', node: String(p.node ?? ''), fold: typeof p.fold === 'boolean' ? p.fold : true }))
    .slice(0, MAX_PRESETS);
}

/** Save (or replace, by name) a preset; newest first. */
export function savePreset(s: Storage | null, p: Preset): Preset[] {
  const name = p.name.trim();
  if (!name) return loadPresets(s);
  const next = [{ ...p, name }, ...loadPresets(s).filter((x) => x.name !== name)].slice(0, MAX_PRESETS);
  write(s, `${KEY}.presets`, next);
  return next;
}

export function deletePreset(s: Storage | null, name: string): Preset[] {
  const next = loadPresets(s).filter((x) => x.name !== name);
  write(s, `${KEY}.presets`, next);
  return next;
}

export function loadMode(s: Storage | null): Mode {
  const m = read<unknown>(s, `${KEY}.mode`, 'items');
  return m === 'assets' || m === 'raw' ? m : 'items';
}

export function saveMode(s: Storage | null, m: Mode): void {
  write(s, `${KEY}.mode`, m);
}

export type FacetKey = 'sections' | 'kinds' | 'sub_types' | 'sizes' | 'manufacturers' | 'grades' | 'classes' | 'tags' | 'changes';

export const FACETS: { key: FacetKey; label: string }[] = [
  { key: 'sections', label: 'Section' },
  { key: 'kinds', label: 'Type' },
  { key: 'sub_types', label: 'Sub-type' },
  { key: 'sizes', label: 'Size' },
  { key: 'manufacturers', label: 'Manufacturer' },
  { key: 'grades', label: 'Grade' },
  { key: 'classes', label: 'Class' },
  { key: 'tags', label: 'Tag' },
];

export function has(f: Filters, key: FacetKey, value: string): boolean {
  return key === 'sizes' ? f.sizes.includes(Number(value)) : (f[key] as string[]).includes(value);
}

/** Pick or unpick one value of a facet. */
export function toggle(f: Filters, key: FacetKey, value: string): Filters {
  if (key === 'sizes') {
    const n = Number(value);
    return { ...f, sizes: f.sizes.includes(n) ? f.sizes.filter((x) => x !== n) : [...f.sizes, n].sort((a, b) => a - b) };
  }
  const cur = f[key] as string[];
  return { ...f, [key]: cur.includes(value) ? cur.filter((x) => x !== value) : [...cur, value] };
}

export interface Chip {
  key: FacetKey | 'model' | 'sounds' | 'player_only';
  value: string;
  label: string;
}

/** The active filters as removable chips; `label` reads values through the page's facets. */
export function activeChips(f: Filters, labelOf: (key: FacetKey, value: string) => string = (_, v) => v): Chip[] {
  const out: Chip[] = [];
  if (f.player_only) out.push({ key: 'player_only', value: '', label: 'Player-usable only' });
  if (f.model) out.push({ key: 'model', value: '', label: 'Has model' });
  if (f.sounds) out.push({ key: 'sounds', value: '', label: 'Has sounds' });
  for (const { key, label } of [...FACETS, { key: 'changes' as const, label: 'Changes' }]) {
    const vals = key === 'sizes' ? f.sizes.map(String) : (f[key] as string[]);
    for (const v of vals) out.push({ key, value: v, label: `${label}: ${labelOf(key, v)}` });
  }
  return out;
}

export function removeChip(f: Filters, c: Chip): Filters {
  if (c.key === 'model' || c.key === 'sounds' || c.key === 'player_only') return { ...f, [c.key]: false };
  return toggle(f, c.key, c.value);
}

export const filterCount = (f: Filters) => activeChips(f).length;

/** Text split around the search words (case-insensitive), for `<mark>`. */
export function highlight(text: string, words: string[]): { text: string; hit: boolean }[] {
  const ws = words.map((w) => w.toLowerCase()).filter((w) => w.length > 0 && !w.includes(':'));
  if (ws.length === 0 || !text) return [{ text, hit: false }];
  const lower = text.toLowerCase();
  const marks = new Array<boolean>(text.length).fill(false);
  for (const w of ws) {
    let at = lower.indexOf(w);
    while (at >= 0) {
      for (let k = at; k < at + w.length; k++) marks[k] = true;
      at = lower.indexOf(w, at + w.length);
    }
  }
  const out: { text: string; hit: boolean }[] = [];
  for (let k = 0; k < text.length; k++) {
    const last = out.at(-1);
    if (last && last.hit === marks[k]) last.text += text[k];
    else out.push({ text: text[k]!, hit: marks[k]! });
  }
  return out;
}

export interface StatColumn {
  key: string;
  label: string;
  unit: string | null;
}

/** The stats the page's rows carry, in first-seen order, at most `max`. */
export function statColumns(items: ListItem[], max = 4): StatColumn[] {
  const out: StatColumn[] = [];
  for (const it of items)
    for (const s of it.stats ?? [])
      if (!out.some((c) => c.key === s.key)) {
        if (out.length >= max) return out;
        out.push({ key: s.key, label: s.label, unit: s.unit });
      }
  return out;
}

export function statText(s: Stat | undefined): string {
  if (!s) return '–';
  const v = Math.abs(s.value) >= 1000 ? Math.round(s.value).toLocaleString('en-US') : String(Math.round(s.value * 10) / 10);
  return s.unit ? `${v} ${s.unit}` : v;
}

/** The records of one folded row, for the record header's Variant picker. */
export interface VariantGroup {
  label: string;
  /** The representative first, then its variants in the worker's order. */
  members: { guid: string; short: string; variant: boolean }[];
}

const shortOf = (name: string) => {
  const d = name.indexOf('.');
  return d >= 0 ? name.slice(d + 1) : name;
};

/** The folded row of the list that holds `guid` (as representative or variant); null when none does or the row has no variants. */
export function variantGroup(items: ListItem[], guid: string | null): VariantGroup | null {
  if (!guid || !Array.isArray(items)) return null;
  for (const it of items) {
    const vs = Array.isArray(it.variants) ? it.variants : [];
    if (vs.length === 0 || (it.guid !== guid && !vs.some((v) => v.guid === guid))) continue;
    return { label: it.label, members: [{ guid: it.guid, short: shortOf(it.name), variant: it.variant }, ...vs.map((v) => ({ guid: v.guid, short: v.short, variant: v.variant }))] };
  }
  return null;
}

/** A record-history entry: the record and the finder view it was opened from. */
export function encodeEntry(guid: string, view: FinderView | null): string {
  return view ? JSON.stringify({ g: guid, v: view }) : guid;
}

export function decodeEntry(x: string): { guid: string; view: FinderView | null } {
  if (!x.startsWith('{')) return { guid: x, view: null };
  try {
    const o = JSON.parse(x) as { g?: unknown; v?: Partial<FinderView> };
    if (typeof o.g !== 'string') return { guid: x, view: null };
    const v = o.v;
    const view: FinderView | null = v
      ? {
          ...DEFAULT_VIEW,
          ...v,
          mode: v.mode === 'assets' || v.mode === 'raw' ? v.mode : 'items',
          filters: fullFilters(v.filters),
          collapsed: Array.isArray(v.collapsed) ? v.collapsed.filter((c): c is string => typeof c === 'string') : [],
          fold: typeof v.fold === 'boolean' ? v.fold : true,
        }
      : null;
    return { guid: o.g, view };
  } catch {
    return { guid: x, view: null };
  }
}

/** Sort and group options of the list. */
export const SORTS: { id: SortKey; label: string }[] = [
  { id: 'name', label: 'Name' },
  { id: 'size', label: 'Size' },
  { id: 'manufacturer', label: 'Manufacturer' },
];
export const GROUPS: { id: ListGroup; label: string }[] = [
  { id: 'none', label: 'No grouping' },
  { id: 'section', label: 'Section' },
  { id: 'kind', label: 'Type' },
  { id: 'manufacturer', label: 'Manufacturer' },
  { id: 'size', label: 'Size' },
];

/** Query prefixes, for the search help. */
export const PREFIXES: [string, string][] = [
  ['type:cooler', 'item type or record type'],
  ['maker:aegis', 'manufacturer name or code'],
  ['size:2 · size:1-3', 'size or a range'],
  ['section:ships', 'section'],
  ['grade:a · class:military', 'component grade and class'],
  ['tag:', 'record tag'],
  ['has:model · has:sounds', 'only records with a model or sounds'],
];
