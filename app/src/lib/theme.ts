/**
 * Theme resolution (spec 9.2). `applyAppearance` sets the <html> data attributes the tokens in
 * app.css read: data-theme (dark | light), data-accent, data-palette, data-perf (lite = no blur).
 * The last appearance is kept in localStorage so it is applied before first paint (main.ts);
 * the backend's Settings remain the source of truth and re-apply once loaded.
 */
import type { Appearance, Theme } from './contract/types';

export type ThemeSetting = Theme;
const KEY = 'starorg:appearance';

export function resolveTheme(setting: ThemeSetting, prefersDark: boolean): 'dark' | 'light' {
  if (setting === 'system') return prefersDark ? 'dark' : 'light';
  return setting;
}

export const isLinux = () => /Linux/i.test(navigator.userAgent) && !/Android/i.test(navigator.userAgent);

export function defaultAppearance(): Appearance {
  return { theme: 'dark', accent: 'cyan', palette: 'void', reduce_effects: isLinux() };
}

let unwatch: (() => void) | null = null;

export function applyAppearance(a: Appearance): void {
  const root = document.documentElement;
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  root.dataset.theme = resolveTheme(a.theme, mq.matches);
  root.dataset.accent = a.accent;
  root.dataset.palette = a.palette;
  if (a.reduce_effects) root.dataset.perf = 'lite';
  else delete root.dataset.perf;
  unwatch?.();
  unwatch = null;
  if (a.theme === 'system') {
    const on = (e: MediaQueryListEvent) => (root.dataset.theme = resolveTheme('system', e.matches));
    mq.addEventListener('change', on);
    unwatch = () => mq.removeEventListener('change', on);
  }
  try {
    localStorage.setItem(KEY, JSON.stringify(a));
  } catch {
    /* private window: fine */
  }
}

/** Before mount: the remembered appearance, or the defaults. `?theme=dark|light` overrides (headless renders). */
export function applyTheme(): void {
  let a = defaultAppearance();
  try {
    const s = localStorage.getItem(KEY);
    if (s) a = { ...a, ...(JSON.parse(s) as Partial<Appearance>) };
  } catch {
    /* ignore */
  }
  const forced = new URLSearchParams(window.location.search).get('theme');
  if (forced === 'dark' || forced === 'light') a = { ...a, theme: forced };
  applyAppearance(a);
}
