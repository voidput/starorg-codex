/**
 * Blueprint finder (addendum §7.19): search, facet filters with counts, sort and group over the
 * rows of `blueprint_finder` (the build's catalogue with every channel's owners). Pure: the screen
 * keeps the prefs (per channel, in localStorage) and renders the view.
 *
 * Facet counts follow the usual rule: a facet's numbers apply every other filter but its own, so
 * a count says what you get when you add that value. "Owned" / "Missing" mean owned / not owned
 * by the picked characters (any character when none is picked) on the channels in view; picking
 * characters with "All" keeps every row and only changes whose ownership is shown and counted.
 */
import type { BlueprintFinder, BlueprintKind, BlueprintOwner, BlueprintRow } from './contract/types';

export type OwnedFilter = 'all' | 'owned' | 'missing';
export type SortKey = 'name' | 'kind' | 'manufacturer' | 'size' | 'acquired' | 'source' | 'owners';
export type GroupKey = 'none' | 'kind' | 'manufacturer' | 'character';
export type Scope = 'channel' | 'all';
export type FacetId = 'kind' | 'type' | 'manufacturer' | 'size' | 'source' | 'character' | 'channel';

export const FACETS: FacetId[] = ['kind', 'type', 'manufacturer', 'size', 'source', 'character', 'channel'];

export interface FinderPrefs {
  scope: Scope;
  search: string;
  owned: OwnedFilter;
  kind: string[];
  type: string[];
  manufacturer: string[];
  size: string[];
  source: string[];
  character: string[];
  channel: string[];
  sort: SortKey;
  desc: boolean;
  group: GroupKey;
}

export const DEFAULT_PREFS: FinderPrefs = {
  scope: 'channel',
  search: '',
  owned: 'owned',
  kind: [],
  type: [],
  manufacturer: [],
  size: [],
  source: [],
  character: [],
  channel: [],
  sort: 'name',
  desc: false,
  group: 'none',
};

export const SORTS: { id: SortKey; label: string }[] = [
  { id: 'name', label: 'Name' },
  { id: 'kind', label: 'Category' },
  { id: 'manufacturer', label: 'Manufacturer' },
  { id: 'size', label: 'Size / grade' },
  { id: 'acquired', label: 'Acquired' },
  { id: 'source', label: 'Source' },
  { id: 'owners', label: 'Owned by' },
];

export const GROUPS: { id: GroupKey; label: string }[] = [
  { id: 'none', label: 'No groups' },
  { id: 'kind', label: 'Category' },
  { id: 'manufacturer', label: 'Manufacturer' },
  { id: 'character', label: 'Character' },
];

export const KINDS: { id: BlueprintKind; label: string }[] = [
  { id: 'ship-component', label: 'Ship component' },
  { id: 'ship-weapon', label: 'Ship weapon' },
  { id: 'fps-weapon', label: 'FPS weapon' },
  { id: 'armor', label: 'Armor' },
  { id: 'fps-ammo', label: 'FPS ammo' },
  { id: 'mission-item', label: 'Mission item' },
  { id: 'other', label: 'Other' },
];
const KIND_RANK = new Map(KINDS.map((k, i) => [k.id, i]));
export const kindLabel = (k: BlueprintKind) => KINDS.find((x) => x.id === k)?.label ?? k;

/** Stored source kinds -> facet ids. */
export function sourceId(s: string): string {
  if (s === 'log') return 'log';
  if (s === 'import:smart-citizen') return 'smart-citizen';
  if (s.startsWith('starorg:')) return 'starorg';
  return s;
}
const SOURCE_LABELS: Record<string, string> = { log: 'Game log', 'smart-citizen': 'Smart Citizen', starorg: 'StarOrg' };
const SOURCE_RANK = ['log', 'smart-citizen', 'starorg'];
export const sourceLabel = (id: string) => SOURCE_LABELS[id] ?? id;
/** A stored source for display: `StarOrg (Org)` keeps the org name. */
export const sourceText = (s: string) => (s.startsWith('starorg:') ? `StarOrg (${s.slice(8)})` : sourceLabel(sourceId(s)));

const CLASSES = ['light', 'medium', 'heavy'];
/** Size facet id: `s3`, or the armor weight (`medium`), or `none`. */
export function sizeId(r: BlueprintRow): string {
  if (r.size !== null) return `s${r.size}`;
  if (r.class) return r.class.toLowerCase();
  return 'none';
}
export function sizeLabel(id: string): string {
  if (id === 'none') return 'No size';
  if (/^s\d+$/.test(id)) return `S${id.slice(1)}`;
  return id.charAt(0).toUpperCase() + id.slice(1);
}
function sizeRank(id: string): number {
  if (/^s\d+$/.test(id)) return Number(id.slice(1));
  const c = CLASSES.indexOf(id);
  return c >= 0 ? 100 + c : 1000;
}

export const makerId = (r: BlueprintRow) => r.manufacturer ?? 'none';
export const makerLabel = (r: BlueprintRow) => r.manufacturer_name ?? r.manufacturer ?? 'Unknown maker';

// Prefs ------------------------------------------------------------------------------------

export const PREFS_KEY = (install: string, channel: string) => `starorg.blueprints.finder.v1:${install}/${channel}`;

const oneOf = <X extends string>(v: unknown, list: readonly X[], d: X): X => (typeof v === 'string' && (list as readonly string[]).includes(v) ? (v as X) : d);
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string').slice(0, 200) : []);

/** Stored prefs, or the defaults for anything missing or malformed. */
export function parsePrefs(raw: string | null): FinderPrefs {
  let o: Record<string, unknown> = {};
  try {
    const v = raw ? JSON.parse(raw) : null;
    if (v && typeof v === 'object' && !Array.isArray(v)) o = v as Record<string, unknown>;
  } catch {
    /* defaults */
  }
  const d = DEFAULT_PREFS;
  return {
    scope: oneOf(o.scope, ['channel', 'all'] as const, d.scope),
    search: typeof o.search === 'string' ? o.search.slice(0, 200) : '',
    owned: oneOf(o.owned, ['all', 'owned', 'missing'] as const, d.owned),
    kind: strings(o.kind),
    type: strings(o.type),
    manufacturer: strings(o.manufacturer),
    size: strings(o.size),
    source: strings(o.source),
    character: strings(o.character),
    channel: strings(o.channel),
    sort: oneOf(o.sort, SORTS.map((s) => s.id), d.sort),
    desc: o.desc === true,
    group: oneOf(o.group, GROUPS.map((g) => g.id), d.group),
  };
}

/** Number of active filters (search, owned toggle and every facet value). */
export function activeFilters(p: FinderPrefs): number {
  return (p.search.trim() ? 1 : 0) + (p.owned !== DEFAULT_PREFS.owned ? 1 : 0) + FACETS.reduce((n, f) => n + p[f].length, 0);
}

/** Every filter back to its default; sort, group and scope stay. */
export function clearFilters(p: FinderPrefs): FinderPrefs {
  return { ...p, search: '', owned: DEFAULT_PREFS.owned, kind: [], type: [], manufacturer: [], size: [], source: [], character: [], channel: [] };
}

export function toggle(p: FinderPrefs, f: FacetId, id: string): FinderPrefs {
  const cur = p[f];
  return { ...p, [f]: cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id] };
}

// Search -----------------------------------------------------------------------------------

export function fold(s: string): string {
  return s
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .trim();
}

function subsequence(needle: string, hay: string): boolean {
  let i = 0;
  for (let j = 0; j < hay.length && i < needle.length; j++) if (hay[j] === needle[i]) i++;
  return i === needle.length;
}

/**
 * 2 = every word found as text (name, record, type, maker), 1 = fuzzy (a word of 3+ letters
 * found in order in the name, gaps allowed: `scnn` finds "Synthetic Cannon"), 0 = no match.
 */
export function matchScore(r: BlueprintRow, words: string[]): number {
  if (!words.length) return 2;
  const hay = ` ${fold([r.name, r.record ?? '', r.type_label ?? '', r.manufacturer_name ?? '', r.manufacturer ?? ''].join(' '))} `;
  const compact = fold(r.name).replace(/ /g, '');
  let score = 2;
  for (const w of words) {
    if (hay.includes(w)) continue;
    if (w.length >= 3 && subsequence(w.replace(/ /g, ''), compact)) score = 1;
    else return 0;
  }
  return score;
}

// The view ---------------------------------------------------------------------------------

export interface FacetValue {
  id: string;
  label: string;
  count: number;
  on: boolean;
}

export interface Group {
  id: string;
  label: string;
  rows: BlueprintRow[];
}

export interface FinderView {
  /** Matching rows, sorted. */
  rows: BlueprintRow[];
  /** With a group-by: the rows per group (a row can sit in several character groups). */
  groups: Group[] | null;
  facets: Record<FacetId, FacetValue[]>;
  owned: Record<OwnedFilter, number>;
  /** Rows in view before any filter. */
  total: number;
}

export interface ViewCtx {
  /** Channels in view (the current one, or all). */
  channels: string[];
  characterName: (id: string) => string;
}

/** Owners on the channels in view and (when set) the picked channels. */
function scoped(r: BlueprintRow, chans: Set<string>): BlueprintOwner[] {
  return r.owners.filter((o) => chans.has(o.channel));
}

export function query(data: BlueprintFinder, p: FinderPrefs, ctx: ViewCtx): FinderView {
  const inView = new Set(ctx.channels);
  const picked = new Set(p.channel.filter((c) => inView.has(c)));
  const chans = picked.size ? picked : inView;
  const chars = new Set(p.character);
  const words = fold(p.search).split(' ').filter(Boolean);
  const rows = (Array.isArray(data.rows) ? data.rows : []).filter((r) => r.channels.some((c) => inView.has(c)) || r.owners.some((o) => inView.has(o.channel)));

  type Eval = { r: BlueprintRow; score: number; own: BlueprintOwner[]; byChars: BlueprintOwner[]; pass: Record<FacetId, boolean>; ownedOk: boolean; ownedAny: boolean };
  const evals: Eval[] = [];
  for (const r of rows) {
    const score = matchScore(r, words);
    if (!score) continue;
    const own = scoped(r, chans);
    const byChars = chars.size ? own.filter((o) => chars.has(o.character)) : own;
    const ownedAny = byChars.length > 0;
    const srcs = new Set(byChars.flatMap((o) => o.sources.map(sourceId)));
    const inChan = r.channels.some((c) => chans.has(c)) || own.length > 0;
    const pass: Record<FacetId, boolean> = {
      kind: !p.kind.length || p.kind.includes(r.kind),
      type: !p.type.length || (r.type_label !== null && p.type.includes(r.type_label)),
      manufacturer: !p.manufacturer.length || p.manufacturer.includes(makerId(r)),
      size: !p.size.length || p.size.includes(sizeId(r)),
      source: !p.source.length || p.source.some((s) => srcs.has(s)),
      // Characters say whose ownership counts; the owned toggle filters by it.
      character: true,
      channel: !picked.size || inChan,
    };
    const ownedOk = p.owned === 'all' || (p.owned === 'owned' ? ownedAny : !ownedAny);
    evals.push({ r, score, own, byChars, pass, ownedOk, ownedAny });
  }
  const passAll = (e: Eval, except?: FacetId) => FACETS.every((f) => f === except || e.pass[f]);

  // Facet counts: every other filter applies (the owned toggle too).
  const counts: Record<FacetId, Map<string, number>> = Object.fromEntries(FACETS.map((f) => [f, new Map()])) as Record<FacetId, Map<string, number>>;
  const bump = (f: FacetId, id: string) => counts[f].set(id, (counts[f].get(id) ?? 0) + 1);
  for (const e of evals) {
    const { r } = e;
    for (const f of FACETS) {
      if (f === 'character') {
        // Per character: rows that character owns (from the picked sources), or lacks with
        // "Missing". The source filter is applied per character here.
        if (!FACETS.every((x) => x === 'character' || x === 'source' || e.pass[x])) continue;
        for (const c of data.characters) {
          const mine = e.own.filter((o) => o.character === c.id && (!p.source.length || o.sources.some((s) => p.source.includes(sourceId(s)))));
          if (p.owned === 'missing' ? !mine.length : mine.length > 0) bump(f, c.id);
        }
        continue;
      }
      if (f === 'channel') {
        // Per channel: rows owned there (from the picked characters and sources), missing
        // there (in its build), or in its build at all, as the owned toggle says.
        if (!FACETS.every((x) => x === 'channel' || x === 'source' || e.pass[x])) continue;
        for (const c of inView) {
          const mine = r.owners.filter((o) => o.channel === c && (!chars.size || chars.has(o.character)) && (!p.source.length || o.sources.some((s) => p.source.includes(sourceId(s)))));
          const inBuild = r.channels.includes(c);
          if (p.owned === 'owned' ? mine.length > 0 : p.owned === 'missing' ? inBuild && !mine.length : inBuild || mine.length > 0) bump(f, c);
        }
        continue;
      }
      if (!passAll(e, f) || !e.ownedOk) continue;
      if (f === 'kind') bump(f, r.kind);
      else if (f === 'type') r.type_label !== null && bump(f, r.type_label);
      else if (f === 'manufacturer') bump(f, makerId(r));
      else if (f === 'size') bump(f, sizeId(r));
      else if (f === 'source') for (const s of new Set(e.byChars.flatMap((o) => o.sources.map(sourceId)))) bump(f, s);

    }
  }
  const owned: Record<OwnedFilter, number> = { all: 0, owned: 0, missing: 0 };
  for (const e of evals) {
    if (!passAll(e)) continue;
    owned.all++;
    if (e.ownedAny) owned.owned++;
    else owned.missing++;
  }

  const labels: Record<FacetId, (id: string) => string> = {
    kind: (id) => kindLabel(id as BlueprintKind),
    type: (id) => id,
    manufacturer: (id) => (id === 'none' ? 'Unknown maker' : (rows.find((r) => r.manufacturer === id)?.manufacturer_name ?? id)),
    size: sizeLabel,
    source: sourceLabel,
    character: ctx.characterName,
    channel: (id) => id,
  };
  const order: Record<FacetId, (a: FacetValue, b: FacetValue) => number> = {
    kind: (a, b) => (KIND_RANK.get(a.id as BlueprintKind) ?? 99) - (KIND_RANK.get(b.id as BlueprintKind) ?? 99),
    type: (a, b) => b.count - a.count || cmp(a.label, b.label),
    manufacturer: (a, b) => (a.id === 'none' ? 1 : 0) - (b.id === 'none' ? 1 : 0) || cmp(a.label, b.label),
    size: (a, b) => sizeRank(a.id) - sizeRank(b.id),
    source: (a, b) => rank(SOURCE_RANK, a.id) - rank(SOURCE_RANK, b.id) || cmp(a.id, b.id),
    character: (a, b) => cmp(a.label, b.label) || cmp(a.id, b.id),
    channel: (a, b) => ctx.channels.indexOf(a.id) - ctx.channels.indexOf(b.id),
  };
  const facets = {} as Record<FacetId, FacetValue[]>;
  for (const f of FACETS) {
    const ids = new Set([...counts[f].keys(), ...p[f]]);
    if (f === 'character') for (const c of data.characters) ids.add(c.id);
    if (f === 'channel') for (const c of ctx.channels) ids.add(c);
    facets[f] = [...ids].map((id) => ({ id, label: labels[f](id), count: counts[f].get(id) ?? 0, on: p[f].includes(id) })).sort(order[f]);
  }

  const hits = evals.filter((e) => e.ownedOk && passAll(e));
  const sorted = sortRows(hits, p, words.length > 0);
  return { rows: sorted.map((e) => e.r), groups: groupRows(sorted, p, ctx), facets, owned, total: rows.length };
}

const cmp = (a: string, b: string) => a.localeCompare(b, 'en', { sensitivity: 'base', numeric: true });
const rank = (list: string[], id: string) => {
  const i = list.indexOf(id);
  return i < 0 ? list.length : i;
};

interface Sortable {
  r: BlueprintRow;
  score: number;
  byChars: BlueprintOwner[];
}

/** Earliest acquired time over the owners in view. */
export function acquired(owners: BlueprintOwner[]): string | null {
  let best: string | null = null;
  for (const o of owners) if (o.acquired && (best === null || o.acquired < best)) best = o.acquired;
  return best;
}

/** Distinct characters among the owners in view. */
export const ownerCount = (owners: BlueprintOwner[]) => new Set(owners.map((o) => o.character)).size;

function sortRows<E extends Sortable>(list: E[], p: FinderPrefs, searching: boolean): E[] {
  const dir = p.desc ? -1 : 1;
  // Unknown values sort last in both directions.
  const nulls = <V>(a: V | null, b: V | null, c: (x: V, y: V) => number) => (a === null ? (b === null ? 0 : 1) : b === null ? -1 : dir * c(a, b));
  const name = (a: E, b: E) => cmp(a.r.name, b.r.name) || cmp(a.r.key, b.r.key);
  const by: Record<SortKey, (a: E, b: E) => number> = {
    name: (a, b) => dir * name(a, b),
    kind: (a, b) => dir * ((KIND_RANK.get(a.r.kind) ?? 99) - (KIND_RANK.get(b.r.kind) ?? 99)) || nulls(a.r.type_label, b.r.type_label, cmp) || name(a, b),
    manufacturer: (a, b) => nulls(a.r.manufacturer ? makerLabel(a.r) : null, b.r.manufacturer ? makerLabel(b.r) : null, cmp) || name(a, b),
    size: (a, b) => {
      const sa = sizeId(a.r);
      const sb = sizeId(b.r);
      return nulls(sa === 'none' ? null : sizeRank(sa), sb === 'none' ? null : sizeRank(sb), (x, y) => x - y) || nulls(a.r.grade, b.r.grade, cmp) || name(a, b);
    },
    acquired: (a, b) => nulls(acquired(a.byChars), acquired(b.byChars), (x, y) => (x < y ? -1 : x > y ? 1 : 0)) || name(a, b),
    source: (a, b) => {
      const s = (e: E) => {
        const ids = e.byChars.flatMap((o) => o.sources.map(sourceId));
        return ids.length ? Math.min(...ids.map((i) => rank(SOURCE_RANK, i))) : null;
      };
      return nulls(s(a), s(b), (x, y) => x - y) || name(a, b);
    },
    owners: (a, b) => dir * (ownerCount(a.byChars) - ownerCount(b.byChars)) || name(a, b),
  };
  const f = by[p.sort];
  // While searching, word hits come before fuzzy ones.
  return [...list].sort((a, b) => (searching ? b.score - a.score : 0) || f(a, b));
}

function groupRows(list: Sortable[], p: FinderPrefs, ctx: ViewCtx): Group[] | null {
  if (p.group === 'none') return null;
  const m = new Map<string, Group>();
  const add = (id: string, label: string, r: BlueprintRow) => {
    let g = m.get(id);
    if (!g) m.set(id, (g = { id, label, rows: [] }));
    g.rows.push(r);
  };
  for (const e of list) {
    if (p.group === 'kind') add(e.r.kind, kindLabel(e.r.kind), e.r);
    else if (p.group === 'manufacturer') add(makerId(e.r), makerLabel(e.r), e.r);
    else {
      const who = [...new Set(e.byChars.map((o) => o.character))];
      if (!who.length) add('', 'Not owned', e.r);
      for (const c of who) add(c, ctx.characterName(c), e.r);
    }
  }
  const groups = [...m.values()];
  if (p.group === 'kind') groups.sort((a, b) => (KIND_RANK.get(a.id as BlueprintKind) ?? 99) - (KIND_RANK.get(b.id as BlueprintKind) ?? 99));
  else groups.sort((a, b) => (a.id === '' || a.id === 'none' ? 1 : 0) - (b.id === '' || b.id === 'none' ? 1 : 0) || cmp(a.label, b.label));
  return groups;
}
