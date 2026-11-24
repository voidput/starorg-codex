<script lang="ts">
  /**
   * Live telemetry -> People (Settings > Experimental > "People", off by default): players
   * Game.log names, grouped by how they relate to you. Memory only, for the current Game.log;
   * never written to a file, the Activity log or a diagnostics report, and never looked up
   * anywhere. Clear forgets everyone listed.
   */
  import Eraser from '@lucide/svelte/icons/eraser';
  import Users from '@lucide/svelte/icons/users';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import type { GameLogPeople } from '@/lib/contract/telemetry';
  import { clock, groupPeople } from '@/lib/telemetry';

  let { people, onclear }: { people: GameLogPeople | null; onclear: () => void } = $props();

  const groups = $derived(groupPeople(people?.people));
</script>

<Card.Root aria-labelledby="tele-people">
  <Card.Header>
    <Card.Title id="tele-people" class="flex items-center gap-2">
      <Users class="size-4" aria-hidden="true" /> People
      {#if people}<span class="text-muted-foreground text-[13px] font-normal">{people.counts.total}</span>{/if}
      <span class="flex-1"></span>
      <Button size="xs" variant="ghost" onclick={onclear} disabled={!people || people.counts.total === 0}><Eraser aria-hidden="true" /> Clear</Button>
    </Card.Title>
    <Card.Description>Handles named in Game.log this session. Kept in memory on this computer only; nothing is looked up or sent.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3 text-[12px]">
    {#if people?.self_name}
      <p><span class="text-muted-foreground">You:</span> <span class="mono">{people.self_name}</span></p>
    {/if}
    {#if groups.length === 0}
      <p class="text-muted-foreground">Nobody yet. Crew channels, party notices, quantum calibration notices and ship kills name players here.</p>
    {:else}
      {#each groups as g (g.relation)}
        <div class="grid gap-1">
          <h3 class="eyebrow text-muted-foreground">{g.title} <span class="font-normal">({g.people.length})</span></h3>
          <ul class="grid gap-1 sm:grid-cols-2">
            {#each g.people as p (p.name)}
              <li class="flex items-baseline gap-2 rounded-md border px-2 py-1">
                <span class="mono min-w-0 truncate" title={p.name}>{p.name}</span>
                {#if p.leader}<span class="text-muted-foreground">leader</span>{/if}
                {#if p.left}<span class="text-muted-foreground">left</span>{/if}
                {#if p.ship}<span class="text-muted-foreground min-w-0 truncate">{p.ship}</span>{/if}
                <span class="flex-1"></span>
                <span class="mono text-muted-foreground">{clock(p.last_ms)}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/each}
      <p class="text-muted-foreground">“Party?” is a quantum calibration notice: usually a party member. “Nearby” are players named in ship kills around you.</p>
    {/if}
  </Card.Content>
</Card.Root>
