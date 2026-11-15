/**
 * Update check wording and banner rules (docs/updates.md). Pure functions over UpdateStatus,
 * so they run under `node --test` without a browser.
 */
import type { ReleaseView, UpdateStatus } from './contract/types';

/** The release the banner offers, or null: switch on, a newer version, not skipped, not
 *  dismissed this session. */
export function bannerRelease(s: UpdateStatus | null, dismissed: string | null = null): ReleaseView | null {
  if (!s?.enabled || s.check.state !== 'available') return null;
  const r = s.check.release;
  if (r.version === s.skipped_version || r.version === dismissed) return null;
  return r;
}

export function bannerTitle(r: ReleaseView): string {
  return `StarOrg Strings ${r.version} is available${r.prerelease ? ' (pre-release)' : ''}`;
}

/** One line for Settings -> Network under "Check for updates". */
export function checkLine(s: UpdateStatus): { tone: 'muted' | 'success' | 'info' | 'warning'; text: string } {
  if (!s.enabled) return { tone: 'muted', text: 'Off. Nothing is requested.' };
  switch (s.check.state) {
    case 'not-checked':
      return { tone: 'muted', text: 'Not checked yet.' };
    case 'up-to-date':
      return { tone: 'success', text: `Up to date (${s.current_version}).` };
    case 'available': {
      const r = s.check.release;
      const skipped = r.version === s.skipped_version ? ' You chose to skip it.' : '';
      return { tone: 'info', text: `${r.version} is available (this is ${s.current_version}).${skipped}` };
    }
    case 'failed':
      // Not alarming: the app works the same without an answer.
      return { tone: 'warning', text: s.check.message };
  }
}

/** What "Check for updates" sends, for the switch hint. */
export function requestHint(repo: string): string {
  return `When the app starts (at most once a day) and when you press Check now: one request to api.github.com for the releases of ${repo}. It sends only "StarOrg-Strings/<version>" as its user agent: no account, token or install id.`;
}
