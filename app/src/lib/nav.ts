/** Sidebar layout (spec 9.3): Home · Customize (Enhancements, Strings, Patches) · Game (Backups, Activity) · Settings. */
export type ScreenId = 'home' | 'enhancements' | 'strings' | 'patches' | 'backups' | 'activity' | 'settings';

export interface NavItem {
  id: ScreenId;
  label: string;
  description: string;
  /** Keyboard shortcut digit (Alt+n). */
  key: string;
  /** Not built yet: the screen shows a placeholder. */
  pending?: boolean;
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
      { id: 'enhancements', label: 'Enhancements', description: 'Stats and tags added to item names and descriptions', key: '2', pending: true },
      { id: 'strings', label: 'Strings', description: 'Every text key, its original and its result', key: '3', pending: true },
      { id: 'patches', label: 'Patches', description: 'Fixes for known text bugs', key: '4', pending: true },
    ],
  },
  {
    label: 'Game',
    items: [
      { id: 'backups', label: 'Backups', description: 'Restore or remove applied changes', key: '5', pending: true },
      { id: 'activity', label: 'Activity', description: 'Job log and warnings', key: '6', pending: true },
    ],
  },
  { label: null, items: [{ id: 'settings', label: 'Settings', description: 'Game location, appearance, network, command line', key: '7' }] },
];

export const NAV_ITEMS = NAV.flatMap((g) => g.items);

export const findNav = (id: ScreenId): NavItem => NAV_ITEMS.find((i) => i.id === id) ?? NAV_ITEMS[0]!;
