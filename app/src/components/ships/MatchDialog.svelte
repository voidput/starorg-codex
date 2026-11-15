<script lang="ts">
  /**
   * Unmatched review (addendum §6.6): one row per pending import row, with a pick over the
   * catalogue (candidates first for ambiguous rows) and "Not in the game" to stop asking. Each pick
   * is saved at once (fleet_match); a matched ship joins My ships at the end.
   */
  import Search from '@lucide/svelte/icons/search';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, FleetHull, FleetView, PendingShip } from '@/lib/contract/types';
  import { compareNames } from '@/lib/shipnames';

  let {
    open = $bindable(false),
    target,
    view,
    onchange,
  }: { open: boolean; target: ChannelRef; view: FleetView; onchange: (v: FleetView) => void } = $props();

  const uid = $props.id();
  let filter = $state('');
  let busy = $state<string | null>(null);
  let failed = $state<string | null>(null);
  /** Row name -> the hull chosen in its list (before Save). */
  let chosen = $state<Record<string, string>>({});

  $effect(() => {
    if (open) {
      filter = '';
      failed = null;
      chosen = {};
    }
  });

  const sorted = $derived([...view.catalog].sort((a, b) => compareNames(a.name, b.name)));
  function options(p: PendingShip): FleetHull[] {
    const f = filter.trim().toLowerCase();
    const cands = sorted.filter((h) => p.candidates.includes(h.hull));
    const rest = sorted.filter((h) => !p.candidates.includes(h.hull) && (!f || h.name.toLowerCase().includes(f)));
    return [...cands, ...rest.slice(0, 60)];
  }

  async function save(p: PendingShip, hull: string | null) {
    busy = p.name;
    failed = null;
    try {
      onchange(await api.fleetMatch({ target, name: p.name, hull }));
    } catch (e) {
      failed = toContractError(e).message;
    } finally {
      busy = null;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[min(88dvh,44rem)] flex-col overflow-hidden sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>Match ships by hand</Dialog.Title>
      <Dialog.Description>
        These rows of your import matched no ship of {target.channel}, or more than one. Pick the ship, or mark it as not in the game (concept ships) so it is not asked again. A pick is remembered for the next import.
      </Dialog.Description>
    </Dialog.Header>
    <div class="relative">
      <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
      <Input type="search" aria-label="Filter the ship lists" placeholder="Filter the ship lists" class="h-8 pl-8" bind:value={filter} />
    </div>
    <ul class="-mr-2 grid min-h-0 flex-1 content-start gap-2 overflow-y-auto pr-2" aria-label="Rows to match">
      {#each view.pending as p, i (p.name)}
        <li class="grid gap-1.5 rounded-md border p-2.5">
          <div class="flex flex-wrap items-baseline gap-x-2">
            <span class="font-medium">{p.name}</span>
            {#if p.count > 1}<span class="mono text-muted-foreground text-[12px]">×{p.count}</span>{/if}
            {#if p.maker}<span class="text-muted-foreground text-[12px]">{p.maker}</span>{/if}
            <span class="text-muted-foreground text-[12px]">{p.candidates.length ? `${p.candidates.length} ships fit` : 'no ship of this name'}</span>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <label class="sr-only" for="{uid}-{i}">Ship for {p.name}</label>
            <select
              id="{uid}-{i}"
              class="dark:bg-input/30 border-input focus-visible:border-ring focus-visible:ring-ring/50 h-8 min-w-0 flex-1 rounded-lg border bg-transparent px-2 text-sm outline-none focus-visible:ring-3"
              value={chosen[p.name] ?? ''}
              onchange={(e) => (chosen = { ...chosen, [p.name]: (e.currentTarget as HTMLSelectElement).value })}
            >
              <option value="" disabled>Choose a ship…</option>
              {#each options(p) as h (h.hull)}
                <option value={h.hull}>{h.name.trim()}{p.candidates.includes(h.hull) ? ' (fits)' : ''}</option>
              {/each}
            </select>
            <Button size="sm" disabled={!chosen[p.name] || busy !== null} onclick={() => save(p, chosen[p.name]!)}>Match</Button>
            <Button size="sm" variant="outline" disabled={busy !== null} onclick={() => save(p, null)}>Not in the game</Button>
          </div>
        </li>
      {:else}
        <li class="text-muted-foreground py-6 text-center">All rows are matched.</li>
      {/each}
    </ul>
    {#if failed}<p role="alert" class="text-destructive">{failed}</p>{/if}
    <Dialog.Footer class="shrink-0">
      <Button variant="ghost" onclick={() => (open = false)}>Done</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
