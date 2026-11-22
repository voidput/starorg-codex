/**
 * The Shops screen as pure functions: category labels and order, the file-age line, the shop
 * list search and system filter, and the item filter and grouping. The data comes from
 * `shop_inventory` and `shop_items` (UEX files; categories and record links in Rust,
 * `starorg_mcp::shops`).
 */
import type { ShopCategory, ShopDirectory, ShopItemRow, ShopSummary } from './contract/types';

/** Display order, as the Rust `ShopCategory::ALL`. */
export const CATEGORY_ORDER: readonly ShopCategory[] = ['fps-weapons', 'fps-attachments', 'armour', 'ship-weapons', 'missiles', 'ship-components', 'mining', 'consumables', 'other'];

export const CATEGORY_LABEL: Record<ShopCategory, string> = {
  'fps-weapons': 'FPS weapons',
  'fps-attachments': 'FPS attachments',
  armour: 'Armour & clothing',
  'ship-weapons': 'Ship weapons',
  missiles: 'Missiles & torpedoes',
  'ship-components': 'Ship components',
  mining: 'Mining & salvage',
  consumables: 'Consumables',
  other: 'Other',
};

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** "UEX data from 21 Nov" (with the year when it is not this year's); null without a date. */
export function uexAgeText(fetched: string | null | undefined, now: Date = new Date()): string | null {
  if (!fetched) return null;
  const d = new Date(fetched);
  if (Number.isNaN(d.getTime())) return null;
  const year = d.getUTCFullYear() === now.getUTCFullYear() ? '' : ` ${d.getUTCFullYear()}`;
  return `UEX data from ${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}${year}`;
}

/** Whole days between the fetch and `now`; null without a date. */
export function uexAgeDays(fetched: string | null | undefined, now: Date = new Date()): number | null {
  if (!fetched) return null;
  const t = new Date(fetched).getTime();
  return Number.isNaN(t) ? null : Math.max(0, Math.floor((now.getTime() - t) / 86_400_000));
}

const words = (s: string): string[] => s.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);

/** Every word of the query is the start of a word of the text (or inside it, for 3+ letters). */
export function matchesWords(text: string, query: string): boolean {
  const q = words(query);
  if (!q.length) return true;
  const hay = text.toLowerCase();
  const tw = words(text);
  return q.every((w) => tw.some((t) => t.startsWith(w)) || (w.length >= 3 && hay.includes(w)));
}

export interface ShopListGroup {
  system: string;
  places: { name: string; shops: ShopSummary[] }[];
  count: number;
}

/** The left list: systems, places and shops matching the search (shop, place or system words) and the system filter (`''` = all). */
export function filterShops(dir: ShopDirectory | null, query: string, system: string): ShopListGroup[] {
  const systems = Array.isArray(dir?.systems) ? dir.systems : [];
  const out: ShopListGroup[] = [];
  for (const s of systems) {
    if (system && s.name !== system) continue;
    const places = [];
    let count = 0;
    for (const p of Array.isArray(s.locations) ? s.locations : []) {
      const shops = (Array.isArray(p.shops) ? p.shops : []).filter((x) => matchesWords(`${x.name} ${p.name} ${s.name}`, query));
      if (shops.length) {
        places.push({ name: p.name, shops });
        count += shops.length;
      }
    }
    if (places.length) out.push({ system: s.name, places, count });
  }
  return out;
}

/** The first shop of the list (selected when nothing is). */
export const firstShop = (groups: ShopListGroup[]): ShopSummary | null => groups[0]?.places[0]?.shops[0] ?? null;

/** Whether a shop id is in the list. */
export const hasShop = (groups: ShopListGroup[], id: number | null): boolean => id !== null && groups.some((g) => g.places.some((p) => p.shops.some((s) => s.id === id)));

/** Rows matching the text filter (game name, UEX name, class name). */
export function textFiltered(items: ShopItemRow[] | null | undefined, query: string): ShopItemRow[] {
  const list = Array.isArray(items) ? items : [];
  return query.trim() ? list.filter((i) => matchesWords(`${i.name} ${i.uex_name ?? ''} ${i.record?.class_name ?? ''}`, query)) : list;
}

/** The chip row: categories with their row counts, in display order. */
export function categoryChips(items: ShopItemRow[]): { category: ShopCategory; label: string; count: number }[] {
  const n = new Map<ShopCategory, number>();
  for (const i of items) n.set(i.category, (n.get(i.category) ?? 0) + 1);
  return CATEGORY_ORDER.filter((c) => n.has(c)).map((c) => ({ category: c, label: CATEGORY_LABEL[c], count: n.get(c) ?? 0 }));
}

export interface ItemGroup {
  category: ShopCategory;
  label: string;
  items: ShopItemRow[];
}

/** Rows by category (display order), each group by name; `category` null = every category. */
export function groupItems(items: ShopItemRow[], category: ShopCategory | null): ItemGroup[] {
  const by = new Map<ShopCategory, ShopItemRow[]>();
  for (const i of items) {
    if (category && i.category !== category) continue;
    const g = by.get(i.category);
    if (g) g.push(i);
    else by.set(i.category, [i]);
  }
  return CATEGORY_ORDER.filter((c) => by.has(c)).map((c) => ({
    category: c,
    label: CATEGORY_LABEL[c],
    items: [...(by.get(c) ?? [])].sort((a, b) => a.name.localeCompare(b.name, 'en')),
  }));
}

/** "Ship weapons · Missiles & torpedoes +2": a shop's biggest categories for the list. */
export function topCategories(s: ShopSummary, max = 2): string {
  const c = Array.isArray(s.categories) ? [...s.categories].sort((a, b) => b.count - a.count) : [];
  const head = c.slice(0, max).map((x) => CATEGORY_LABEL[x.category]);
  return head.join(' · ') + (c.length > max ? ` +${c.length - max}` : '');
}
