/**
 * The Shops screen's item search in the in-browser mock: every mock item with its shops (the Rust mock's
 * `mock_item_catalogue`, written to samples.gen.json by the contract tests), matched by name words here.
 * Rust ranks with the fuzzy scorer of the `search` tool; this keeps the catalogue order (by name) and
 * only matches words, which is enough for the synthetic catalogue. `missing` without the UEX files.
 */
import type * as T from './types';
import { matchesWords } from '../shops.ts';

export type ShopItemCatalogue = { item: T.ShopItemMatch; offers: T.ShopItemOffer[] }[];

/** Rust's `ITEM_MATCH_LIMIT` and `ITEM_OFFER_LIMIT`. */
const MATCH_LIMIT = 40;
const OFFER_LIMIT = 200;

export function mockItemSearch(
  catalogue: ShopItemCatalogue,
  ready: boolean,
  folder: string | null,
  fetched: string | null,
  linked: string | null,
  query: string,
  key: string | null,
): T.ShopItemSearch {
  const empty: T.ShopItemSearch = { state: 'missing', folder, fetched: null, linked: null, query, items: [], total: 0, item: null, offers: [], offer_count: 0 };
  if (!ready) return empty;
  const list = Array.isArray(catalogue) ? catalogue : [];
  const found = query.trim() ? list.filter((c) => matchesWords(`${c.item.name} ${c.item.uex_name ?? ''} ${c.item.record?.class_name ?? ''}`, query)) : [];
  const chosen = (key ? list.find((c) => c.item.key === key) : undefined) ?? found[0] ?? null;
  const offers = chosen ? chosen.offers : [];
  return structuredClone({
    ...empty,
    state: 'ready',
    fetched,
    linked,
    items: found.slice(0, MATCH_LIMIT).map((c) => c.item),
    total: found.length,
    item: chosen?.item ?? null,
    offers: offers.slice(0, OFFER_LIMIT),
    offer_count: offers.length,
  });
}
