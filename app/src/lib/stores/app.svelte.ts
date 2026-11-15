/**
 * App state over the contract: overview, the current install and channel, settings, the running
 * job. Components read `app.*` and call the actions; nothing else talks to the transport.
 *
 * Writing actions (apply, remove, undo, restore) go through `write()`: M1 reports guards and
 * refusals as a returned report (`skipped`, `needs-decision`, ...), so each result is turned
 * into words by `describeOutcome` instead of being toasted as success.
 */
import { api, ContractError, toContractError } from '@/lib/contract/client';
import type * as T from '@/lib/contract/types';
import { applyAppearance } from '@/lib/theme';
import { toast } from './toast.svelte';
import { findNav, isScreen, type ScreenId } from '@/lib/nav';
import { describeOutcome, type Action, type Outcome } from '@/lib/outcome';
import { blockedReason } from '@/lib/messages';

function initialScreen(): ScreenId {
  try {
    const s = new URLSearchParams(window.location.search).get('screen');
    return isScreen(s) ? s : 'home';
  } catch {
    return 'home';
  }
}

/** Unsaved edits a screen holds (Enhancements). Navigation and Apply ask before dropping them. */
export interface Draft {
  label: string;
  save: () => Promise<boolean>;
  discard: () => void;
}

/** A pending step that waits on the unsaved-changes dialog. */
export interface Pending {
  kind: 'leave' | 'apply';
  /** What happens next, in words ("open Strings", "switch to HOTFIX"). */
  what: string;
  proceed: () => void;
}

const ACTION_NAME: Record<Action, string> = { apply: 'Apply', remove: 'Remove', undo: 'Undo', restore: 'Restore' };

class AppState {
  overview = $state<T.Overview | null>(null);
  settings = $state<T.Settings | null>(null);
  loading = $state(true);
  error = $state<ContractError | null>(null);
  installId = $state<string | null>(null);
  channelName = $state<string | null>(null);
  job = $state<T.JobProgress | null>(null);
  busy = $state(false);
  /** The wizard is open (first run, or re-run from Settings). */
  wizard = $state(false);
  /** Current screen; `?screen=` deep-links it (headless renders). */
  screen = $state<ScreenId>(initialScreen());
  /** Bumped on every navigation so the frame moves focus to <main>. */
  navSeq = $state(0);
  /** Refresh counter: screens reload their data when this or the channel changes. */
  rev = $state(0);
  /** Unsaved edits of the open screen, registered by that screen. */
  draft = $state.raw<Draft | null>(null);
  pending = $state.raw<Pending | null>(null);

  install = $derived(this.overview?.installs.find((i) => i.id === this.installId) ?? this.overview?.installs[0] ?? null);
  channels = $derived(this.install?.channels ?? []);
  channel = $derived(this.channels.find((c) => c.channel === this.channelName) ?? this.channels[0] ?? null);
  target = $derived<T.ChannelRef | null>(this.install && this.channel ? { install: this.install.id, channel: this.channel.channel } : null);
  /** Stable key of the target, for effects that must not re-run on every refresh. */
  targetKey = $derived(this.install && this.channel ? `${this.install.id}/${this.channel.channel}` : '');
  /** Why writing to the game is blocked now (shown next to the status pill; disables writes). */
  blocked = $derived(blockedReason(this.channel, this.busy));

  private unlisten: (() => void) | null = null;

  async init(): Promise<void> {
    this.unlisten ??= await api.onJob((p) => {
      this.job = p.phase === 'running' ? p : null;
    });
    await Promise.all([this.refresh(), this.loadSettings()]);
    if (this.overview?.first_run) this.wizard = true;
  }

  async refresh(): Promise<void> {
    this.loading = true;
    try {
      const o = await api.overview();
      this.overview = o;
      this.error = null;
      this.rev++;
      const inst = o.installs.find((i) => i.id === (this.installId ?? o.active_install)) ?? o.installs.find((i) => i.saved) ?? o.installs[0];
      this.installId = inst?.id ?? null;
      if (!this.channelName || !inst?.channels.some((c) => c.channel === this.channelName)) {
        this.channelName = o.default_channel ?? inst?.channels.find((c) => c.set_up)?.channel ?? inst?.channels[0]?.channel ?? null;
      }
    } catch (e) {
      this.error = toContractError(e);
    } finally {
      this.loading = false;
    }
  }

  async loadSettings(): Promise<void> {
    try {
      this.settings = await api.getSettings();
      applyAppearance(this.settings.appearance);
    } catch (e) {
      toast('error', 'Could not read settings', toContractError(e).message);
    }
  }

  /** Saves and resolves once Rust has written app.json; false when the save failed. */
  async saveSettings(s: T.Settings): Promise<boolean> {
    const before = this.settings;
    this.settings = s;
    applyAppearance(s.appearance);
    try {
      this.settings = await api.saveSettings(s);
      return true;
    } catch (e) {
      this.settings = before;
      if (before) applyAppearance(before.appearance);
      toast('error', 'Settings not saved', toContractError(e).message);
      return false;
    }
  }

  /** Run `proceed` now, or after the unsaved-changes dialog when a screen holds a draft. */
  private guarded(kind: Pending['kind'], what: string, proceed: () => void): void {
    if (this.draft) this.pending = { kind, what, proceed };
    else proceed();
  }

  go(s: ScreenId): void {
    if (s === this.screen) {
      this.navSeq++;
      return;
    }
    this.guarded('leave', `open ${findNav(s).label}`, () => {
      this.screen = s;
      this.navSeq++;
    });
  }

  selectChannel(name: string): void {
    if (name === this.channelName) return;
    this.guarded('leave', `switch to ${name}`, () => {
      this.channelName = name;
    });
  }

  /** Header / palette Apply: asks first when the open screen has unsaved edits. */
  requestApply(): void {
    if (this.blocked || !this.channel?.set_up) return;
    this.guarded('apply', `apply to ${this.channel.channel}`, () => void this.apply());
  }

  /** Stop the running job, when it can be stopped. */
  async cancel(): Promise<void> {
    const j = this.job;
    if (!j?.cancellable) return;
    try {
      await api.cancelJob(j.job);
    } catch (e) {
      toast('error', 'Could not stop the job', toContractError(e).message);
    }
  }

  /** Show an outcome as a toast; warnings and errors keep a way to the log. */
  report(o: Outcome): void {
    const toActivity = { label: 'View in Activity', run: () => this.go('activity') };
    toast(o.tone, o.title, o.detail, o.tone === 'warning' || o.tone === 'error' ? toActivity : undefined);
  }

  /**
   * Run one writing action and report what it did. With `inline`, a not-done outcome is thrown
   * (as a ContractError) so the dialog that started it stays open and shows the reason.
   */
  async write(action: Action, run: () => Promise<T.ApplyReport>, opts: { inline?: boolean } = {}): Promise<Outcome | null> {
    if (this.busy) return null;
    this.busy = true;
    try {
      const o = describeOutcome(await run(), action);
      if (!o.done && opts.inline) throw new ContractError({ code: 'needs-decision', message: o.title, detail: o.detail });
      this.report(o);
      return o;
    } catch (e) {
      const err = toContractError(e);
      if (opts.inline) throw err;
      if (err.code === 'cancelled') toast('info', err.message);
      else toast('error', `${ACTION_NAME[action]} on ${this.target?.channel ?? 'the channel'} failed`, err.detail ?? err.message, { label: 'View in Activity', run: () => this.go('activity') });
      return null;
    } finally {
      this.busy = false;
      this.job = null;
      await this.refresh();
    }
  }

  /** Apply to the current channel; reports through toasts. */
  async apply(dryRun = false): Promise<Outcome | null> {
    const target = this.target;
    if (!target || this.blocked) return null;
    return this.write('apply', () => api.apply({ target, dry_run: dryRun }));
  }
}

export const app = new AppState();
