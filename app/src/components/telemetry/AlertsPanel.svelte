<script lang="ts">
  /**
   * Live telemetry screen: the recent alerts (newest first). Alerts are checked by the backend
   * only while this screen's stream runs and Notifications is on (Settings > Experimental). The
   * list changes only when an alert fires, never per frame.
   */
  import { onMount } from 'svelte';
  import BellOff from '@lucide/svelte/icons/bell-off';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { clockText, severityLabel, severityTone } from '@/lib/telemetryAlerts';
  import { telemetryAlerts } from '@/lib/stores/telemetryAlerts.svelte';
  import { app } from '@/lib/stores/app.svelte';

  onMount(() => void telemetryAlerts.load());

  const mode = $derived(telemetryAlerts.settings?.notifications ?? 'off');
  const list = $derived(telemetryAlerts.recent);
</script>

<Card.Root aria-labelledby="tele-alerts">
  <Card.Header>
    <Card.Title id="tele-alerts">Alerts</Card.Title>
    <Card.Description>
      {#if mode === 'off'}Notifications are off.{:else if mode === 'in_app'}Toasts, no sound.{:else}Toasts and a tone{telemetryAlerts.settings?.speech ? ', spoken' : ''}.{/if}
    </Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-2">
    {#if mode === 'off'}
      <div class="text-muted-foreground flex flex-wrap items-center gap-2 text-[12px]">
        <BellOff class="size-3.5" aria-hidden="true" />
        <span>Turn on Notifications to be told about low fuel, shields, hull, G and more.</span>
        <Button size="xs" variant="outline" onclick={() => ((app.settingsSection = 'experimental'), app.go('settings'))}>Settings</Button>
      </div>
    {/if}
    {#if list.length}
      <ul class="grid max-h-64 gap-1 overflow-auto text-[12px]" aria-label="Recent alerts" aria-live="polite">
        {#each list as a, i (`${a.at_ms}-${a.rule}-${i}`)}
          <li class="flex items-center gap-2 rounded-md border px-2 py-1">
            <span class="mono text-muted-foreground w-16 shrink-0">{clockText(a.at_ms)}</span>
            <ToneBadge tone={severityTone(a.severity)} label={severityLabel(a.severity)} />
            <span class="min-w-0 flex-1 truncate" title={a.rule}>{a.text}</span>
          </li>
        {/each}
      </ul>
    {:else if mode !== 'off'}
      <p class="text-muted-foreground text-[12px]">None yet. A rule fires once when its condition starts, not on what already holds when the stream starts.</p>
    {/if}
  </Card.Content>
</Card.Root>
