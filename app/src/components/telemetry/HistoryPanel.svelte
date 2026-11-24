<script lang="ts">
  /**
   * Live telemetry -> History: the last minutes of speed, G, fuel, hull, shields, health and
   * oxygen as small line graphs (sc-telemetry's sparklines, same owner, MIT), from the host's
   * 10-minute ring (one sample a second). Plain SVG paths, no chart library.
   *
   * The store polls the ring once a second while this panel is mounted and not folded away
   * (its CollapsibleCard), so the graphs redraw at 1 Hz whatever the frame rate, and a folded
   * card asks nothing. A gap (no sample for 3 s, or a value the game did not
   * send) breaks the line rather than bridging it.
   */
  import * as Card from '@/components/ui/card';
  import { getCardCollapse } from '@/components/ui/card/collapse';
  import Segmented from '@/components/common/Segmented.svelte';
  import { SERIES, WINDOWS, latestValue, seriesRange, sparkPath, windowStats } from '@/lib/telemetry';
  import { telemetry } from '@/lib/stores/telemetry.svelte';
  import { cn } from '@/lib/utils';

  const card = getCardCollapse();
  const open = $derived(card?.open ?? true);
  // A primitive: folding stops the poll, unfolding starts it again.
  $effect(() => {
    if (!open) return;
    return telemetry.watchHistory();
  });

  type WindowId = (typeof WINDOWS)[number]['id'];
  let win = $state<WindowId>('300');
  const span = $derived(WINDOWS.find((w) => w.id === win)?.ms ?? 300_000);

  /** viewBox units; the SVG stretches to its box. */
  const W = 300;
  const H = 56;

  const h = $derived(telemetry.history);
  const graphs = $derived.by(() => {
    if (!h || h.t_ms.length === 0) return [];
    const end = h.t_ms.at(-1)!;
    const start = end - span;
    const gapMs = h.interval_ms * 3;
    return SERIES.map((spec) => {
      const v = h.series[spec.key];
      const visible = v.filter((_, i) => h.t_ms[i]! >= start);
      const r = seriesRange(visible, spec);
      const last = latestValue(v);
      const stats = windowStats(h.t_ms, v, start);
      return {
        spec,
        path: sparkPath(h.t_ms, v, { w: W, h: H, end, span, min: r.min, max: r.max, gapMs }),
        last,
        stats,
        low: spec.low !== null && last !== null && last < spec.low,
        lowY: spec.low === null ? null : Math.round((H - ((spec.low - r.min) / (r.max - r.min)) * H) * 10) / 10,
      };
    });
  });
  const samples = $derived(h?.t_ms.length ?? 0);
</script>

<Card.Root class="lg:col-span-2" aria-labelledby="tele-history">
  <Card.Header class="flex flex-row flex-wrap items-center justify-between gap-2">
    <div class="grid gap-1">
      <Card.Title id="tele-history">History</Card.Title>
      <Card.Description>One sample a second, the last 10 minutes at most. Kept while the app runs, never saved (Record sessions saves a session).</Card.Description>
    </div>
    <Segmented ariaLabel="Graph window" size="sm" options={WINDOWS.map((w) => ({ id: w.id, label: w.label }))} bind:value={win} />
  </Card.Header>
  <Card.Content>
    {#if graphs.length === 0}
      <p class="text-muted-foreground py-4 text-center text-[12px]">The graphs fill in once frames arrive.</p>
    {:else}
      <ul class="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-4" aria-label="Graphs, {samples} samples">
        {#each graphs as g (g.spec.key)}
          <li class="bg-card grid gap-1 rounded-lg border px-3 py-2">
            <div class="flex items-baseline justify-between gap-2 text-[12px]">
              <span class="text-muted-foreground">{g.spec.label}</span>
              <span class={cn('mono font-semibold', g.low && 'text-destructive')}>{g.spec.text(g.last)}</span>
            </div>
            <svg
              class="h-12 w-full overflow-visible"
              viewBox="0 0 {W} {H}"
              preserveAspectRatio="none"
              role="img"
              aria-label={g.stats ? `${g.spec.label}: now ${g.spec.text(g.last)}, from ${g.spec.text(g.stats.min)} to ${g.spec.text(g.stats.max)} in this window` : `${g.spec.label}: no data in this window`}
            >
              <rect x="0" y="0" width={W} height={H} class="fill-primary/5" />
              {#if g.lowY !== null && g.lowY > 0 && g.lowY < H}
                <line x1="0" x2={W} y1={g.lowY} y2={g.lowY} class="stroke-destructive/40" stroke-dasharray="4 4" vector-effect="non-scaling-stroke" />
              {/if}
              {#if g.path}
                <path d={g.path} fill="none" class={g.low ? 'stroke-destructive' : 'stroke-primary'} stroke-width="1.5" stroke-linejoin="round" vector-effect="non-scaling-stroke" />
              {/if}
            </svg>
            <div class="text-muted-foreground mono flex justify-between text-[11px]">
              {#if g.stats}<span>min {g.spec.text(g.stats.min)}</span><span>max {g.spec.text(g.stats.max)}</span>{:else}<span>Not sent</span>{/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </Card.Content>
</Card.Root>
