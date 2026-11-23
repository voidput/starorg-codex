/**
 * The Loot screen in the in-browser mock: the Rust mock's answers (app/src-tauri/src/contract/mock_loot.rs)
 * from `mock-loot.gen.json`, written by the Rust contract tests: both lists, and every detail ungrouped
 * with all its rows, which this file filters and groups as `LootScreen::detail` does. `missing` until the
 * channel's game data is read, as in Rust. No JSON import here, so node can load it.
 */
import type * as T from './types';
import { defaultGroupBy, groupDrops } from '../lootgroups.ts';

export interface LootMockData {
  areas: T.LootList;
  items: T.LootList;
  /** Ungrouped (`group_by: none`): one group with every row. */
  area_details: T.LootDetail[];
  item_details: T.LootDetail[];
}

const missingList = (view: T.LootView): T.LootList => ({ state: 'missing', view, build: null, rows: [] });

const emptyDetail = (state: T.LootDataState, view: T.LootView, group_by: T.LootGroupBy = defaultGroupBy(view)): T.LootDetail => ({
  state,
  view,
  entry: null,
  record: null,
  group_by,
  groups: [],
  total: 0,
  all: 0,
});

export function mockLootList(data: LootMockData, ready: boolean, view: T.LootView): T.LootList {
  if (!ready) return missingList(view);
  return structuredClone(view === 'areas' ? data.areas : data.items);
}

/** Whether a drop holds every word (as `LootScreen::detail` matches them). */
function matches(d: T.LootDrop, words: string[]): boolean {
  const hay = `${d.name} ${d.group} ${d.container} ${d.rarity ?? ''} ${d.id}`.toLowerCase();
  return words.every((w) => hay.includes(w));
}

export function mockLootDetail(
  data: LootMockData,
  ready: boolean,
  view: T.LootView,
  id: string,
  filter: string | null = null,
  groupBy: T.LootGroupBy | null = null,
  groups: string[] | null = null,
): T.LootDetail {
  if (!ready) return emptyDetail('missing', view);
  const by = groupBy ?? defaultGroupBy(view);
  const key = id.trim().toLowerCase();
  const all = view === 'areas' ? data.area_details : data.item_details;
  const hit = all.find((d) => d.entry?.id.toLowerCase() === key);
  if (!hit) return emptyDetail('ready', view, by);
  const out = structuredClone(hit);
  const words = (filter ?? '').toLowerCase().split(/\s+/).filter(Boolean);
  // The mock's details are never capped, so its one group holds every drop.
  const rows = (out.groups[0]?.rows ?? []).filter((d) => !words.length || matches(d, words));
  out.group_by = by;
  out.groups = groupDrops(rows, by, groups, words.length > 0);
  out.total = rows.length;
  return out;
}
