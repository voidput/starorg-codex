<script lang="ts">
  /**
   * The Explorer record's combat numbers, from the build's GameData (never the P4K): a ship's
   * total weapon DPS with its stock guns and missiles, a weapon's DPS, alpha, rate and fire
   * modes, a missile's damage and lock. Names link to their record. Nothing renders while the
   * GameData was not read or the record is none of these. Collapsible; kept per viewer.
   */
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right';
  import { devApi, type Combat, type CombatRef, type CombatRow } from '@/lib/contract/dev';
  import type { ChannelRef } from '@/lib/contract/types';
  import { loadCombatOpen, numText, saveCombatOpen, statText, sumRows } from '@/lib/combat';
  import { storage } from '@/lib/finder';
  import { cn } from '@/lib/utils';

  let {
    target,
    guid,
    onopen,
    class: className = '',
  }: {
    target: ChannelRef | null;
    /** The opened record's GUID. */
    guid: string;
    /** Open the record a row names. */
    onopen: (guid: string) => void;
    class?: string;
  } = $props();

  const store = storage();
  let open = $state(loadCombatOpen(store));
  let combat = $state.raw<Combat | null>(null);

  // A newer record (or channel) wins over a reply still on its way.
  let gen = 0;
  const install = $derived(target?.install ?? '');
  const channel = $derived(target?.channel ?? '');
  $effect(() => {
    const t = install && channel ? { install, channel } : null;
    const g = guid;
    const n = ++gen;
    combat = null;
    if (!t || !g) return;
    devApi
      .recordCombat({ target: t, guid: g })
      .then((c) => {
        if (n === gen) combat = c;
      })
      .catch(() => {
        // No card on failure: the record itself still shows.
        if (n === gen) combat = null;
      });
  });

  function toggle() {
    open = !open;
    saveCombatOpen(store, open);
  }

  const TITLE: Record<Combat['kind'], string> = { ship: 'Stock loadout', weapon: 'Weapon', missile: 'Missile' };
  const GRID = 'grid grid-cols-[minmax(0,1.6fr)_2.5rem_2.5rem_minmax(0,1fr)_minmax(3.5rem,5rem)_minmax(4rem,5.5rem)] gap-x-2';
  const MODE_GRID = 'grid grid-cols-[minmax(0,1fr)_repeat(5,minmax(3.5rem,5rem))] gap-x-2';
</script>

{#snippet ref(r: CombatRef | null)}
  {#if !r}
    <span class="text-muted-foreground">–</span>
  {:else if r.guid}
    {@const g = r.guid}
    <button type="button" class="text-primary inline-flex max-w-full min-w-0 items-center gap-0.5 text-left underline-offset-2 hover:underline" title={`${r.name}\n${r.item}`} onclick={() => onopen(g)}>
      <ArrowUpRight class="size-3 shrink-0" aria-hidden="true" /><span class="truncate">{r.name}</span>
    </button>
  {:else}
    <span class="min-w-0 truncate" title="{r.item} (no record in the game data)">{r.name}</span>
  {/if}
{/snippet}

{#snippet table(rows: CombatRow[], heading: string, mountLabel: string, eachLabel: string, totalLabel: string)}
  {@const sum = sumRows(rows)}
  <div role="table" aria-label={heading} class="text-[12px]">
    <div role="row" class={cn(GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
      <span role="columnheader">{heading}</span>
      <span role="columnheader" class="text-right">Size</span>
      <span role="columnheader" class="text-right">Qty</span>
      <span role="columnheader">{mountLabel}</span>
      <span role="columnheader" class="text-right">{eachLabel}</span>
      <span role="columnheader" class="text-right">{totalLabel}</span>
    </div>
    {#each rows as r, i (i)}
      <div role="row" class={cn(GRID, 'items-baseline py-0.5')}>
        <span role="cell" class="flex min-w-0">{@render ref(r.item)}</span>
        <span role="cell" class="text-right tabular-nums">{r.size === null ? '–' : `S${r.size}`}</span>
        <span role="cell" class="text-right tabular-nums">×{r.count}</span>
        <span role="cell" class="text-muted-foreground flex min-w-0 text-[11.5px]">{@render ref(r.mount)}</span>
        <span role="cell" class="text-right tabular-nums">{numText(r.each)}</span>
        <span role="cell" class="text-right font-medium tabular-nums">{numText(r.total)}</span>
      </div>
    {/each}
    {#if rows.length > 1}
      <div role="row" class={cn(GRID, 'border-t py-1 font-medium')}>
        <span role="cell">Total</span>
        <span role="cell"></span>
        <span role="cell" class="text-right tabular-nums">×{sum.count}</span>
        <span role="cell"></span>
        <span role="cell"></span>
        <span role="cell" class="text-right tabular-nums">{numText(sum.total)}</span>
      </div>
    {/if}
  </div>
{/snippet}

{#if combat}
  {@const c = combat}
  <section class={cn('bg-card shrink-0 overflow-auto border-b', className)} aria-labelledby="combat-h">
    <h3 id="combat-h" class="m-0">
      <button
        type="button"
        aria-expanded={open}
        aria-controls="combat-body"
        class="hover:bg-accent/50 focus-visible:ring-ring/60 flex w-full min-w-0 items-center gap-2 px-2 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset"
        onclick={toggle}
      >
        <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', open && 'rotate-90')} aria-hidden="true" />
        <span class="text-[12.5px] font-semibold">Combat</span>
        <span class="text-muted-foreground min-w-0 truncate text-[11px]">{TITLE[c.kind]} · from the build's game data</span>
        {#if !open && c.stats[0]}
          <span class="mono ml-auto shrink-0 text-[11.5px] tabular-nums">{c.stats[0].label} {statText(c.stats[0])}</span>
        {/if}
      </button>
    </h3>
    {#if open}
      <div id="combat-body" class="flex flex-col gap-2 px-3 pt-0.5 pb-2.5">
        {#if (Array.isArray(c.stats) && c.stats.length) || (Array.isArray(c.facts) && c.facts.length)}
          <ul class="flex flex-wrap gap-1.5" aria-label="Combat stats">
            {#each Array.isArray(c.stats) ? c.stats : [] as s (s.key)}
              <li class={cn('bg-muted/50 rounded-md border px-2 py-0.5 text-[11.5px]', s.key === 'dps' && 'border-primary/40')}>
                <span class="text-muted-foreground">{s.label}</span>
                <span class="ml-1 font-medium tabular-nums">{statText(s)}</span>
              </li>
            {/each}
            {#each Array.isArray(c.facts) ? c.facts : [] as f (f.label)}
              <li class="rounded-md border border-dashed px-2 py-0.5 text-[11.5px]">
                <span class="text-muted-foreground">{f.label}</span>
                <span class="ml-1">{f.text}</span>
              </li>
            {/each}
          </ul>
        {/if}
        {#if Array.isArray(c.weapons) && c.weapons.length}
          {@render table(c.weapons, 'Weapon', 'Mount / turret', 'DPS each', 'DPS total')}
        {/if}
        {#if Array.isArray(c.missiles) && c.missiles.length}
          {@render table(c.missiles, 'Missile', 'Rack', 'Damage each', 'Damage total')}
        {/if}
        {#if Array.isArray(c.modes) && c.modes.length}
          {@const charged = c.modes.some((m) => m.charge_time !== null)}
          <div role="table" aria-label="Fire modes" class="text-[12px]">
            <div role="row" class={cn(MODE_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
              <span role="columnheader">Fire mode</span>
              <span role="columnheader" class="text-right">Rate</span>
              <span role="columnheader" class="text-right">Alpha</span>
              <span role="columnheader" class="text-right">DPS</span>
              <span role="columnheader" class="text-right">Heat/shot</span>
              <span role="columnheader" class="text-right">{charged ? 'Charge' : ''}</span>
            </div>
            {#each c.modes as m, i (i)}
              <div role="row" class={cn(MODE_GRID, 'items-baseline py-0.5')}>
                <span role="cell" class="min-w-0 truncate" title={m.label}>{m.label}{#if m.pellets}<span class="text-muted-foreground"> · {m.pellets} pellets</span>{/if}</span>
                <span role="cell" class="text-right tabular-nums">{numText(m.rate, 'rpm')}</span>
                <span role="cell" class="text-right tabular-nums">{numText(m.alpha)}</span>
                <span role="cell" class="text-right font-medium tabular-nums">{numText(m.dps)}</span>
                <span role="cell" class="text-right tabular-nums">{numText(m.heat_per_shot)}</span>
                <span role="cell" class="text-right tabular-nums">{charged ? numText(m.charge_time, 's') : ''}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </section>
{/if}
