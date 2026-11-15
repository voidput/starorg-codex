/**
 * App state over the contract: overview, the current install and channel, settings, the running
 * job. Components read `app.*` and call the actions; nothing else talks to the transport.
 */
import { api, ContractError, toContractError } from '@/lib/contract/client';
import type * as T from '@/lib/contract/types';
import { applyAppearance } from '@/lib/theme';
import { toast } from './toast.svelte';
import { isScreen, type ScreenId } from '@/lib/nav';

function initialScreen(): ScreenId {
  try {
    const s = new URLSearchParams(window.location.search).get('screen');
    return isScreen(s) ? s : 'home';
  } catch {
    return 'home';
  }
}

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

  install = $derived(this.overview?.installs.find((i) => i.id === this.installId) ?? this.overview?.installs[0] ?? null);
  channels = $derived(this.install?.channels ?? []);
  channel = $derived(this.channels.find((c) => c.channel === this.channelName) ?? this.channels[0] ?? null);
  target = $derived<T.ChannelRef | null>(this.install && this.channel ? { install: this.install.id, channel: this.channel.channel } : null);

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

  async saveSettings(s: T.Settings): Promise<void> {
    const before = this.settings;
    this.settings = s;
    applyAppearance(s.appearance);
    try {
      this.settings = await api.saveSettings(s);
    } catch (e) {
      this.settings = before;
      if (before) applyAppearance(before.appearance);
      toast('error', 'Settings not saved', toContractError(e).message);
    }
  }

  go(s: ScreenId): void {
    this.screen = s;
    this.navSeq++;
  }

  selectChannel(name: string): void {
    this.channelName = name;
  }

  /** Apply to the current channel; reports through toasts. */
  async apply(dryRun = false): Promise<T.ApplyReport | null> {
    const target = this.target;
    if (!target || this.busy) return null;
    this.busy = true;
    try {
      const r = await api.apply({ target, dry_run: dryRun });
      if (r.warnings.length) toast('warning', `Applied to ${r.channel} with ${r.warnings.length} warning(s)`, r.warnings[0]?.detail);
      else toast('success', dryRun ? `Dry run for ${r.channel}: nothing written` : `Applied to ${r.channel}`, `${r.counts.changed.toLocaleString('en-US')} strings changed`);
      return r;
    } catch (e) {
      const err = toContractError(e);
      toast('error', `Apply to ${target.channel} failed`, err.detail ?? err.message);
      return null;
    } finally {
      this.busy = false;
      this.job = null;
      await this.refresh();
    }
  }
}

export const app = new AppState();
