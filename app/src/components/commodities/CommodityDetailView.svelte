<script lang="ts">
  /**
   * One resource in full (`commodity_detail`): forms, quality, volatility and cargo damage,
   * cargo items, where it is mined, and UEX kiosk prices. A section the data does not cover
   * says so in one quiet line; nothing here is an error.
   */
  import Download from '@lucide/svelte/icons/download';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Settings from '@lucide/svelte/icons/settings';
  import { Button } from '@/components/ui/button';
  import type { CommodityDetail, KioskPrice } from '@/lib/contract/types';
  import { beltBars, beltPlace, fetchedText, pctText, qualityText, scuPriceText, spotPlace } from '@/lib/sources';
  import { explosionText, numText, scuText } from '@/lib/commodities';
  import { cn } from '@/lib/utils';

  interface Props {
    c: CommodityDetail;
    uexOn: boolean;
    fetching: boolean;
    onpick: (id: string) => void;
    onfetch: () => void;
    onnetwork: () => void;
  }
  let { c, uexOn, fetching, onpick, onfetch, onnetwork }: Props = $props();

  const KIOSK_GRID = 'grid grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_7.5rem_5.5rem] gap-x-2';
  const BELT_GRID = 'grid grid-cols-[minmax(0,1.3fr)_3.5rem_4rem_minmax(0,1fr)_minmax(4rem,6rem)] gap-x-2';
  const SPOT_GRID = 'grid grid-cols-[minmax(0,1.3fr)_minmax(0,1.2fr)_4.5rem] gap-x-2';
  const STEP_GRID = 'grid grid-cols-[4rem_6rem_minmax(4rem,1fr)_3.5rem] gap-x-2';

  const mined = $derived(Array.isArray(c.mined) ? c.mined : null);
  const when = $derived(fetchedText(c.market.fetched));
  const topShare = $derived(Math.max(0, ...(c.quality?.steps ?? []).map((s) => s.share ?? 0)));
</script>

{#snippet quiet(text: string)}
  <p class="text-muted-foreground m-0 text-[12px]">{text}</p>
{/snippet}

{#snippet ref(id: string, name: string)}
  <button
    type="button"
    class="text-primary focus-visible:ring-ring/60 rounded-sm underline-offset-2 outline-none hover:underline focus-visible:ring-2"
    title="Show {name}"
    onclick={() => onpick(id)}>{name}</button
  >
{/snippet}

{#snippet kiosks(rows: KioskPrice[], total: number, label: string, priceHead: string, more: string)}
  {#if rows.length}
    <div role="table" aria-label={label} class="text-[12.5px]">
      <div role="row" class={cn(KIOSK_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
        <span role="columnheader">Kiosk</span>
        <span role="columnheader">Location</span>
        <span role="columnheader" class="text-right">{priceHead}</span>
        <span role="columnheader" class="text-right" title="Day UEX last confirmed the price">Updated</span>
      </div>
      {#each rows as o, i (i)}
        <div role="row" class={cn(KIOSK_GRID, 'items-baseline py-0.5')}>
          <span role="cell" class="min-w-0 truncate" title={o.terminal}>{o.terminal}</span>
          <span role="cell" class="text-muted-foreground min-w-0 truncate" title={o.location}>{o.location}</span>
          <span role="cell" class="text-right font-medium tabular-nums" title={o.scu_stock !== null ? `Holds ${o.scu_stock} SCU` : undefined}>{scuPriceText(o.price)}</span>
          <span role="cell" class={cn('text-right tabular-nums', o.stale ? 'text-muted-foreground/70' : 'text-muted-foreground')} title={o.stale ? 'Not confirmed for over 30 days' : undefined}
            >{o.updated ?? '–'}{o.stale ? ' ⚠' : ''}</span
          >
        </div>
      {/each}
    </div>
    {#if total > rows.length}
      <p class="text-muted-foreground m-0 text-[11px]">{total - rows.length} more {more} not shown.</p>
    {/if}
  {:else}
    {@render quiet('Not known: no kiosk in the UEX data.')}
  {/if}
{/snippet}

<article class="grid gap-4" aria-labelledby="commodity-title">
  <header class="grid gap-1">
    <h2 id="commodity-title" class="text-[16px] font-semibold">{c.name}</h2>
    <p class="text-muted-foreground text-[12px]">
      {c.group}{c.subgroup ? ` > ${c.subgroup}` : ''} · <span class="mono select-all">{c.id}</span>{c.density !== null ? ` · ${numText(c.density, 2)} g/cm³` : ''}
    </p>
    {#if c.description}<p class="text-[12.5px] leading-relaxed">{c.description}</p>{/if}
  </header>

  <section class="grid gap-1" aria-labelledby="c-forms">
    <h3 id="c-forms" class="eyebrow text-muted-foreground">Forms</h3>
    {#if c.refined || c.raw_sources.length}
      {#if c.refined}<p class="m-0 text-[12.5px]">Refines into {@render ref(c.refined.id, c.refined.name)}</p>{/if}
      {#if c.raw_sources.length}
        <p class="m-0 text-[12.5px]">
          Refined from {#each c.raw_sources as r, i (r.id)}{@render ref(r.id, r.name)}{i < c.raw_sources.length - 1 ? ', ' : ''}{/each}
        </p>
      {/if}
    {:else}
      {@render quiet('Not known: no refined or raw form in the game data.')}
    {/if}
  </section>

  <section class="grid gap-1" aria-labelledby="c-quality">
    <h3 id="c-quality" class="eyebrow text-muted-foreground">Quality</h3>
    {#if c.quality}
      {@const q = c.quality}
      <p class="m-0 text-[12.5px]">
        {#if q.tier}<span class="font-medium">{q.tier}</span> · {/if}Rolls {qualityText(q.range)}
        {#each q.overrides as o (o.location)}<span class="text-muted-foreground"> · {o.location}: {qualityText(o.range)}</span>{/each}
      </p>
      {#if q.steps.length}
        <div role="table" aria-label="Quality values the game shows for {c.name}" class="text-[12.5px]">
          <div role="row" class={cn(STEP_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
            <span role="columnheader">Shows</span>
            <span role="columnheader">Rolled</span>
            <span role="columnheader" title="Estimated from the curve, assuming a normal curve cut to its range">Share (est.)</span>
            <span role="columnheader"></span>
          </div>
          {#each q.steps as st (st.start)}
            <div role="row" class={cn(STEP_GRID, 'items-center py-0.5')}>
              <span role="cell" class="font-medium tabular-nums">{st.shown}</span>
              <span role="cell" class="text-muted-foreground tabular-nums">{st.start}–{st.end}</span>
              <span role="cell" class="bg-muted h-1.5 overflow-hidden rounded-full" aria-hidden="true">
                <span class="bg-primary/70 block h-full rounded-full" style:width="{topShare > 0 ? Math.round(((st.share ?? 0) / topShare) * 100) : 0}%"></span>
              </span>
              <span role="cell" class="text-right tabular-nums">{pctText(st.share)}</span>
            </div>
          {/each}
        </div>
      {/if}
    {:else}
      {@render quiet('Not known: no quality curve in the game data.')}
    {/if}
  </section>

  <section class="grid gap-1" aria-labelledby="c-volatile">
    <h3 id="c-volatile" class="eyebrow text-muted-foreground">Volatility and damage</h3>
    {#if c.volatility}
      <p class="m-0 text-[12.5px]">
        <span class={cn((c.volatility.volatility ?? 0) > 0 && 'text-warning font-medium')}>Volatile cargo</span>
        {#if c.volatility.volatility !== null}· factor {numText(c.volatility.volatility)}{/if}
        {#if c.volatility.decay_per_second !== null}· loses {numText(c.volatility.decay_per_second)} health per second{/if}
      </p>
      {#each c.damage as d (d.configs.join())}
        {@const boom = explosionText(d)}
        <p class="text-muted-foreground m-0 text-[12px]" title={d.configs.join(', ')}>
          {#if d.minimum_speed !== null}Impacts above {numText(d.minimum_speed, 1)} m/s damage it{d.grace_period !== null ? ` (after ${numText(d.grace_period, 1)} s)` : ''}.{/if}
          {#if boom}Explodes: {boom}.{/if}
          <span class="text-[11px]">({d.configs.length} config{d.configs.length === 1 ? '' : 's'})</span>
        </p>
      {/each}
    {:else}
      {@render quiet('Not known: no volatility in the game data.')}
    {/if}
  </section>

  <section class="grid gap-1" aria-labelledby="c-cargo">
    <h3 id="c-cargo" class="eyebrow text-muted-foreground">Cargo items</h3>
    {#if c.cargo.length}
      <ul class="m-0 grid list-none gap-0.5 p-0 text-[12.5px]">
        {#each c.cargo as i (i.class_name)}
          <li class="flex flex-wrap items-baseline gap-x-2">
            <span>{i.name}</span>
            <span class="mono text-muted-foreground text-[11px]">{i.class_name}</span>
            <span class="text-muted-foreground ml-auto tabular-nums">{i.kind ? `${i.kind} · ` : ''}{scuText(i.capacity_scu)}</span>
          </li>
        {/each}
      </ul>
    {:else}
      {@render quiet('Not known: no cargo item holds it.')}
    {/if}
  </section>

  <section class="grid gap-2" aria-labelledby="c-mined">
    <h3 id="c-mined" class="eyebrow text-muted-foreground">Where it is mined</h3>
    {#if mined === null}
      {#if c.raw_sources.length}
        <p class="text-muted-foreground m-0 text-[12px]">
          A refined form: see where {#each c.raw_sources as r, i (r.id)}{@render ref(r.id, r.name)}{i < c.raw_sources.length - 1 ? ' or ' : ''}{/each} is mined.
        </p>
      {:else}
        {@render quiet('Not known: the mining data of this build is not read.')}
      {/if}
    {:else if !mined.length}
      {@render quiet('Not known: no body, belt or spot in the mining data places it.')}
    {:else}
      {#each mined as m (m.ore)}
        {@const bars = beltBars(m.belts)}
        <div class="grid gap-1 text-[12.5px]">
          <p class="m-0 font-medium">{m.name} <span class="text-muted-foreground mono text-[11px] font-normal">{m.ore}</span></p>
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
                  <span role="cell">{b.tier ?? '–'}</span>
                  <span role="cell" class="mono text-muted-foreground min-w-0 truncate text-[11px]" title={b.rock}>{b.rock}</span>
                  <span role="cell" class="flex items-center gap-1" title={b.spawn_share === null ? 'No weight' : String(b.spawn_share)}>
                    <span class="bg-muted h-1.5 flex-1 overflow-hidden rounded-full" aria-hidden="true">
                      <span class="bg-primary/70 block h-full rounded-full" style:width="{Math.round((bars[i] ?? 0) * 100)}%"></span>
                    </span>
                  </span>
                </div>
              {/each}
            </div>
          {:else if m.spots.length}
            {@render quiet('No body or belt spawns it: only the spots below place it.')}
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
                  <span role="cell" class="min-w-0 truncate" title={s.area}>{spotPlace(s)}{#if s.place}<span class="text-muted-foreground"> · {s.area}</span>{/if}</span>
                  <span role="cell" class="text-muted-foreground min-w-0 truncate" title={s.configs.join(', ')}>{s.configs.join(', ')}</span>
                  <span role="cell" class="text-right tabular-nums">{pctText(s.slot_share)}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </section>

  <section class="grid gap-2" aria-labelledby="c-market">
    <h3 id="c-market" class="eyebrow text-muted-foreground">
      Buy and sell <span class="font-normal normal-case">· UEX, player-reported{when ? `, ${when}` : ''}</span>
    </h3>
    {#if !c.market.loaded}
      <div class="text-muted-foreground flex flex-wrap items-center gap-2 text-[12px]">
        <span class="min-w-0 flex-1">
          Not known: kiosk prices are in no game file. {uexOn ? 'Fetch the UEX market data to see them.' : 'Turn on "Download UEX market data" in Settings -> Network to see them.'}
        </span>
        {#if uexOn}
          <Button size="sm" variant="outline" onclick={onfetch} disabled={fetching}>
            {#if fetching}<LoaderCircle class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Download aria-hidden="true" />{/if}
            Fetch now
          </Button>
        {:else}
          <Button size="sm" variant="outline" onclick={onnetwork}><Settings aria-hidden="true" /> Network settings</Button>
        {/if}
      </div>
    {:else}
      <div class="grid gap-1">
        <p class="m-0 text-[12.5px] font-medium">Sell <span class="text-muted-foreground text-[11px] font-normal">kiosks that pay for it, best first</span></p>
        {@render kiosks(c.market.sell, c.market.sell_total, `Kiosks that buy ${c.name}, best price first`, 'Pays', 'kiosks paying less')}
      </div>
      <div class="grid gap-1">
        <p class="m-0 text-[12.5px] font-medium">Buy <span class="text-muted-foreground text-[11px] font-normal">kiosks that sell it, cheapest first</span></p>
        {@render kiosks(c.market.buy, c.market.buy_total, `Kiosks that sell ${c.name}, cheapest first`, 'Asks', 'dearer kiosks')}
      </div>
    {/if}
  </section>
</article>
