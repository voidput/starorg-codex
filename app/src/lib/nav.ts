/** Sidebar layout (spec 9.3, addendum §6.6): Home · Customize (Enhancements, Strings, Patches, Blueprints, Ships) · Game (Backups, Activity) · Settings. */
export type ScreenId = 'home' | 'enhancements' | 'strings' | 'patches' | 'blueprints' | 'ships' | 'backups' | 'activity' | 'settings';

export interface NavItem {
  id: ScreenId;
  label: string;
  description: string;
  /** Keyboard shortcut digit (Alt+n). */
  key: string;
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
      { id: 'blueprints', label: 'Blueprints', description: 'Owned blueprints per character, imports and StarOrg sync', key: '5' },
      { id: 'ships', label: 'Ships', description: 'Your fleet: order and rename ships in game lists', key: '6' },
    ],
  },
  {
    label: 'Game',
    items: [
      { id: 'backups', label: 'Backups', description: 'Restore or remove applied changes', key: '7' },
      { id: 'activity', label: 'Activity', description: 'Applies, launch hooks and warnings', key: '8' },
    ],
  },
  { label: null, items: [{ id: 'settings', label: 'Settings', description: 'Game, appearance, network, StarOrg Tools, hooks, diagnostics', key: '9' }] },
];

export const NAV_ITEMS = NAV.flatMap((g) => g.items);

export const findNav = (id: ScreenId): NavItem => NAV_ITEMS.find((i) => i.id === id) ?? NAV_ITEMS[0]!;

export const isScreen = (s: string | null): s is ScreenId => NAV_ITEMS.some((n) => n.id === s);
