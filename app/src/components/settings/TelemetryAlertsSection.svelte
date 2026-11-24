<script lang="ts">
  /**
   * Settings -> Experimental -> Live telemetry -> Notifications (docs/user-guide.md#live-telemetry):
   * alerts over the live frames (low fuel, shields, hull, G, health, oxygen...). Their own
   * choice, Off by default: Metrics never turns them on. In app = toasts and the screen's Alerts
   * panel; In app + sound adds a short tone made in the app, and spoken callouts are a further
   * switch (off by default; this computer's own voices only). Each rule can be switched off, and
   * a threshold rule's threshold and every cooldown changed; the changes live in config.toml.
   * Alerts are checked only while a stream runs (the Live telemetry screen is open).
   */
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Volume2 from '@lucide/svelte/icons/volume-2';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Input } from '@/components/ui/input';
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { toContractError } from '@/lib/contract/client';
  import type { TelemetryAlertRule, TelemetryAlertSettings, TelemetryNotifications, TelemetryRuleOverride } from '@/lib/contract/telemetry';
  import { fromShown, isChanged, severityLabel, severityTone, thresholdText, toShown, unitSuffix, withOverride } from '@/lib/telemetryAlerts';
  import { canSpeak } from '@/lib/alertSound';
  import { telemetryAlerts } from '@/lib/stores/telemetryAlerts.svelte';

  let { enabled }: { enabled: boolean } = $props();

  $effect(() => {
    if (!telemetryAlerts.view) void telemetryAlerts.load();
  });

  const v = $derived(telemetryAlerts.view);
  const s = $derived(v?.settings ?? null);
  let error = $state<string | null>(null);

  const MODES: { id: TelemetryNotifications; label: string; hint: string }[] = [
    { id: 'off', label: 'Off', hint: 'No alerts are checked.' },
    { id: 'in_app', label: 'In app', hint: 'A toast and the Alerts panel on the Live telemetry screen. Silent.' },
    { id: 'in_app_sound', label: 'In app + sound', hint: 'Also a short tone made by the app; spoken callouts are a separate switch.' },
  ];

  async function save(next: TelemetryAlertSettings) {
    error = null;
    try {
      await telemetryAlerts.save(next);
    } catch (e) {
      error = toContractError(e).message;
    }
  }

  const setMode = (m: TelemetryNotifications) => s && m !== s.notifications && void save({ ...s, notifications: m });
  const patch = (r: TelemetryAlertRule, p: Partial<TelemetryRuleOverride> | null) => s && void save(withOverride(s, r, p));

  function num(text: string): number | null {
    const n = Number(text.trim().replace(',', '.'));
    return text.trim() === '' || !Number.isFinite(n) ? null : n;
  }

  function setThreshold(r: TelemetryAlertRule, text: string) {
    const n = num(text);
    if (n === null) return (error = `${r.id}: the threshold is a number.`);
    const value = fromShown(r, n);
    if (value === r.threshold) return;
    // A new threshold keeps the default's re-arm gap (the backend moves it with the value).
    patch(r, { value, rearm: null });
  }

  function setCooldown(r: TelemetryAlertRule, text: string) {
    const n = num(text);
    if (n === null || n < 0) return (error = `${r.id}: the cooldown is a number of seconds.`);
    if (n !== r.cooldown_s) patch(r, { cooldown_s: n });
  }

  let speechOk = $state(true);
  $effect(() => {
    if (s?.speech) speechOk = canSpeak();
  });
</script>

<div class="grid gap-2 border-t pt-3" id="telemetry-notifications">
  <div class="flex flex-wrap items-center gap-3">
    <span class="text-[13px] font-medium">Notifications</span>
    {#if s}
      <Segmented ariaLabel="Live telemetry notifications" size="sm" options={MODES} value={s.notifications} disabled={telemetryAlerts.busy} onchange={setMode} />
    {/if}
  </div>
  <p class="text-muted-foreground text-[12px]">
    Alerts while Live telemetry runs: low fuel, shields and shield faces down, hull, high G, health, oxygen, missile and proximity warnings. A separate choice, off unless you turn it on: Metrics alone never shows or plays an alert.
    {#if !enabled}They are checked only while Live telemetry is on and its screen is open.{/if}
  </p>
  {#if s && s.notifications === 'in_app_sound'}
    <SwitchRow
      id="tele-speech"
      size="sm"
      label="Spoken callouts"
      hint="Speaks a short callout after the tone, with this computer's own voices only (no online voice). Off by default."
      checked={s.speech}
      disabled={telemetryAlerts.busy}
      onchange={(on) => void save({ ...s, speech: on })}
    />
    {#if s.speech && !speechOk}<p class="text-warning text-[12px]">No voice of this computer was found, so only tones play.</p>{/if}
  {/if}
  {#if v && s && s.notifications !== 'off'}
    <details class="rounded-md border px-3 py-2 text-[12px]">
      <summary class="cursor-pointer font-medium">Rules ({v.rules.filter((r) => r.enabled).length} of {v.rules.length} on)</summary>
      <p class="text-muted-foreground mt-1.5">Each rule fires once when its condition starts, never on what holds when a stream starts or a ship appears, and again only after its cooldown. "Unverified" rules are reasonable guesses not yet seen in a recording.</p>
      <ul class="mt-2 grid gap-1.5" aria-label="Alert rules">
        {#each v.rules as r (r.id)}
          {@const th = thresholdText(r)}
          <li class="grid gap-1 rounded-md border px-2 py-1.5">
            <div class="flex flex-wrap items-center gap-2">
              <Checkbox id={`rule-${r.id}`} checked={r.enabled} disabled={telemetryAlerts.busy} onCheckedChange={(c) => patch(r, { enabled: c === true })} aria-label={`Alert ${r.id}`} />
              <label for={`rule-${r.id}`} class="mono cursor-pointer">{r.id}</label>
              <ToneBadge tone={severityTone(r.severity)} label={severityLabel(r.severity)} />
              {#if !r.verified}<ToneBadge tone="muted" label="Unverified" />{/if}
              <span class="flex-1"></span>
              <Button variant="ghost" size="xs" disabled={!r.enabled} onclick={() => void telemetryAlerts.test(r.id)} title="Show (and play) this alert once"><Volume2 aria-hidden="true" /> Test</Button>
              {#if isChanged(r)}<Button variant="ghost" size="xs" onclick={() => patch(r, null)} title="Back to the default"><RotateCcw aria-hidden="true" /> Default</Button>{/if}
            </div>
            <p class="text-muted-foreground">{r.description}</p>
            <div class="flex flex-wrap items-center gap-2">
              {#if r.threshold !== null}
                <label for={`th-${r.id}`}>Threshold</label>
                <Input id={`th-${r.id}`} class="h-6 w-20 font-mono text-[12px]" inputmode="decimal" value={String(toShown(r, r.threshold))} disabled={telemetryAlerts.busy} onchange={(e: Event) => setThreshold(r, (e.currentTarget as HTMLInputElement).value)} />
                <span class="text-muted-foreground">{unitSuffix(r)} {th}</span>
              {/if}
              <label for={`cd-${r.id}`}>Cooldown</label>
              <Input id={`cd-${r.id}`} class="h-6 w-16 font-mono text-[12px]" inputmode="decimal" value={String(r.cooldown_s)} disabled={telemetryAlerts.busy} onchange={(e: Event) => setCooldown(r, (e.currentTarget as HTMLInputElement).value)} />
              <span class="text-muted-foreground">s</span>
            </div>
          </li>
        {/each}
      </ul>
    </details>
  {/if}
  {#each v?.warnings ?? [] as w (w)}<p class="text-warning text-[12px]">Saved change skipped: {w}</p>{/each}
  {#if error}<p class="text-destructive text-[12px]" role="alert">{error}</p>{/if}
</div>
