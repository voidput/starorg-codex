/**
 * Theme resolution before the app mounts (spec 9.2). Settings do not exist yet (M3), so this
 * applies the defaults: System theme, cyan accent, Void palette, and Reduce effects on Linux.
 */
export type ThemeSetting = 'system' | 'dark' | 'light';

export function resolveTheme(setting: ThemeSetting, prefersDark: boolean): 'dark' | 'light' {
  if (setting === 'system') return prefersDark ? 'dark' : 'light';
  return setting;
}

export function applyTheme(setting: ThemeSetting = 'system'): void {
  const root = document.documentElement;
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  root.dataset.theme = resolveTheme(setting, mq.matches);
  root.dataset.accent ??= 'cyan';
  root.dataset.palette ??= 'void';
  // WebKitGTK handles backdrop-filter poorly on NVIDIA; Linux starts with solid surfaces.
  if (/Linux/i.test(navigator.userAgent) && !/Android/i.test(navigator.userAgent)) {
    root.dataset.perf = 'lite';
  }
  if (setting === 'system') {
    mq.addEventListener('change', (e) => {
      root.dataset.theme = resolveTheme('system', e.matches);
    });
  }
}
