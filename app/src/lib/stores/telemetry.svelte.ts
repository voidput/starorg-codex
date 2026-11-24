/**
 * Live telemetry (experimental) state, shared by Settings > Experimental, the sidebar (the
 * screen shows only while the feature is on) and the Live telemetry screen.
 *
 * Frames arrive at most 10 a second through one `onTelemetry` listener; the latest is kept in
 * a raw state (no deep proxy), so a panel re-renders only the values it reads. Events of an
 * older stream (a lower session number) are dropped.
 *
 * The graphs' history is polled once a second while a screen watches it ([`watchHistory`]),
 * only what is new each time, so the graphs redraw at 1 Hz, never at the frame rate. A replay
 * of a recording comes through the same events, with its position in `replay`. The Game.log
 * view (Events, Sessions and People panels) comes through the same listener, at most twice a
 * second, also as one raw value.
 */
import { api, toContractError } from '@/lib/contract/client';
import {
  telemetryApi,
  type GameLogView,
  type LinkStatus,
  type TelemetryConfigureRequest,
  type TelemetryConfigureResult,
  type TelemetryFrame,
  type TelemetryHistory,
  type TelemetryRecordings,
  type TelemetryReplayControl,
  type TelemetryReplayState,
  type TelemetryStatus,
} from '@/lib/contract/telemetry';
import type { ChannelRef } from '@/lib/contract/types';
import { mergeHistory, sessionStep } from '@/lib/telemetry';
import { toast } from './toast.svelte';
import { telemetryAlerts } from './telemetryAlerts.svelte';

/** How often the graphs' history is asked for (they redraw at most this often). */
export const HISTORY_POLL_MS = 1000;

class TelemetryState {
  status = $state<TelemetryStatus | null>(null);
  /** The latest frame of the running stream. */
  frame = $state.raw<TelemetryFrame | null>(null);
  link = $state.raw<LinkStatus>({ state: 'disabled' });
  /** The Game.log reader's latest view ("Read Game.log events" on and the stream running). */
  gamelog = $state.raw<GameLogView | null>(null);
  /** Frames seen on this stream (the screen shows the rate). */
  count = $state(0);
  /** The first press of this app session was confirmed (the dialog asks once per session). */
  confirmed = $state(false);
  busy = $state(false);
  /** A start is on its way: the first one of a build scans StarCitizen.exe for a few seconds. */
  starting = $state(false);
  /** The channel picked in Settings or on the screen (`<install>/<CHANNEL>`); kept while the app runs. */
  picked = $state<string | null>(null);
  /** The replay playing instead of the game, as of its latest event. */
  replay = $state.raw<TelemetryReplayState | null>(null);
  /** The graphs' history (one sample a second), replaced whole at each poll. */
  history = $state.raw<TelemetryHistory | null>(null);
  /** The recordings list, once asked for. */
  recordings = $state.raw<TelemetryRecordings | null>(null);

  /** The feature is on: the sidebar shows the screen. */
  enabled = $derived(!!this.status?.settings.enabled);
  streaming = $derived(this.status?.streaming ?? null);

  private session = 0;
  private unlisten: Promise<() => void> | null = null;

  private listen(): void {
    this.unlisten ??= api.onTelemetry((e) => {
      const step = sessionStep(this.session, e.session);
      if (step === 'stale') return;
      if (step === 'new') {
        // A new stream (another channel): the old one's frame is not shown under it.
        this.frame = null;
        this.gamelog = null;
        this.count = 0;
        this.replay = null;
      }
      this.session = e.session;
      if (e.status) this.link = e.status;
      if (e.replay) this.replay = e.replay;
      if (e.gamelog) this.gamelog = e.gamelog;
      if (e.frame) {
        this.frame = e.frame;
        this.count++;
      }
      if (e.alerts?.length) telemetryAlerts.take(e.alerts);
    });
  }

  private take(s: TelemetryStatus): TelemetryStatus {
    this.status = s;
    if (s.session > this.session) {
      this.session = s.session;
      this.frame = null;
      this.gamelog = null;
      this.count = 0;
    }
    // Switched off: the panels empty at once (the host dropped what it held).
    if (!s.settings.enabled || !s.settings.gamelog) this.gamelog = null;
    else if (!s.settings.people && this.gamelog?.people) this.gamelog = { ...this.gamelog, people: null };
    // The answer of a start may come after the stream's first statuses.
    if (!s.streaming || this.link.state === 'disabled') this.link = s.link;
    // A replay's events are newer than the answer that started it; none in the answer ends it.
    if (!s.replay) this.replay = null;
    else if (!this.replay || this.replay.name !== s.replay.name) this.replay = s.replay;
    return s;
  }

  async load(): Promise<TelemetryStatus | null> {
    try {
      return this.take(await telemetryApi.status());
    } catch (e) {
      toast('error', 'Could not read the live telemetry state', toContractError(e).message);
      return null;
    }
  }

  /** Save the switches (throws, so the caller can show the reason inline). */
  async configure(req: TelemetryConfigureRequest): Promise<TelemetryConfigureResult> {
    this.busy = true;
    try {
      const r = await telemetryApi.configure(req);
      this.take(r.status);
      if (!r.status.settings.enabled) {
        this.frame = null;
        this.link = { state: 'disabled' };
      }
      return r;
    } catch (e) {
      // Nothing saved; read the switches and lines again so the screen shows what is there.
      void this.load();
      throw e;
    } finally {
      this.busy = false;
    }
  }

  /** A stream is wanted (the screen is open); a stop while a start is on its way wins. */
  private wanted = false;

  /** A channel asked for while a start was on its way; started after it. */
  private queued: ChannelRef | null = null;

  async start(target: ChannelRef): Promise<void> {
    // One start at a time: a second click must not run a second scan of the exe. Another
    // channel picked meanwhile starts once this one has answered.
    if (this.starting) {
      this.wanted = true;
      this.queued = target;
      return;
    }
    this.wanted = true;
    this.starting = true;
    this.listen();
    try {
      await this.unlisten;
      const s = await telemetryApi.start(target);
      if (!this.wanted) {
        this.take(await telemetryApi.stop());
        return;
      }
      this.take(s);
    } catch (e) {
      toast('error', 'Live telemetry not started', toContractError(e).message);
    } finally {
      this.starting = false;
    }
    const next = this.queued;
    this.queued = null;
    const now = this.status?.streaming;
    if (next && this.wanted && (!now || now.install !== next.install || now.channel !== next.channel)) await this.start(next);
  }

  /** The People panel's Clear: the host forgets everyone it listed. */
  async clearPeople(): Promise<void> {
    try {
      const v = await telemetryApi.clearPeople();
      if (v) this.gamelog = v;
    } catch (e) {
      toast('error', 'People not cleared', toContractError(e).message);
    }
  }

  async stop(): Promise<void> {
    this.wanted = false;
    this.queued = null;
    if (!this.status?.streaming && !this.status?.replay) return;
    try {
      this.take(await telemetryApi.stop());
      this.frame = null;
      this.replay = null;
      this.gamelog = null;
      this.link = { state: 'disabled' };
    } catch (e) {
      toast('error', 'Live telemetry not stopped', toContractError(e).message);
    }
  }

  /** "Record sessions" on or off (its own switch; throws so Settings can show why). */
  async setRecord(on: boolean): Promise<void> {
    this.busy = true;
    try {
      this.take(await telemetryApi.record(on));
    } finally {
      this.busy = false;
    }
  }

  async loadRecordings(): Promise<void> {
    try {
      this.recordings = await telemetryApi.recordings();
    } catch (e) {
      toast('error', 'Could not list the recordings', toContractError(e).message);
    }
  }

  async deleteRecording(name: string): Promise<void> {
    try {
      this.recordings = await telemetryApi.deleteRecording(name);
    } catch (e) {
      toast('error', 'Recording not deleted', toContractError(e).message);
    }
  }

  /** Play a recording into the screen in place of the stream. */
  async replayStart(name: string): Promise<void> {
    this.listen();
    try {
      await this.unlisten;
      // A start waiting for its answer must not start the game stream after this.
      this.queued = null;
      const s = await telemetryApi.replayStart(name);
      this.frame = null;
      this.count = 0;
      this.history = null;
      this.take(s);
      void this.loadRecordings();
    } catch (e) {
      toast('error', 'Replay not started', toContractError(e).message);
    }
  }

  async replayControl(c: Partial<TelemetryReplayControl>): Promise<void> {
    try {
      const s = await telemetryApi.replayControl({ playing: c.playing ?? null, seek_ms: c.seek_ms ?? null, speed: c.speed ?? null });
      if (s.replay) this.replay = s.replay;
      this.status = s;
    } catch (e) {
      toast('error', 'Replay', toContractError(e).message);
    }
  }

  private polling = 0;
  private historyTimer: ReturnType<typeof setInterval> | null = null;
  private historyBusy = false;

  /** Ask for what is new in the history (skipped while an answer is on its way or the window is hidden). */
  async pollHistory(): Promise<void> {
    if (this.historyBusy) return;
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
    this.historyBusy = true;
    try {
      const h = this.history;
      const next = await telemetryApi.history({ epoch: h?.epoch ?? null, since_ms: h?.t_ms.at(-1) ?? null });
      // A seek or a new stream meanwhile dropped the old one: merge only onto what is there now.
      this.history = mergeHistory(this.history, next);
    } catch {
      // The graphs keep what they have; the next poll tries again.
    } finally {
      this.historyBusy = false;
    }
  }

  /** Poll the history while the caller watches (a screen's mount); returns the unwatch. */
  watchHistory(): () => void {
    this.polling++;
    if (!this.historyTimer) {
      void this.pollHistory();
      this.historyTimer = setInterval(() => void this.pollHistory(), HISTORY_POLL_MS);
    }
    return () => {
      this.polling = Math.max(0, this.polling - 1);
      if (this.polling === 0 && this.historyTimer) {
        clearInterval(this.historyTimer);
        this.historyTimer = null;
      }
    };
  }
}

export const telemetry = new TelemetryState();
