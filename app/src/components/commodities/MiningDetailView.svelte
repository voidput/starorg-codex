<script lang="ts">
  /**
   * One mineral in full (`mining_detail`): what it refines into, its quality curve and the
   * place overrides, the rock physics, and every body or belt that spawns it with that
   * place's quality first, then a short rock line and the difficulty estimate. A number the
   * data does not hold says "not known". No ship or laser pick: the head records hold only beam
   * power and range, not the modifiers that decide a head, so a pick would mislead. Places sort
   * most common or best quality first.
   */
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Segmented from '@/components/common/Segmented.svelte';
  import type { MineralDetail, MiningPlace } from '@/lib/contract/types';
  import { pctText, qualityText, spotPlace } from '@/lib/sources';
  import { numText } from '@/lib/commodities';
  import { curveSource, difficultyInputs, GRADE_LABEL, gradeTone, kindLabel, placeName, rangeText, scaleNote, shareText, sortPlaces, type PlaceSort } from '@/lib/mining';
  import { cn } from '@/lib/utils';

  interface Props {
    m: MineralDetail;
    physics: boolean;
    /** `game-mining-spawns` and `game-item-sources` were read. */
    spawnsRead: boolean;
    spotsRead: boolean;
    /** Show a resource on the Commodities tab. */
    oncommodity: (id: string) => void;
  }
  let { m, physics, spawnsRead, spotsRead, oncommodity }: Props = $props();

  let sort = $state<PlaceSort>('common');
  let open = $state<Record<string, boolean>>({});
  const SORTS: { id: PlaceSort; label: string; hint: string }[] = [
    { id: 'common', label: 'Most common', hint: 'By mining kind, then spawn weight' },
    { id: 'quality', label: 'Best quality', hint: 'Highest estimated chance of a roll at 700 or more first; rows with a quality scale below 1 or an unknown curve after the others' },
  ];
  const places = $derived(sortPlaces(m.places, sort));
  const key = (p: MiningPlace) => `${p.ore}|${p.location}|${p.rock}`;
  const PLACE_GRID = 'grid grid-cols-[1rem_minmax(0,1.4fr)_3rem_minmax(0,1.3fr)_4rem_minmax(0,1.3fr)_4.5rem] gap-x-2';
  const SPOT_GRID = 'grid grid-cols-[minmax(0,1.3fr)_minmax(0,1.2fr)_4.5rem] gap-x-2';
</script>

{#snippet quiet(text: string)}
  <p class="text-muted-foreground m-0 text-[12px]">{text}</p>
{/snippet}

{#snippet ref(id: string, name: string)}
  <button
    type="button"
    class="text-primary focus-visible:ring-ring/60 rounded-sm underline-offset-2 outline-none hover:underline focus-visible:ring-2"
    title="Show {name} on the Commodities tab"
    onclick={() => oncommodity(id)}>{name}</button
  >
{/snippet}

<article class="grid gap-4" aria-labelledby="mineral-title">
  <header class="grid gap-1">
    <h2 id="mineral-title" class="text-[16px] font-semibold">{m.name}</h2>
    <p class="text-muted-foreground text-[12px]">
      {m.kinds.map(kindLabel).join(', ') || 'No place in the mining data'}{m.tier ? ` · ${m.tier}` : ''} · <span class="mono select-all">{m.ores.join(', ') || m.id}</span>
    </p>
    {#if m.commodity}
      <p class="m-0 text-[12.5px]">Prices, cargo and volatility: {@render ref(m.commodity.id, `${m.commodity.name} on Commodities`)}</p>
    {/if}
  </header>

  <section class="grid gap-1" aria-labelledby="m-refined">
    <h3 id="m-refined" class="eyebrow text-muted-foreground">Raw to refined</h3>
    {#if m.refined}
      <p class="m-0 text-[12.5px]">Refines into {@render ref(m.refined.id, m.refined.name)}.</p>
      {@render quiet('Refining yield: not known. Yields, costs and times are worked out on the server, not in the game files.')}
    {:else}
      {@render quiet('Not known: the game data names no refined form (sold as it is).')}
    {/if}
  </section>

  <section class="grid gap-1" aria-labelledby="m-quality">
    <h3 id="m-quality" class="eyebrow text-muted-foreground">Quality</h3>
    {#if m.base}
      <p class="m-0 text-[12.5px]">
        Base curve: rolls {qualityText(m.base)}{m.base_high !== null ? ` · ${pctText(m.base_high)} of rolls at 700 or more (est.)` : ''}
      </p>
      {#if m.steps.length}
        <p class="text-muted-foreground m-0 text-[12px]">Shows as {m.steps.map((s) => `${s.shown}${s.share !== null ? ` (${pctText(s.share)})` : ''}`).join(' · ')}</p>
      {/if}
      {#each m.overrides as o, i (i)}
        <p class="m-0 text-[12.5px]">
          <span class="font-medium">{o.location}</span> override: rolls {qualityText(o.range)}
          <span class="text-muted-foreground"
            >· {o.places ? `applies to ${o.places} place${o.places === 1 ? '' : 's'} below` : 'no body or belt of the mining data is that place, so it is not applied to any row'}</span
          >
        </p>
      {/each}
      {#if !m.overrides.length}{@render quiet('No place override: every place rolls the base curve.')}{/if}
    {:else}
      {@render quiet('Not known: no quality curve in the game data.')}
    {/if}
  </section>

  <details class="grid gap-1">
    <summary class="eyebrow text-muted-foreground cursor-pointer">Rock physics</summary>
    {#if m.elements.length}
      {#each m.elements as e (e.ore)}
        <p class="m-0 text-[12.5px]">
          {#if m.elements.length > 1}<span class="mono text-muted-foreground text-[11px]">{e.ore}</span> ·{/if}
          resistance {numText(e.resistance, 3)} · instability {numText(e.instability, 2)} · optimal window at {e.window_midpoint !== null ? pctText(e.window_midpoint) : '–'}{e.window_randomness
            ? ` (± ${pctText(e.window_randomness)})`
            : ''}, thinness {numText(e.window_thinness, 3)}
          {#if e.explosion_multiplier !== null}· explosion ×{numText(e.explosion_multiplier, 2)}{/if}
        </p>
      {/each}
      {@render quiet('From the MineableElement records. Rock mass: not known (rolled when a rock spawns; the records hold a placeholder).')}
    {:else if !physics}
      {@render quiet('Not known yet: the mining physics of this build are not read. Read game data (above) adds them.')}
    {:else}
      {@render quiet('Not known: no MineableElement record for this ore.')}
    {/if}
  </details>

  <section class="grid gap-2" aria-labelledby="m-places">
    <div class="flex flex-wrap items-center gap-2">
      <h3 id="m-places" class="eyebrow text-muted-foreground">Where found</h3>
      {#if m.places.length > 1}
        <Segmented size="sm" ariaLabel="Sort places" class="ml-auto" options={SORTS} bind:value={sort} />
      {/if}
    </div>
    {#if !spawnsRead}
      {@render quiet('Not read yet: the mining spawns of this build are not read. Read game data adds them.')}
    {:else if !m.places.length}
      {@render quiet(m.spots.length ? 'No body or belt spawns it: only the spots below place it.' : spotsRead ? 'Not known: no body, belt or spot in the mining data places it.' : 'No body or belt spawns it; the hand-placed spots are not read yet.')}
    {:else}
      <div role="table" aria-label="Places that spawn {m.name}" class="text-[12.5px]">
        <div role="row" class={cn(PLACE_GRID, 'text-muted-foreground border-b py-1 text-[11px]')}>
          <span role="columnheader"><span class="sr-only">Details</span></span>
          <span role="columnheader">Body or belt</span>
          <span role="columnheader">Kind</span>
          <span role="columnheader" title="The place's quality curve: an override for the place, else the base curve">Quality</span>
          <span role="columnheader" class="text-right" title="Estimated share of rolls at 700 or more on this place's curve (a normal curve cut to its range; quality scales not applied)">≥ 700 (est.)</span>
          <span role="columnheader" title="The highest shown quality values on this place's curve and their estimated shares">Best rolls (est.)</span>
          <span role="columnheader" title="An estimate from resistance, instability and the charge window">Difficulty</span>
        </div>
        {#each places as p (key(p))}
          {@const k = key(p)}
          {@const on = !!open[k]}
          <div role="row" class={cn(PLACE_GRID, 'items-center py-0.5')}>
            <span role="cell">
              <button
                type="button"
                class="text-muted-foreground hover:text-foreground focus-visible:ring-ring/60 grid size-4 place-items-center rounded-sm outline-none focus-visible:ring-2"
                aria-expanded={on}
                aria-label="{on ? 'Hide' : 'Show'} the rock at {placeName(p)}"
                onclick={() => (open = { ...open, [k]: !on })}><ChevronRight class={cn('size-3.5 transition-transform motion-reduce:transition-none', on && 'rotate-90')} aria-hidden="true" /></button
              >
            </span>
            <span role="cell" class="min-w-0 truncate" title={p.location}>{placeName(p)}</span>
            <span role="cell" class="text-muted-foreground">{kindLabel(p.kind)}</span>
            <span role="cell" class="min-w-0 truncate" title={qualityText(p.quality.range)}>
              {rangeText(p.quality.range)}
              <span class={cn('text-[11px]', p.quality.source === 'override' ? 'text-primary' : p.quality.source === 'unknown' ? 'text-warning' : 'text-muted-foreground')}>· {p.quality.source === 'unknown' ? 'curve not known' : curveSource(p.quality)}</span>
            </span>
            <span role="cell" class={cn('text-right tabular-nums', !p.quality.full_scale && 'text-muted-foreground')} title={scaleNote(p) ?? undefined}>{pctText(p.quality.high)}{p.quality.full_scale ? '' : '*'}</span>
            <span role="cell" class="text-muted-foreground min-w-0 truncate tabular-nums">{p.quality.top.length ? p.quality.top.map((s) => `${s.shown}${s.share !== null ? ` ${pctText(s.share)}` : ''}`).join(' · ') : '–'}</span>
            <span role="cell" class={cn(gradeTone(p.difficulty?.grade))} title={p.difficulty ? difficultyInputs(p.difficulty) : undefined}>{p.difficulty ? GRADE_LABEL[p.difficulty.grade] : 'Not known'}</span>
          </div>
          {#if on}
            <div role="row" class="bg-muted/40 mb-1 rounded-md px-3 py-2 text-[12px]">
              <div role="cell" class="grid gap-1">
              <p class="m-0">
                <span class="font-medium">Quality here:</span> rolls {qualityText(p.quality.range)} ({p.quality.source === 'unknown' ? 'curve not known, base shares shown' : curveSource(p.quality)}){p.quality.high !== null ? ` · ${pctText(p.quality.high)} of rolls at 700 or more (est.)` : ''}
              </p>
              {#if p.quality.top.length}
                <p class="m-0">
                  Best rolls: {p.quality.top.map((s) => `${s.shown} → ${s.share !== null ? pctText(s.share) : 'not known'}`).join(' · ')}{p.quality.top_share !== null ? ` (together ${pctText(p.quality.top_share)}, est.)` : ''}
                </p>
              {/if}
              {#if scaleNote(p)}<p class="text-warning m-0">* Quality scale ×{p.quality_scales.join(', ×')} in this rock: the data does not say how the game applies it, so the real share may be lower.</p>{/if}
              <p class="text-muted-foreground m-0">
                Rock: {p.tier ?? 'tier not known'} · {kindLabel(p.kind)}-mined · {m.name} {shareText(p.min_percent, p.max_percent)}{p.companions.length ? ` · also ${p.companions.join(', ')}` : ''}{p.signature !== null ? ` · signature ${numText(p.signature, 0)}` : ''}
              </p>
              <p class="text-muted-foreground m-0">
                Difficulty (estimate): {#if p.difficulty}{GRADE_LABEL[p.difficulty.grade]} · {difficultyInputs(p.difficulty)}{:else}not known: {physics ? 'no physics record for this ore' : 'the mining physics are not read'}.{/if}
              </p>
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </section>

  {#if !spotsRead && spawnsRead}
    {@render quiet('Hand-placed spots: not read yet (the loot area data of this build). Read game data adds them.')}
  {/if}
  {#if m.spots.length}
    <section class="grid gap-1" aria-labelledby="m-spots">
      <h3 id="m-spots" class="eyebrow text-muted-foreground">Hand-placed spots</h3>
      <div role="table" aria-label="Spots that place {m.name}" class="text-[12.5px]">
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
      {@render quiet('Spots roll the base curve: the data does not say which system a spot is in, so no override is applied.')}
    </section>
  {/if}

</article>
