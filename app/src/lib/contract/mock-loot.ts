/**
 * The Loot screen in the in-browser mock: the Rust mock's answers (app/src-tauri/src/contract/mock_loot.rs)
 * from `mock-loot.gen.json`, written by the Rust contract tests. `missing` until the channel's game data
 * is read, as in Rust. No JSON import here, so node can load it.
 */
import type * as T from './types';

export interface LootMockData {
  areas: T.LootList;
  items: T.LootList;
  area_details: T.LootDetail[];
  item_details: T.LootDetail[];
}

const missingList = (view: T.LootView): T.LootList => ({ state: 'missing', view, build: null, rows: [] });

const emptyDetail = (state: T.LootDataState, view: T.LootView): T.LootDetail => ({ state, view, entry: null, record: null, rows: [], total: 0 });

export function mockLootList(data: LootMockData, ready: boolean, view: T.LootView): T.LootList {
  if (!ready) return missingList(view);
  return structuredClone(view === 'areas' ? data.areas : data.items);
}

export function mockLootDetail(data: LootMockData, ready: boolean, view: T.LootView, id: string): T.LootDetail {
  if (!ready) return emptyDetail('missing', view);
  const key = id.trim().toLowerCase();
  const all = view === 'areas' ? data.area_details : data.item_details;
  const hit = all.find((d) => d.entry?.id.toLowerCase() === key);
  return hit ? structuredClone(hit) : emptyDetail('ready', view);
}
