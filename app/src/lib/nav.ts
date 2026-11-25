/**
 * Sidebar layout (spec 9.3, addendum §6.6, §7.8): Home · Customize (Enhancements, Strings, Patches,
 * Blueprints, Ships) · Game (Shops, Commodities, Mining, Loot, and Live telemetry while Settings > Experimental >
 * Live telemetry is on) · Dev (Explorer, Datasets, Diff, Files; only with Settings > Advanced > Developer
 * mode on) · App (Backups, Activity, Settings). With Developer mode on, Alt+9 opens the Explorer and
 * Settings moves to Alt+0 (Q-D6: Datasets and Diff through Ctrl+K).
 */
export type ScreenId = 'home' | 'enhancements' | 'strings' | 'patches' | 'blueprints' | 'ships' | 'shops' | 'commodities' | 'mining' | 'loot' | 'telemetry' | 'backups' | 'activity' | 'settings' | DevScreenId;
export type DevScreenId = 'explorer' | 'datasets' | 'diff' | 'files';

export interface NavItem {
  id: ScreenId;
  label: string;
  description: string;
  /** Keyboard shortcut digit (Alt+n); empty = palette only. */
  key: string;
  /** Shown next to the label (an experimental screen). */
  badge?: string;
}

export interface NavGroup {
  label: string | null;
  items: NavItem[];
}

export const NAV: NavGroup[] = [
  { label: null, items: [{ id: 'home', label: 'Home', description: 'Your game channels and their state', key: '1' }] },
  {
    label: 'Customize',
    items: [
      { id: 'enhancements', label: 'Enhancements', description: 'Stats and tags added to item names and descriptions', key: '2' },
      { id: 'strings', label: 'Strings', description: 'Every text key, its original and its result', key: '3' },
      { id: 'patches', label: 'Patches', description: 'Fixes for known text bugs', key: '4' },
      { id: 'blueprints', label: 'Blueprints', description: 'Every blueprint, who owns it per character and channel, imports and StarOrg sync', key: '5' },
      { id: 'ships', label: 'Ships', description: 'Your fleet: order and rename ships in game lists', key: '6' },
    ],
  },
  {
    label: 'Game',
    items: [
      { id: 'shops', label: 'Shops', description: 'What each shop sells, by category, from UEX market data', key: '' },
      { id: 'commodities', label: 'Commodities', description: 'Every tradable resource: refined form, quality, where it is mined, kiosk prices', key: '' },
      { id: 'mining', label: 'Mining', description: 'Every mineable resource: where it spawns, quality by place, rock profile, difficulty and which laser it takes', key: '' },
      { id: 'loot', label: 'Loot', description: 'What drops in each loot area, and where each item drops, with rough odds per box', key: '' },
    ],
  },
  {
    label: 'App',
    items: [
      { id: 'backups', label: 'Backups', description: 'Restore or remove applied changes', key: '7' },
      { id: 'activity', label: 'Activity', description: 'Applies, launch hooks and warnings', key: '8' },
      { id: 'settings', label: 'Settings', description: 'Game, appearance, network, StarOrg Tools, hooks, diagnostics', key: '9' },
    ],
  },
];

export const NAV_ITEMS = NAV.flatMap((g) => g.items);

/** Live telemetry (experimental): in the Game group, only while the feature is on. */
export const TELEMETRY_ITEM: NavItem = { id: 'telemetry', label: 'Live telemetry', description: "The game's own live data on this computer: vitals, ship, shields, game actions", key: '', badge: 'Experimental' };
export const isTelemetryScreen = (s: ScreenId): boolean => s === 'telemetry';

export const DEV_GROUP: NavGroup = {
  label: 'Dev',
  items: [
    { id: 'explorer', label: 'Explorer', description: "The game's DataCore records: types, folders, search, references", key: '9' },
    { id: 'datasets', label: 'Datasets', description: 'Our game data and folders of JSON files as tables', key: '' },
    { id: 'diff', label: 'Diff', description: 'Compare two builds or two dataset folders', key: '' },
    { id: 'files', label: 'Files', description: "The game's Data.p4k: browse, preview textures and text, extract", key: '' },
  ],
};

export const DEV_SCREENS: readonly DevScreenId[] = ['explorer', 'datasets', 'diff', 'files'];
export const isDevScreen = (s: ScreenId): s is DevScreenId => (DEV_SCREENS as readonly string[]).includes(s);

/**
 * The sidebar groups: Live telemetry at the end of Game when `telemetry` is on; the Dev group after
 * Game when Developer mode is on, and Settings then moves to Alt+0.
 */
export function navGroups(dev: boolean, telemetry = false): NavGroup[] {
  const base = telemetry ? NAV.map((g) => (g.label === 'Game' ? { ...g, items: [...g.items, TELEMETRY_ITEM] } : g)) : NAV;
  if (!dev) return base;
  const app = base[base.length - 1]!;
  return [...base.slice(0, -1), DEV_GROUP, { ...app, items: app.items.map((i) => (i.id === 'settings' ? { ...i, key: '0' } : i)) }];
}

export const navItems = (dev: boolean, telemetry = false): NavItem[] => navGroups(dev, telemetry).flatMap((g) => g.items);

export const findNav = (id: ScreenId): NavItem => navItems(true, true).find((i) => i.id === id) ?? NAV_ITEMS[0]!;

export const isScreen = (s: string | null): s is ScreenId => navItems(true, true).some((n) => n.id === s);

/** Commodities and Mining are one screen (shared search and data) on two tabs. */
export type CommoditiesTab = 'commodities' | 'mining';
export const isCommoditiesScreen = (s: ScreenId): s is CommoditiesTab => s === 'commodities' || s === 'mining';
