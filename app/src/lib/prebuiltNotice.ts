/**
 * One-time notice after the first "Read game data" with prebuilt game data on (the default,
 * also for people updating from a version without it): datasets of the exact installed
 * build may now come from GitHub. Shown once per installation of the UI, dismissible, with a
 * way to the switch (Settings -> Game data).
 */
import { app } from '@/lib/stores/app.svelte';
import { toast } from '@/lib/stores/toast.svelte';

export const NOTICE_KEY = 'starorg.prebuiltNotice.v1';

function seen(): boolean {
  try {
    return localStorage.getItem(NOTICE_KEY) === '1';
  } catch {
    return false;
  }
}

function markSeen(): void {
  try {
    localStorage.setItem(NOTICE_KEY, '1');
  } catch {
    // Private storage off: the notice may show again next run, which is harmless.
  }
}

/** Show the notice unless it was shown before or the switch is off. */
export function prebuiltNoticeOnce(): void {
  if (seen() || !(app.settings?.data?.prebuilt_dumps ?? true)) return;
  markSeen();
  toast(
    'warning',
    'Prebuilt game data may now be downloaded from GitHub',
    'When voidput/sc-gamedata-dumps has your exact game build, Read game data downloads the datasets it can use instead of extracting them (the request names only the build number). Turn this off in Settings → Game data.',
    {
      label: 'Settings',
      run: () => {
        app.settingsSection = 'game-data';
        app.go('settings');
      },
    },
  );
}
