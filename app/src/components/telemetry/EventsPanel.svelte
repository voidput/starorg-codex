<script lang="ts">
  /**
   * Live telemetry -> Events (Settings > Experimental > "Read Game.log events"): what the
   * channel's own Game.log says that the telemetry API does not: session, where you are,
   * quantum travel, nearby ships, deaths and respawns. The view arrives at most twice a second
   * as one value; the list shows the newest 100 (the host keeps 200).
   */
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import * as Card from '@/components/ui/card';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import type { GameLogView } from '@/lib/contract/telemetry';
  import { clock, eventLabel, eventTone, recentEvents, whereText } from '@/lib/telemetry';

  let { view }: { view: GameLogView | null } = $props();

  const events = $derived(recentEvents(view));
  const s = $derived(view?.signals ?? null);
  const nearby = $derived(Array.isArray(s?.nearby) ? s.nearby.slice(0, 8) : []);
</script>

<Card.Root aria-labelledby="tele-events">
  <Card.Header>
    <Card.Title id="tele-events" class="flex items-center gap-2"><ScrollText class="size-4" aria-hidden="true" /> Events</Card.Title>
    <Card.Description>From this channel's Game.log on this computer. Unofficial debug lines: a game patch can change or drop them.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    {#if !view}
      <p class="text-muted-foreground text-[12px]">Waiting for Game.log…</p>
    {:else if !view.found}
      <p class="text-muted-foreground text-[12px]">No Game.log in this channel's folder yet. It appears when the game starts.</p>
    {:else}
      <dl class="grid grid-cols-2 gap-x-3 gap-y-1.5 text-[12px] sm:grid-cols-4">
        <div><dt class="text-muted-foreground">Where</dt><dd>{whereText(view)}</dd></div>
        <div>
          <dt class="text-muted-foreground">Shard</dt>
          <dd class="mono">{view.session.shard_short ?? '—'}{#if view.session.region_name}<span class="text-muted-foreground"> {view.session.region_name}</span>{/if}</dd>
        </div>
        <div>
          <dt class="text-muted-foreground">Quantum</dt>
          <dd>
            {#if s?.qt.state === 'routed'}Route to <span class="mono">{s.qt.destination}</span>{#if s.qt.fuel_estimate !== null}<span class="text-muted-foreground"> (fuel {Math.round(s.qt.fuel_estimate)})</span>{/if}
            {:else if s?.qt.last_arrival}Arrived at <span class="mono">{s.qt.last_arrival}</span>
            {:else}Idle{/if}
          </dd>
        </div>
        <div><dt class="text-muted-foreground">Deaths / respawns</dt><dd class="mono">{s?.deaths ?? 0} / {s?.respawns ?? 0}</dd></div>
        <div><dt class="text-muted-foreground">Own ship</dt><dd>{s?.own_ship?.display ?? '—'}</dd></div>
        <div>
          <dt class="text-muted-foreground">Build</dt>
          <dd class="mono">{view.session.build ?? '—'}{#if view.session.channel}<span class="text-muted-foreground"> {view.session.channel}</span>{/if}</dd>
        </div>
        {#if view.session.queue_position !== null}
          <div><dt class="text-muted-foreground">Login queue</dt><dd class="mono">#{view.session.queue_position}</dd></div>
        {/if}
        {#if view.session.disconnects > 0}
          <div><dt class="text-muted-foreground">Disconnects</dt><dd>{view.session.disconnects}{#if view.session.last_disconnect_reason}<span class="text-muted-foreground"> · {view.session.last_disconnect_reason}</span>{/if}</dd></div>
        {/if}
      </dl>

      {#if nearby.length}
        <div class="grid gap-1">
          <h3 class="eyebrow text-muted-foreground">Ships nearby <span class="font-normal normal-case">({s?.nearby_count ?? 0} seen this shard; parked and AI ships count too)</span></h3>
          <ul class="flex flex-wrap gap-1.5 text-[12px]">
            {#each nearby as n (n.class)}
              <li class="rounded-md border px-2 py-0.5">{n.display}{#if n.count > 1}<span class="text-muted-foreground"> ×{n.count}</span>{/if}</li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if events.length}
        <ol class="grid max-h-80 gap-1 overflow-auto text-[12px]" aria-label="Game.log events, newest first">
          {#each events as e (e.seq)}
            <li class="flex items-baseline gap-2">
              <span class="mono text-muted-foreground w-16 shrink-0">{clock(e.t_ms)}</span>
              <ToneBadge tone={eventTone(e.kind)} label={eventLabel(e.kind)} />
              <span class="min-w-0 break-words">{e.detail}</span>
            </li>
          {/each}
        </ol>
      {:else}
        <p class="text-muted-foreground text-[12px]">No events yet.</p>
      {/if}
    {/if}
  </Card.Content>
</Card.Root>
