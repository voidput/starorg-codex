/**
 * The Explorer record's "Readable" view, as pure functions: the record's JSON turned into
 * sections of key/value rows a player can read. Keys are humanized ("maxHealth" reads as "Max
 * health"), numbers lose float noise and gain their unit, references and GUIDs read as the
 * record they name, `@keys` as their text, and DataCore's own wrappers and markers are left
 * out. Every row and group keeps its JSON pointer, so a cut part loads like in the raw tree.
 */
import type { LocText, Readable } from './contract/dev';
import { cutOf, esc, refOf, type Json, type RefInfo } from './jsontree.ts';
import { numberCell, unitOf } from './readable.ts';

export type SheetMode = 'readable' | 'raw';
export const SHEET_MODE_KEY = 'starorg.dev.recordView';

type Store = Pick<Storage, 'getItem' | 'setItem'>;

/** The view as this viewer left it (Readable when storage is blocked or empty). */
export function loadSheetMode(s: Store | null): SheetMode {
  try {
    return s?.getItem(SHEET_MODE_KEY) === 'raw' ? 'raw' : 'readable';
  } catch {
    return 'readable';
  }
}

export function saveSheetMode(s: Store | null, m: SheetMode): void {
  try {
    s?.setItem(SHEET_MODE_KEY, m);
  } catch {
    /* storage blocked: the switch still works for this session */
  }
}

export type SheetValue =
  | { kind: 'text'; text: string }
  | { kind: 'number'; text: string }
  | { kind: 'bool'; text: string }
  /** A record: a `_ref` or a GUID with a label. `follow` is set when it can be opened. */
  | { kind: 'ref'; text: string; detail: string | null; follow: RefInfo | null }
  /** A loc key: its text, or the key itself when nothing reads it. */
  | { kind: 'loc'; text: string; key: string; enhanced: string | null; missing: boolean }
  | { kind: 'list'; text: string; items: string[]; more: number }
  | { kind: 'empty'; text: string };

export interface SheetRow {
  pointer: string;
  /** Raw key (or `#n` for an array element). */
  key: string;
  label: string;
  value: SheetValue;
  /** The raw value as text: shown with "Show IDs", copied, searched. */
  raw: string;
  empty: boolean;
}

export interface SheetGroup {
  /** JSON pointer of the node; `#general` for the record's top-level values. */
  id: string;
  title: string;
  /** Muted line beside the title: the humanized `_Type_`, or where an element sits. */
  subtitle: string | null;
  /** Raw `_Type_`, when there is one. */
  type: string | null;
  /** Titles of the enclosing groups, outermost first. */
  trail: string[];
  depth: number;
  rows: SheetRow[];
  groups: SheetGroup[];
  /** A part the backend cut: loads with `onload(id)`. */
  cut: { kind: 'object' | 'array'; len: number } | null;
}

export interface SheetOptions {
  labels?: Record<string, Readable>;
  loc?: LocText[];
}

const isObj = (v: unknown): v is { [k: string]: Json } => typeof v === 'object' && v !== null && !Array.isArray(v);
const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const ZERO_GUID = '00000000-0000-0000-0000-000000000000';
/** Loc keys that stand for "no text". */
const EMPTY_LOC = /^@(LOC_EMPTY|LOC_UNINITIALIZED|LOC_PLACEHOLDER)?$/i;
const ACRONYMS = new Set(['SCM', 'HP', 'DPS', 'SCU', 'EM', 'IR', 'QT', 'ID', 'UI', 'AI', 'GUID', 'FPS', 'RPM', 'HUD', 'NPC']);
const NAME_KEYS = ['name', 'Name', 'displayName', 'DisplayName', 'itemName', 'portName', 'Tag', 'tag'];
const LIST_SHOWN = 8;

/** DataCore's markers and wrappers, not data a player reads. */
export function isNoiseKey(k: string): boolean {
  return k === '_Type_' || k === '_Pointer_' || k === '_RecordName_' || k === '_RecordId_' || k === '_RecordPath_' || k === '_RecordTag_' || k.startsWith('__');
}

const WORD = /[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+|\d+/g;

/** "maxHealth" reads as "Max health", "SCMSpeed" as "SCM speed", "_Type_" as "Type". */
export function humanize(key: string): string {
  const words = key.split(/[_\s-]+/).flatMap((p) => p.match(WORD) ?? []);
  if (!words.length) return key;
  const out = words.map((w, i) => {
    const up = w.toUpperCase();
    if (ACRONYMS.has(up) || (w.length > 1 && w === up && /[A-Z]/.test(w))) return up;
    const low = w.toLowerCase();
    return i === 0 ? low.charAt(0).toUpperCase() + low.slice(1) : low;
  });
  return out.join(' ');
}

/** A DataCore type as a title: "SAttachableComponentParams" reads as "Attachable component". */
export function humanizeType(type: string): string {
  const t = type.replace(/^[SE](?=[A-Z][a-z])/, '').replace(/(Params|Parameters|Definition|Def)$/, '');
  return humanize(t || type);
}

/** A number without float noise (0.30000001 reads as 0.3), grouped, with the unit its key says. */
export function numberText(n: number, key: string): string {
  if (!Number.isFinite(n)) return String(n);
  const r = Number.isInteger(n) || Math.abs(n) >= 1000 ? n : Number(n.toPrecision(6));
  const k = key.toLowerCase();
  // A multiplier of a speed is not a speed.
  const unit = /multiplier|modifier|scale|ratio|factor|percent/.test(k) ? null : unitOf(key);
  if (Math.abs(r) >= 1000 || Number.isInteger(r)) return numberCell(String(r), unit, false) ?? String(r);
  return unit ? `${r} ${unit}` : String(r);
}

/** The text of a scalar for lists and search. */
function scalarRead(v: Json, key: string, o: Ctx): SheetValue {
  if (v === null) return { kind: 'empty', text: 'None' };
  if (typeof v === 'boolean') return { kind: 'bool', text: v ? 'Yes' : 'No' };
  if (typeof v === 'number') return { kind: 'number', text: numberText(v, key) };
  if (typeof v !== 'string') return { kind: 'text', text: JSON.stringify(v) };
  const s = v.trim();
  if (s === '') return { kind: 'empty', text: 'Empty' };
  if (GUID.test(s)) {
    if (s === ZERO_GUID) return { kind: 'empty', text: 'None' };
    const lab = o.labels[s];
    if (lab && lab.text) return { kind: 'ref', text: lab.text, detail: lab.detail, follow: lab.kind === 'record' || lab.kind === 'tag' ? { guid: s, name: lab.detail, type: null, path: null } : null };
    return { kind: 'text', text: s };
  }
  if (s.startsWith('@')) {
    if (EMPTY_LOC.test(s)) return { kind: 'empty', text: 'No text' };
    const lab = o.labels[s];
    const text = (lab?.kind === 'loc' || !lab ? lab?.text : null) ?? o.loc.get(s);
    if (text !== undefined && text !== null) return text.trim() ? { kind: 'loc', text, key: s, enhanced: lab?.enhanced ?? null, missing: false } : { kind: 'empty', text: 'No text' };
    return { kind: 'loc', text: s, key: s, enhanced: null, missing: true };
  }
  const lab = o.labels[s];
  if (lab && lab.text && lab.text !== s) return { kind: 'ref', text: lab.text, detail: lab.detail, follow: null };
  return { kind: 'text', text: v };
}

interface Ctx {
  labels: Record<string, Readable>;
  loc: Map<string, string>;
}

const rawText = (v: Json): string => (v === null ? 'null' : typeof v === 'string' ? v : JSON.stringify(v));

function refValue(r: RefInfo, o: Ctx): SheetValue {
  if (r.guid === ZERO_GUID) return { kind: 'empty', text: 'None' };
  const lab = o.labels[r.guid];
  return { kind: 'ref', text: lab?.text || r.name || r.guid, detail: lab?.text ? (r.name ?? lab.detail) : null, follow: r };
}

function emptyContainer(v: Json): boolean {
  return (Array.isArray(v) && v.length === 0) || (isObj(v) && !refOf(v) && !cutOf(v) && Object.keys(v).every(isNoiseKey));
}

/** An array that reads as one comma list: scalars only, and no GUID that names a record. */
function scalarList(a: Json[], o: Ctx): boolean {
  return a.every((x) => x === null || typeof x !== 'object') && !a.some((x) => typeof x === 'string' && GUID.test(x) && o.labels[x]?.kind === 'record');
}

function row(pointer: string, key: string, label: string, v: Json, o: Ctx): SheetRow {
  const ref = refOf(v);
  let value: SheetValue;
  if (ref) value = refValue(ref, o);
  else if (Array.isArray(v)) {
    if (!v.length) value = { kind: 'empty', text: 'None' };
    else {
      const all = v.map((x) => scalarRead(x, key, o).text);
      value = { kind: 'list', text: all.join(', '), items: all.slice(0, LIST_SHOWN), more: Math.max(0, all.length - LIST_SHOWN) };
    }
  } else if (isObj(v)) value = { kind: 'empty', text: 'None' };
  else value = scalarRead(v, key, o);
  return { pointer, key, label, value, raw: ref ? ref.guid : rawText(v), empty: value.kind === 'empty' };
}

function typeOf(v: Json): string | null {
  return isObj(v) && typeof v['_Type_'] === 'string' ? v['_Type_'] : null;
}

function nameOf(v: Json): string | null {
  if (!isObj(v)) return null;
  for (const k of NAME_KEYS) {
    const x = v[k];
    if (typeof x === 'string' && x.trim() && !x.startsWith('@')) return x;
  }
  return null;
}

/** Fill `g` with the fields of object `v` at `pointer`. */
function fill(g: SheetGroup, v: { [k: string]: Json }, pointer: string, o: Ctx, general: SheetGroup | null): void {
  for (const [k, x] of Object.entries(v)) {
    if (isNoiseKey(k)) continue;
    add(g, k, humanize(k), x, `${pointer}/${esc(k)}`, o, general);
  }
}

function group(id: string, title: string, subtitle: string | null, type: string | null, parent: SheetGroup | null): SheetGroup {
  const trail = parent && parent.id !== '' ? [...parent.trail, parent.title] : [];
  return { id, title, subtitle, type, trail, depth: parent && parent.id !== '' ? parent.depth + 1 : 0, rows: [], groups: [], cut: null };
}

/**
 * One field of `parent`. `general` is the record's top-level "General" section: top-level
 * scalars go there, while top-level objects become sections of their own.
 */
function add(parent: SheetGroup, key: string, label: string, v: Json, pointer: string, o: Ctx, general: SheetGroup | null): void {
  const rows = general ?? parent;
  const cut = cutOf(v);
  if (cut) {
    const g = group(pointer, label, cut.kind === 'array' ? `${cut.len} items, not loaded` : `${cut.len} fields, not loaded`, null, parent);
    g.cut = cut;
    parent.groups.push(g);
    return;
  }
  if (refOf(v) || v === null || typeof v !== 'object' || emptyContainer(v)) {
    rows.rows.push(row(pointer, key, label, v, o));
    return;
  }
  if (Array.isArray(v)) {
    if (scalarList(v, o)) {
      rows.rows.push(row(pointer, key, label, v, o));
      return;
    }
    // Objects become groups of their own; references and scalars are rows of one group.
    let loose: SheetGroup | null = null;
    v.forEach((x, i) => {
      const p = `${pointer}/${i}`;
      const n = `${label} #${i + 1}`;
      const c = cutOf(x);
      if (isObj(x) && !refOf(x) && !emptyContainer(x)) {
        const t = typeOf(x);
        const name = nameOf(x);
        const title = c ? n : t ? humanizeType(t) : (name ?? n);
        const g = group(p, title, c ? `${c.len} ${c.kind === 'array' ? 'items' : 'fields'}, not loaded` : t && name ? `${name} · ${n}` : title === n ? null : n, t, parent);
        if (c) g.cut = c;
        else fill(g, x, p, o, null);
        parent.groups.push(g);
        return;
      }
      if (!loose) {
        loose = group(pointer, label, `${v.length} items`, null, parent);
        parent.groups.push(loose);
      }
      loose.rows.push(row(p, `#${i + 1}`, `#${i + 1}`, x, o));
    });
    return;
  }
  const t = typeOf(v);
  const g = group(pointer, label, t ? humanizeType(t) : null, t, parent);
  fill(g, v, pointer, o, null);
  parent.groups.push(g);
}

/** The record's sections: "General" for top-level values, then one per top-level part. */
export function buildSheet(root: Json, opts: SheetOptions = {}): SheetGroup[] {
  const o: Ctx = { labels: opts.labels ?? {}, loc: new Map((Array.isArray(opts.loc) ? opts.loc : []).map((l) => [l.key, l.text])) };
  const top = group('', '', null, null, null);
  const general = group('#general', 'General', null, null, null);
  let start: Json = root;
  let pointer = '';
  if (isObj(root) && isObj(root['_RecordValue_'])) {
    start = root['_RecordValue_'];
    pointer = '/_RecordValue_';
  }
  const t = typeOf(start);
  if (t) general.subtitle = humanizeType(t);
  general.type = t;
  if (isObj(start) && !refOf(start) && !cutOf(start)) fill(top, start, pointer, o, general);
  else add(top, '', 'Value', start, pointer, o, general);
  return general.rows.length ? [general, ...top.groups] : top.groups;
}

/** Rows hidden by a view, and the groups left. */
export interface SheetView {
  groups: SheetGroup[];
  /** Empty rows left out. */
  hidden: number;
}

function hay(g: SheetGroup): string {
  return [...g.trail, g.title, g.subtitle ?? '', g.type ?? ''].join('\n').toLowerCase();
}

/**
 * The sections as shown: empty rows (and groups left with nothing) dropped unless `showEmpty`,
 * and, with a query, only rows with every word in their label, key, value or the titles above
 * them. A cut part stays while its title matches (its fields are not known yet).
 */
export function viewSheet(groups: SheetGroup[], query: string, showEmpty: boolean): SheetView {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  let hidden = 0;
  const go = (g: SheetGroup): SheetGroup | null => {
    const head = hay(g);
    if (g.cut) return !words.length || words.every((w) => head.includes(w)) ? g : null;
    const rows = g.rows.filter((r) => {
      if (words.length) {
        const h = `${head}\n${r.label}\n${r.key}\n${r.value.text}\n${r.raw}`.toLowerCase();
        if (!words.every((w) => h.includes(w))) return false;
      }
      if (r.empty && !showEmpty) {
        hidden++;
        return false;
      }
      return true;
    });
    const subs = g.groups.map(go).filter((x): x is SheetGroup => x !== null);
    if (!rows.length && !subs.length) return null;
    return rows.length === g.rows.length && subs.length === g.groups.length && subs.every((s, i) => s === g.groups[i]) ? g : { ...g, rows, groups: subs };
  };
  return { groups: groups.map(go).filter((x): x is SheetGroup => x !== null), hidden };
}

/** Rows in a group and below it (for a collapsed group's count). */
export function rowCount(g: SheetGroup): number {
  return g.rows.length + g.groups.reduce((n, s) => n + (s.cut ? 0 : rowCount(s)), 0);
}
