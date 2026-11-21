/**
 * The Explorer record's Combat card, as pure functions: number text, the sum row of a loadout
 * table and the per-viewer open/closed state. The numbers themselves come from the build's
 * GameData (`dev_record_combat`).
 */
import type { CombatRow, Stat } from './contract/dev';
import { numberCell } from './readable.ts';

export const COMBAT_OPEN_KEY = 'starorg.dev.combatOpen';

type Store = Pick<Storage, 'getItem' | 'setItem'>;

/** Open unless this viewer closed it (open when storage is blocked or empty). */
export function loadCombatOpen(s: Store | null): boolean {
  try {
    return s?.getItem(COMBAT_OPEN_KEY) !== '0';
  } catch {
    return true;
  }
}

export function saveCombatOpen(s: Store | null, open: boolean): void {
  try {
    s?.setItem(COMBAT_OPEN_KEY, open ? '1' : '0');
  } catch {
    /* storage blocked: the card still opens and closes for this session */
  }
}

/** A number as the Diff shows numbers ("1,537.5 m/s"); a dash when missing. */
export function numText(v: number | null | undefined, unit: string | null = null): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return numberCell(String(v), unit, false) ?? '–';
}

export const statText = (s: Stat): string => numText(s.value, s.unit);

/** The sum row of a loadout table: how many items, and the summed totals (null when none has one). */
export function sumRows(rows: CombatRow[] | null | undefined): { count: number; total: number | null } {
  if (!Array.isArray(rows)) return { count: 0, total: null };
  let count = 0;
  let total: number | null = null;
  for (const r of rows) {
    count += r.count;
    if (r.total !== null) total = Math.round(((total ?? 0) + r.total) * 100) / 100;
  }
  return { count, total };
}
