/** Sidebar layout (spec 9.3). Only Home has content in M0; the rest are placeholders. */
export type ScreenId =
  | 'home'
  | 'enhancements'
  | 'strings'
  | 'patches'
  | 'backups'
  | 'activity'
  | 'settings';

export interface NavItem {
  id: ScreenId;
  label: string;
  description: string;
  milestone?: string;
}

export interface NavGroup {
  label: string | null;
  items: NavItem[];
}

export const NAV: NavGroup[] = [
  {
    label: null,
    items: [{ id: 'home', label: 'Home', description: 'Your game channels and their state' }],
  },
  {
    label: 'Customize',
    items: [
      { id: 'enhancements', label: 'Enhancements', description: 'Stats and tags added to item names and descriptions', milestone: 'M2' },
      { id: 'strings', label: 'Strings', description: 'Every text key, its original and its result', milestone: 'M3' },
      { id: 'patches', label: 'Patches', description: 'Fixes for known text bugs', milestone: 'M2' },
    ],
  },
  {
    label: 'Game',
    items: [
      { id: 'backups', label: 'Backups', description: 'Restore or remove applied changes', milestone: 'M1' },
      { id: 'activity', label: 'Activity', description: 'Job log and warnings', milestone: 'M3' },
    ],
  },
  {
    label: null,
    items: [{ id: 'settings', label: 'Settings', description: 'Game location, appearance, network, command line', milestone: 'M3' }],
  },
];

export const findNav = (id: ScreenId): NavItem =>
  NAV.flatMap((g) => g.items).find((i) => i.id === id) ?? NAV[0].items[0];
