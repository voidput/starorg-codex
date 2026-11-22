<script lang="ts">
  /**
   * Where the Explorer record comes from: the shops that sell it (UEX prices the user put in
   * the cache, cheapest first), the loot containers that can hold it (area, type, size,
   * rarity, rough odds per box) and, for an ore, the bodies and belts that spawn it and
   * the spots in caves, asteroid bases and events that place it, and the quality it comes in. Closed until the viewer opens
   * it: the first open per build extracts the loot and mining data (shared with the MCP
   * server's cache). Kept open or closed per viewer.
   */
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { devApi, type LootContainer, type RecordSources } from '@/lib/contract/dev';
  import type { ChannelRef } from '@/lib/contract/types';
  import {
    SOURCES_OPEN_KEY,
    beltBars,
    beltPlace,
    isEmpty,
    pctText,
    priceText,
    qualityText,
    sourcesSummary,
    spotPlace,
  } from '@/lib/sources';
  import { cn } from '@/lib/utils';
  import CollapsibleCard from './CollapsibleCard.svelte';

  let {
    target,
    name,
    class: className = '',
  }: {
    target: ChannelRef | null;
    /** The opened record's name (`Type.ShortName`). */
    name: string;
    class?: string;
  } = $props();

  let open = $state(false);
  let sources = $state.raw<RecordSources | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // A newer record (or channel) wins over a reply still on its way.
  let gen = 0;
  const install = $derived(target?.install ?? '');
  const channel = $derived(target?.channel ?? '');
  $effect(() => {
    const t = install && channel ? { install, channel } : null;
    const n = ++gen;
    const want = open;
    sources = null;
    error = null;
    loading = false;
    if (!t || !name || !want) return;
    loading = true;
    devApi
      .recordSources({ target: t, name })
      .then((s) => {
        if (n === gen) sources = s;
      })
      .catch((e: unknown) => {
        if (n === gen) error = e instanceof Error ? e.message : String((e as { message?: string })?.message ?? e);
      })
      .finally(() => {
        if (n === gen) loading = false;
      });
  });

  const summary = $derived(sourcesSummary(sources));
  const LOOT_GRID = 'grid grid-cols-[minmax(0,1.5fr)_minmax(0,1fr)_minmax(0,0.8fr)_3.5rem_4.5rem_4.5rem] gap-x-2';
  const BELT_GRID = 'grid grid-cols-[minmax(0,1.3fr)_3.5rem_4rem_minmax(0,1fr)_minmax(4rem,6rem)] gap-x-2';
  const SHOP_GRID = 'grid grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_6rem_5.5rem] gap-x-2';
  const SPOT_GRID = 'grid grid-cols-[minmax(0,1.3fr)_minmax(0,1.2fr)_4.5rem] gap-x-2';

  const RARITY: Record<string, string> = {
    rare: 'text-sky-600 dark:text-sky-400',
    epic: 'text-violet-600 dark:text-violet-400',
    legendary: 'text-amber-600 dark:text-amber-400',
  };
  const rarityClass = (r: string | null) => (r ? (RARITY[r.toLowerCase()] ?? '') : '');
  const kindText = (c: LootContainer) => (c.kind === 'fixed' ? 'always holds it' : c.kind === 'loose' ? 'lies loose' : null);
</script>

{#if target && name}
  <CollapsibleCard
    id="sources"
    title="Sources"
    subtitle="Shops, loot containers, where it is mined and its quality"
    storageKey={SOURCES_OPEN_KEY}
    defaultOpen={false}
    bind:open
    class={className}
    bodyClass="flex flex-col gap-2.5 px-3 pt-0.5 pb-2.5 text-[12px]"
  >
    {#snippet aside(isOpen: boolean)}
      {#if isOpen && summary}
        <span class="text-muted-foreground ml-auto shrink-0 text-[11px] tabular-nums">{summary}</span>
      {/if}
    {/snippet}
    {#if loading}
      <p class="text-muted-foreground flex items-center gap-1.5 text-[11.5px]" role="status">
        <LoaderCircle class="size-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" />
        Reading the loot and mining data. The first time per build extracts it from Data.p4k (a minute or two).
      </p>
    {:else if error}
      <p class="text-destructive text-[11.5px]" role="alert">{error}</p>
    {:else if sources && isEmpty(sources)}
      <p class="text-muted-foreground text-[11.5px]">
        {#if sources.loot?.can_loot === false}
          Not loot: {sources.loot.reason ?? 'the game bars it from loot tables'}.
        {:else}
          Not sold in any shop, not in any loot container or loot table, and not a mined ore.
        {/if}
      </p>
    {:else if sources}
      {#if sources.buy}
        {@const b = sources.buy}
        <div class="flex flex-col gap-1">
          <p class="m-0 font-medium">
            Buy
            <span class="text-muted-foreground ml-1 text-[11px] font-normal">UEX prices, player-reported</span>
          </p>
          {#if !b.loaded}
            <p class="text-muted-foreground m-0 text-[11.5px]">
              No shop data: shop stock is in no game file. Turn on "Download UEX market data" in Settings -> Network and
              press Fetch now, or put <span class="mono">uex-item-prices.json</span> and <span class="mono">uex-terminals.json</span> in
              <span class="mono select-all">{b.folder ?? 'the UEX folder'}</span>.
            </p>
          {:else if b.offers.length}
            <div role="table" aria-label="Shops that sell {sources.loot?.name ?? name}">
              <div role="row" class={cn(SHOP_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
                <span role="columnheader">Shop</span>
                <span role="columnheader">Location</span>
                <span role="columnheader" class="text-right">Price</span>
                <span role="columnheader" class="text-right" title="Day UEX last confirmed the price">Updated</span>
              </div>
              {#each b.offers as o, i (i)}
                <div role="row" class={cn(SHOP_GRID, 'items-baseline py-0.5')}>
                  <span role="cell" class="min-w-0 truncate" title={o.terminal}>
                    {o.terminal}{#if o.not_on_live}<span class="text-muted-foreground"> · not on LIVE</span>{/if}
                  </span>
                  <span role="cell" class="text-muted-foreground min-w-0 truncate" title={o.location}>{o.location}</span>
                  <span role="cell" class="text-right font-medium tabular-nums">{priceText(o.price_buy)}</span>
                  <span
                    role="cell"
                    class={cn('text-right tabular-nums', o.stale ? 'text-muted-foreground/70' : 'text-muted-foreground')}
                    title={o.stale ? 'Not confirmed for over 30 days' : undefined}>{o.updated ?? '–'}{o.stale ? ' ⚠' : ''}</span
                  >
                </div>
              {/each}
            </div>
            {#if b.total > b.offers.length}
              <p class="text-muted-foreground m-0 text-[11px]">{b.total - b.offers.length} dearer shops not shown.</p>
            {/if}
          {:else}
            <p class="text-muted-foreground m-0 text-[11.5px]">No shop in the UEX data sells it.</p>
          {/if}
        </div>
      {/if}
      {#each Array.isArray(sources.mined) ? sources.mined : [] as m (m.ore)}
        {@const bars = beltBars(m.belts)}
        <div class="flex flex-col gap-1">
          <p class="m-0 font-medium">
            Mined: {m.name}
            <span class="text-muted-foreground mono text-[11px] font-normal">{m.ore}</span>
          </p>
          {#if !m.belts.length && m.spots.length}
            <p class="text-muted-foreground m-0 text-[11.5px]">No body or belt spawns it: only the spots below place it.</p>
          {/if}
          {#if m.belts.length}
            <div role="table" aria-label="Bodies and belts that spawn {m.name}">
              <div role="row" class={cn(BELT_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
                <span role="columnheader">Body or belt</span>
                <span role="columnheader">Mining</span>
                <span role="columnheader">Tier</span>
                <span role="columnheader">Rock</span>
                <span role="columnheader" title="Spawn weight against the heaviest row of the same mining kind">Spawn weight</span>
              </div>
              {#each m.belts as b, i (i)}
                <div role="row" class={cn(BELT_GRID, 'items-center py-0.5')}>
                  <span role="cell" class="min-w-0 truncate" title={b.location}>{beltPlace(b)}</span>
                  <span role="cell" class="text-muted-foreground">{b.kind ?? '–'}</span>
                  <span role="cell" class={rarityClass(b.tier)}>{b.tier ?? '–'}</span>
                  <span role="cell" class="mono text-muted-foreground min-w-0 truncate text-[11px]" title={b.rock}>{b.rock}</span>
                  <span role="cell" class="flex items-center gap-1" title={b.spawn_share === null ? 'No weight' : String(b.spawn_share)}>
                    <span class="bg-muted h-1.5 flex-1 overflow-hidden rounded-full" aria-hidden="true">
                      <span class="bg-primary/70 block h-full rounded-full" style:width="{Math.round((bars[i] ?? 0) * 100)}%"></span>
                    </span>
                  </span>
                </div>
              {/each}
            </div>
          {/if}
          {#if m.spots.length}
            <div role="table" aria-label="Spots that place {m.name}">
              <div role="row" class={cn(SPOT_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
                <span role="columnheader">Spot</span>
                <span role="columnheader">Spawn point</span>
                <span role="columnheader" class="text-right" title="Share of the spawn point's slots that place this rock">Slot share</span>
              </div>
              {#each m.spots as s, i (i)}
                <div role="row" class={cn(SPOT_GRID, 'items-baseline py-0.5')}>
                  <span role="cell" class="min-w-0 truncate" title={s.area}>
                    {spotPlace(s)}{#if s.place}<span class="text-muted-foreground"> · {s.area}</span>{/if}
                  </span>
                  <span role="cell" class="text-muted-foreground min-w-0 truncate" title={s.configs.join(', ')}>{s.configs.join(', ')}</span>
                  <span role="cell" class="text-right tabular-nums">{pctText(s.slot_share)}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
      {#each Array.isArray(sources.quality) ? sources.quality : [] as q (q.resource)}
        {@const top = Math.max(0, ...q.steps.map((st) => st.share ?? 0))}
        <div class="flex flex-col gap-1">
          <p class="m-0 font-medium">
            Quality: {q.name}
            {#if q.tier}<span class="text-muted-foreground ml-1 text-[11px] font-normal">{q.tier}</span>{/if}
          </p>
          <p class="m-0 text-[11.5px]">
            Rolls {qualityText(q.range)}
            {#each q.overrides as o (o.location)}<span class="text-muted-foreground"> · {o.location}: {qualityText(o.range)}</span>{/each}
          </p>
          {#if q.steps.length}
            <div role="table" aria-label="Quality values the game shows for {q.name}">
              <div role="row" class="text-muted-foreground grid grid-cols-[4rem_6rem_minmax(4rem,1fr)_3.5rem] gap-x-2 border-b py-1 text-[11px]">
                <span role="columnheader">Shows</span>
                <span role="columnheader">Rolled</span>
                <span role="columnheader" title="Estimated from the curve, assuming a normal curve cut to its range">Share (est.)</span>
                <span role="columnheader"></span>
              </div>
              {#each q.steps as st (st.start)}
                <div role="row" class="grid grid-cols-[4rem_6rem_minmax(4rem,1fr)_3.5rem] items-center gap-x-2 py-0.5">
                  <span role="cell" class="font-medium tabular-nums">{st.shown}</span>
                  <span role="cell" class="text-muted-foreground tabular-nums">{st.start}–{st.end}</span>
                  <span role="cell" class="bg-muted h-1.5 overflow-hidden rounded-full" aria-hidden="true">
                    <span class="bg-primary/70 block h-full rounded-full" style:width="{top > 0 ? Math.round(((st.share ?? 0) / top) * 100) : 0}%"></span>
                  </span>
                  <span role="cell" class="text-right tabular-nums">{pctText(st.share)}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
      {#if sources.loot}
        {@const l = sources.loot}
        <div class="flex flex-col gap-1">
          <p class="m-0 font-medium">
            Loot
            {#if l.rarity}<span class={cn('ml-1 text-[11.5px] font-normal', rarityClass(l.rarity))}>{l.rarity}</span>{/if}
            {#if l.can_loot === false}<span class="text-muted-foreground ml-1 text-[11.5px] font-normal">· barred from loot{l.reason ? `: ${l.reason}` : ''}</span>{/if}
          </p>
          {#if l.containers.length}
            <div role="table" aria-label="Loot containers that can hold {l.name}">
              <div role="row" class={cn(LOOT_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
                <span role="columnheader">Area</span>
                <span role="columnheader">Container</span>
                <span role="columnheader">Type</span>
                <span role="columnheader">Size</span>
                <span role="columnheader">Rarity</span>
                <span role="columnheader" class="text-right" title="Rough odds one spawned container holds it (an even pick inside a loot pool is assumed)">Odds / box</span>
              </div>
              {#each l.containers as c, i (i)}
                <div role="row" class={cn(LOOT_GRID, 'items-baseline py-0.5')}>
                  <span role="cell" class="min-w-0 truncate" title={`${c.area}${c.spawn_point ? `\n${c.spawn_point}` : ''}`}>
                    {c.place ?? c.area}{#if c.place}<span class="text-muted-foreground"> · {c.area}</span>{/if}
                  </span>
                  <span role="cell" class="min-w-0 truncate" title={c.table ?? undefined}>
                    {c.container}{#if kindText(c)}<span class="text-muted-foreground"> · {kindText(c)}</span>{/if}
                  </span>
                  <span role="cell" class="text-muted-foreground min-w-0 truncate">{c.contents ?? '–'}</span>
                  <span role="cell">{c.size ?? '–'}</span>
                  <span role="cell" class={rarityClass(c.rarity)}>{c.rarity ?? '–'}</span>
                  <span role="cell" class="text-right font-medium tabular-nums">{pctText(c.odds_per_box)}</span>
                </div>
              {/each}
            </div>
            {#if l.container_total > l.containers.length}
              <p class="text-muted-foreground m-0 text-[11px]">{l.container_total - l.containers.length} more areas with lower odds not shown.</p>
            {/if}
          {:else}
            <p class="text-muted-foreground m-0 text-[11.5px]">No loot container spawns it.</p>
          {/if}
          {#if l.tables.length}
            <p class="text-muted-foreground m-0 mt-1 text-[11px]">
              Also rolled by {l.table_total} loot table{l.table_total === 1 ? '' : 's'} no container uses (enemies, bosses):
              {#each l.tables as t, i (t.table)}<span class="mono" title="{pctText(t.per_roll)} per roll">{t.table}</span>{i < l.tables.length - 1 ? ', ' : ''}{/each}{l.table_total > l.tables.length ? ', …' : ''}
            </p>
          {/if}
        </div>
      {/if}
    {/if}
  </CollapsibleCard>
{/if}
