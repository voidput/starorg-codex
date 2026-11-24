/**
 * Live telemetry (experimental) state, shared by Settings > Experimental, the sidebar (the
 * screen shows only while the feature is on) and the Live telemetry screen.
 *
 * Frames arrive at most 10 a second through one `onTelemetry` listener; the latest is kept in
 * a raw state (no deep proxy), so a panel re-renders only the values it reads. Events of an
 * older stream (a lower session number) are dropped.
 */
import { api, toContractError } from '@/lib/contract/client';
import { telemetryApi, type LinkStatus, type TelemetryConfigureRequest, type TelemetryConfigureResult, type TelemetryFrame, type TelemetryStatus } from '@/lib/contract/telemetry';
import type { ChannelRef } from '@/lib/contract/types';
import { toast } from './toast.svelte';

class TelemetryState {
  status = $state<TelemetryStatus | null>(null);
  /** The latest frame of the running stream. */
  frame = $state.raw<TelemetryFrame | null>(null);
  link = $state.raw<LinkStatus>({ state: 'disabled' });
  /** Frames seen on this stream (the screen shows the rate). */
  count = $state(0);
  /** The first press of this app session was confirmed (the dialog asks once per session). */
  confirmed = $state(false);
  busy = $state(false);

  /** The feature is on: the sidebar shows the screen. */
  enabled = $derived(!!this.status?.settings.enabled);
  streaming = $derived(this.status?.streaming ?? null);

  private session = 0;
  private unlisten: Promise<() => void> | null = null;

  private listen(): void {
    this.unlisten ??= api.onTelemetry((e) => {
      if (e.session < this.session) return;
      this.session = e.session;
      if (e.status) this.link = e.status;
      if (e.frame) {
        this.frame = e.frame;
        this.count++;
      }
    });
  }

  private take(s: TelemetryStatus): TelemetryStatus {
    this.status = s;
    if (s.session > this.session) {
      this.session = s.session;
      this.frame = null;
      this.count = 0;
    }
    // The answer of a start may come after the stream's first statuses.
    if (!s.streaming || this.link.state === 'disabled') this.link = s.link;
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
    } finally {
      this.busy = false;
    }
  }

  /** A stream is wanted (the screen is open); a stop while a start is on its way wins. */
  private wanted = false;

  async start(target: ChannelRef): Promise<void> {
    this.wanted = true;
    this.listen();
    await this.unlisten;
    try {
      const s = await telemetryApi.start(target);
      if (!this.wanted) {
        this.take(await telemetryApi.stop());
        return;
      }
      this.take(s);
    } catch (e) {
      toast('error', 'Live telemetry not started', toContractError(e).message);
    }
  }

  async stop(): Promise<void> {
    this.wanted = false;
    if (!this.status?.streaming) return;
    try {
      this.take(await telemetryApi.stop());
      this.frame = null;
      this.link = { state: 'disabled' };
    } catch (e) {
      toast('error', 'Live telemetry not stopped', toContractError(e).message);
    }
  }
}

export const telemetry = new TelemetryState();
