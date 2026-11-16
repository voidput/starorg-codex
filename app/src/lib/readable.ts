/**
 * Readable ids in Developer mode (addendum 7.14), pure: what a raw value shows as given the
 * backend's label and the "Show IDs" switch, the switch's per-viewer storage, and the text of
 * a field-diff value (label, number with unit). The backend resolves; this only decides what
 * is primary and what is secondary, so a missing label always falls back to the raw value.
 */
import type { Readable } from './contract/dev';

export const SHOW_IDS_KEY = 'starorg.dev.showIds';

type Store = Pick<Storage, 'getItem' | 'setItem'>;

/** The switch as this viewer left it (off when storage is blocked or empty). */
export function loadShowIds(s: Store | null): boolean {
  try {
    return s?.getItem(SHOW_IDS_KEY) === '1';
  } catch {
    return false;
  }
}

export function saveShowIds(s: Store | null, on: boolean): void {
  try {
    s?.setItem(SHOW_IDS_KEY, on ? '1' : '0');
  } catch {
    /* storage blocked: the switch still works for this session */
  }
}

const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** A value that is an id a player would want read (GUID or `@key`). */
export const idLike = (raw: string): boolean => GUID.test(raw.trim()) || /^@\S+$/.test(raw.trim());

export interface Shown {
  /** Main text. */
  primary: string;
  /** The raw id beside it (muted, mono), when the primary is a label. */
  secondary: string | null;
  /** The primary is the raw value itself (mono). */
  raw: boolean;
  /** The raw value looks like an id but nothing reads it. */
  unresolved: boolean;
  /** Our enhanced text, when it differs from stock. */
  enhanced: string | null;
  /** Tooltip: raw value, short name, enhanced text. */
  title: string;
}

/** What to show for `raw` with `label` (null = unresolved) and the switch. */
export function shown(raw: string, label: Readable | null | undefined, showIds: boolean): Shown {
  const unresolved = !label && idLike(raw);
  if (!label || showIds || !label.text) {
    return {
      primary: raw,
      secondary: null,
      raw: true,
      unresolved,
      enhanced: null,
      title: unresolved ? `${raw} (no readable name)` : label && showIds ? `${raw} = ${label.text}` : raw,
    };
  }
  const second = label.detail && label.detail !== raw ? `${label.detail} · ${raw}` : raw;
  const parts = [`${label.text}`, second];
  if (label.enhanced) parts.push(`Enhanced: ${label.enhanced}`);
  return { primary: label.text, secondary: raw, raw: false, unresolved: false, enhanced: label.enhanced, title: parts.join('\n') };
}

/** A diff record or row title: "Bracer" with the id beside, or the id alone. */
export function titled(id: string, label: string | null | undefined, showIds: boolean): { primary: string; secondary: string | null } {
  if (!label || showIds || label === id) return { primary: id, secondary: null };
  return { primary: label, secondary: id };
}

const num = (n: number) => (Number.isInteger(n) ? n.toLocaleString('en-US') : n.toLocaleString('en-US', { maximumFractionDigits: 3 }));

/** One side of a field change: its label, else the number with its unit, else the JSON text. */
export function valueText(v: unknown, label: Readable | null | undefined, unit: string | null | undefined, showIds: boolean): Shown {
  if (v === null || v === undefined) return { primary: '—', secondary: null, raw: true, unresolved: false, enhanced: null, title: 'not present' };
  if (typeof v === 'string') {
    const s = shown(v, label, showIds);
    return s.raw ? { ...s, primary: JSON.stringify(v) } : s;
  }
  if (typeof v === 'number' && !showIds) {
    const t = unit ? `${num(v)} ${unit}` : num(v);
    return { primary: t, secondary: null, raw: true, unresolved: false, enhanced: null, title: String(v) };
  }
  const t = JSON.stringify(v);
  return { primary: t, secondary: null, raw: true, unresolved: false, enhanced: null, title: t };
}

/**
 * Both sides of a field change. When the two read the same but their raw values differ (a
 * key swapped for another with the same text), the raw values lead so the change shows,
 * with the shared text as the secondary line.
 */
export function changeTexts(c: { old: unknown; new: unknown; old_label: Readable | null; new_label: Readable | null; unit: string | null }, showIds: boolean): [Shown, Shown] {
  const o = valueText(c.old, c.old_label, c.unit, showIds);
  const n = valueText(c.new, c.new_label, c.unit, showIds);
  if (o.raw || n.raw || o.primary !== n.primary || typeof c.old !== 'string' || typeof c.new !== 'string' || c.old === c.new) return [o, n];
  const same = (s: Shown, raw: string): Shown => ({ ...s, primary: raw, secondary: `same text: ${s.primary}`, raw: true });
  return [same(o, c.old), same(n, c.new)];
}

/**
 * Whether a field change matches the Diff field filter: every word of `query` (case-insensitive)
 * is in its path, read path, group, or either side as shown (names, numbers with units) or raw.
 */
export function changeMatches(
  c: { path: string; path_label: string | null; group: string | null; old: unknown; new: unknown; old_label: Readable | null; new_label: Readable | null; unit: string | null },
  query: string,
  showIds: boolean,
): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const [o, n] = changeTexts(c, showIds);
  const raw = (v: unknown) => (v === null || v === undefined ? '' : typeof v === 'string' ? v : JSON.stringify(v));
  const hay = [c.path, c.path_label, c.group, o.primary, o.secondary, n.primary, n.secondary, raw(c.old), raw(c.new)].filter(Boolean).join('\n').toLowerCase();
  return words.every((w) => hay.includes(w));
}

/** Field changes in their groups, in order (a group per item or top-level field). */
export function groupChanges<T extends { group: string | null }>(changes: T[]): { group: string | null; items: T[] }[] {
  const out: { group: string | null; items: T[] }[] = [];
  for (const c of changes) {
    const last = out.at(-1);
    if (last && last.group === c.group) last.items.push(c);
    else out.push({ group: c.group, items: [c] });
  }
  return out;
}
