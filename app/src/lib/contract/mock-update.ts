/**
 * Update commands for the in-browser mock (mock.ts), like the Rust mock's checker
 * (app/src-tauri/src/update/mock.rs): no network, seeded from samples.gen.json.
 * The default repo has an available 0.2.0 (and a 0.2.1 beta with pre-releases on); any other
 * repo looks private. A start-up check runs once per session (the 24 h throttle).
 *
 * URL switches: `?update=available` (switch on, already checked: banner shows),
 * `?update=failed` (switch on, a private mirror), `?update=skipped` (0.2.0 skipped).
 */
import type { Args, Result } from './client';
import { ContractError } from './client';
import samples from './samples.gen.json';
import type * as T from './types';

type Cmd = 'update_status' | 'check_for_updates' | 'set_update_prefs' | 'skip_update' | 'open_release_page' | 'install_update';
export type UpdateHandlers = { [K in Cmd]: (args: Args<K>) => Result<K> };

const S = samples as unknown as { update_status: T.UpdateStatus; check_for_updates: T.UpdateStatus; check_for_updates_failed: T.UpdateStatus };
const DEFAULT_REPO = S.update_status.prefs.repo;
const clone = <X>(x: X): X => structuredClone(x);
const fail = (code: T.ErrorCode, message: string): never => {
  throw new ContractError({ code, message, detail: null });
};

function beta(): T.UpdateCheck {
  const a = S.check_for_updates.check;
  if (a.state !== 'available') return a;
  const v = '0.2.1-beta.1';
  return {
    state: 'available',
    release: { ...clone(a.release), version: v, tag: `v${v}`, title: 'StarOrg Strings 0.2.1 beta 1', url: a.release.url.replace(a.release.tag, `v${v}`), prerelease: true, published: '2026-11-13T18:00:00Z', notes: [{ kind: 'heading', text: 'Bug Fixes' }, { kind: 'item', text: 'app: beta fix' }], notes_truncated: false },
  };
}

export function createUpdateMock(settings: () => T.Settings, params: URLSearchParams): UpdateHandlers {
  let st: Omit<T.UpdateStatus, 'enabled'> = clone(S.update_status);
  const view = (): T.UpdateStatus => {
    const out = { ...clone(st), enabled: settings().network.check_updates };
    return out;
  };
  const run = () => {
    st.last_checked = S.check_for_updates.last_checked;
    if (st.prefs.repo !== DEFAULT_REPO) {
      const f = clone(S.check_for_updates_failed.check);
      if (f.state === 'failed') f.message = f.message.replace(S.check_for_updates_failed.prefs.repo, st.prefs.repo);
      st.check = f;
    } else {
      st.check = st.prefs.include_prereleases ? beta() : clone(S.check_for_updates.check);
    }
  };
  const mode = params.get('update');
  if (mode) {
    settings().network.check_updates = true;
    if (mode === 'failed') st.prefs.repo = 'someone/private-fork';
    run();
    if (mode === 'skipped') st.skipped_version = '0.2.0';
  }
  return {
    update_status: () => view(),
    check_for_updates: ({ req }) => {
      const on = settings().network.check_updates;
      if (!on) return req.force ? fail('network-off', 'Turn on "Check for updates" under Settings -> Network first.') : view();
      if (req.force || st.last_checked === null) run();
      return view();
    },
    set_update_prefs: ({ req }) => {
      const repo = req.repo.trim().replace(/\/+$/, '').replace(/^https:\/\/github\.com\//, '');
      if (!/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/.test(repo) || repo.split('/').some((p) => p === '.' || p === '..')) {
        fail('invalid', 'Enter the repository as owner/name, for example voidput/starorg-strings.');
      }
      if (repo !== st.prefs.repo || req.include_prereleases !== st.prefs.include_prereleases) {
        st = { ...st, last_checked: null, check: { state: 'not-checked' } };
      }
      st.prefs = { repo, include_prereleases: req.include_prereleases };
      return view();
    },
    skip_update: ({ version }) => {
      const v = version.trim().replace(/^[vV]/, '');
      if (v && !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(v)) fail('invalid', `not a version: ${version}`);
      st.skipped_version = v || null;
      return view();
    },
    // The browser mock has no system browser to hand the link to.
    open_release_page: () => {
      if (st.check.state !== 'available') fail('not-found', 'No newer version is known. Check for updates first.');
      return null;
    },
    install_update: () => {
      if (!settings().network.check_updates) fail('network-off', 'Turn on "Check for updates" under Settings -> Network first.');
      return fail('internal', 'The mock backend does not install updates.');
    },
  };
}
