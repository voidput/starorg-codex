/**
 * Collapsed sections, remembered in localStorage: whole cards (CollapsibleCard, one list for the
 * app) and the groups of the blueprint finder (one list per channel, scope and group-by). Pure:
 * the components read and write the storage, these helpers validate and update the lists.
 */

/** Ids of the collapsed cards (CollapsibleCard `key`). */
export const CARDS_KEY = 'starorg.cards.collapsed.v1';

/** Ids of the collapsed finder groups, per install/channel, scope and group-by. */
export const GROUPS_KEY = (install: string, channel: string, scope: 'channel' | 'all', group: string) => `starorg.blueprints.groups.v1:${install}/${channel}/${scope}/${group}`;

const MAX = 500;

/** A stored list of ids; anything malformed reads as "nothing collapsed". */
export function parseCollapsed(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const v: unknown = JSON.parse(raw);
    return Array.isArray(v) ? [...new Set(v.filter((x): x is string => typeof x === 'string' && x.length <= 200))].slice(0, MAX) : [];
  } catch {
    return [];
  }
}

/** The list with `id` collapsed (true) or expanded (false); order kept, no duplicates. */
export function withCollapsed(list: readonly string[], id: string, collapsed: boolean): string[] {
  const rest = list.filter((x) => x !== id);
  return collapsed ? [...rest, id].slice(-MAX) : rest;
}

/**
 * Arrow keys on a disclosure header: Left collapses, Right expands (null = not handled, also when
 * it is already in that state, so the key keeps its normal meaning).
 */
export function arrowToggle(key: string, open: boolean): boolean | null {
  if (key === 'ArrowLeft' && open) return false;
  if (key === 'ArrowRight' && !open) return true;
  return null;
}

/** Read a list, tolerating blocked storage. */
export function readCollapsed(storageKey: string): string[] {
  try {
    return parseCollapsed(localStorage.getItem(storageKey));
  } catch {
    return [];
  }
}

/** Write a list (empty removes the key), tolerating blocked storage. */
export function writeCollapsed(storageKey: string, list: readonly string[]): void {
  try {
    if (list.length) localStorage.setItem(storageKey, JSON.stringify(list));
    else localStorage.removeItem(storageKey);
  } catch {
    /* storage blocked: the state lasts for this session only */
  }
}
