/**
 * Which in-game preview examples belong to which Enhancements section (spec 9.4): the
 * preview shows only the open section's. Categories are the ones `preview_rows`
 * (contract/real/diag.rs) and both mocks give: ships, components, ship-weapons,
 * fps-weapons, missiles, missions, quest-hints, obtain-hints, missing-names, ui.
 */
import type { CategoryId, PreviewItem } from './contract/types';

/** The Enhancements tabs. */
export type Section = 'general' | CategoryId | 'blueprints' | 'name-tags' | 'quest-items';

/** A name key (`item_NameX`), where name tags land; same rule as `diag::is_name`. */
export const isNameKey = (key: string) => {
  const k = key.replace(/,.*$/, '').toLowerCase();
  return k.includes('_name') && !k.includes('_desc');
};

/** A contract with a blueprint list ([Owned] / [Missing] lines). */
export const hasBlueprintList = (it: PreviewItem) => it.category === 'missions' && /\[(Owned|Missing)\]|BLUEPRINTS/.test(it.result);

const STATS: string[] = ['ships', 'components', 'ship-weapons', 'fps-weapons', 'missiles'];
const TAGGED: string[] = ['components', 'ship-weapons', 'missiles'];
const cat = (c: string) => (it: PreviewItem) => it.category === c;

/** Per section: which examples, in which order. */
const RULES: Record<Section, (items: PreviewItem[]) => PreviewItem[]> = {
  // The stats-block format and header line (one example), a fixed missing name and the
  // colour test.
  general: (items) => {
    const stat = items.filter((it) => STATS.includes(it.category) && !isNameKey(it.key));
    const sample = stat.find((it) => it.result !== it.original) ?? stat[0];
    return [...(sample ? [sample] : []), ...items.filter(cat('missing-names')), ...items.filter(cat('ui'))];
  },
  ships: (items) => items.filter(cat('ships')),
  // Stats sections show descriptions; tagged names are Name tags'.
  components: (items) => items.filter((it) => it.category === 'components' && !isNameKey(it.key)),
  'ship-weapons': (items) => items.filter((it) => it.category === 'ship-weapons' && !isNameKey(it.key)),
  'fps-weapons': (items) => items.filter(cat('fps-weapons')),
  missiles: (items) => items.filter((it) => it.category === 'missiles' && !isNameKey(it.key)),
  missions: (items) => items.filter(cat('missions')),
  // A contract with a blueprint list first, then the other contract examples.
  blueprints: (items) => {
    const m = items.filter(cat('missions'));
    return [...m.filter(hasBlueprintList), ...m.filter((it) => !hasBlueprintList(it))];
  },
  'name-tags': (items) => items.filter((it) => TAGGED.includes(it.category) && isNameKey(it.key)),
  'quest-items': (items) => items.filter(cat('quest-hints')),
  obtain: (items) => items.filter(cat('obtain-hints')),
};

/** The open section's examples; `null` section = every example. */
export function sectionPreview(items: PreviewItem[] | null | undefined, section: Section | null): PreviewItem[] {
  if (!Array.isArray(items)) return [];
  return section ? RULES[section](items) : [...items];
}
