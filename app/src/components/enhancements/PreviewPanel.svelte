<script lang="ts">
  /**
   * Live in-game preview of example texts, before and after (spec 9.4 Enhancements). Wide: a
   * sticky side column. Narrow (`strip`): a sticky panel at the top that can be folded, showing
   * the example for the open category first, so a switch change is visible without scrolling.
   */
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import InGamePreview from '@/components/common/InGamePreview.svelte';
  import { Skeleton } from '@/components/ui/skeleton';
  import type { PreviewItem } from '@/lib/contract/types';
  import { cn } from '@/lib/utils';
  import { previewOrder } from '@/lib/bpmarkers';

  let { items, loading = false, focus = null, strip = false }: { items: PreviewItem[] | null; loading?: boolean; focus?: string | null; strip?: boolean } = $props();
  const shown = $derived(previewOrder(items, focus));
  let open = $state(true);
  const uid = $props.id();
</script>

{#snippet sample(it: PreviewItem)}
  <article class="grid min-w-0 gap-1.5" aria-label={it.label}>
    <div class="flex items-baseline justify-between gap-2">
      <h3 class="shrink-0 font-semibold">{it.label}</h3>
      <span class="mono text-muted-foreground min-w-0 truncate text-[11px]" title={it.key}>{it.key}</span>
    </div>
    {#if it.original === it.result}
      <InGamePreview text={it.result} />
      <p class="text-muted-foreground text-[12px]">Unchanged with these settings.</p>
    {:else}
      <div class="text-muted-foreground text-[11px] font-medium uppercase">Before</div>
      <InGamePreview text={it.original} class="opacity-80" />
      <div class="text-primary text-[11px] font-medium uppercase">After</div>
      <InGamePreview text={it.result} />
    {/if}
  </article>
{/snippet}

<section aria-labelledby="{uid}-title" aria-busy={loading} class={cn('bg-card min-w-0 rounded-lg border', strip && 'shadow-md')}>
  <div class="flex items-center justify-between gap-2 border-b px-4 py-2.5">
    <h2 id="{uid}-title" class="eyebrow text-muted-foreground">In-game preview</h2>
    <span class="text-muted-foreground flex items-center gap-2 text-[12px]">
      <span aria-live="polite">{loading ? 'Updating…' : 'Examples'}</span>
      {#if strip}
        <button type="button" class="hover:text-foreground focus-visible:ring-ring/60 flex items-center gap-1 rounded px-1 outline-none focus-visible:ring-2" aria-expanded={open} aria-controls="{uid}-body" onclick={() => (open = !open)}>
          {open ? 'Hide' : 'Show'} <ChevronDown class={cn('size-3.5 transition-transform', open && 'rotate-180')} aria-hidden="true" />
        </button>
      {/if}
    </span>
  </div>
  <div id="{uid}-body" class={cn('grid gap-4 p-4', strip && 'max-h-64 overflow-y-auto', strip && !open && 'hidden')}>
    {#if !items}
      <Skeleton class="h-24 w-full" /><Skeleton class="h-12 w-full" />
    {:else if !shown.length}
      <p class="text-muted-foreground text-[12.5px]">
        No examples yet: this build's game data has not been read, or the cached copy is from an older StarOrg Codex
        (footer: <span class="mono">cache stale</span>). Apply reads it; the examples show after that.
      </p>
    {:else if strip}
      {#each shown.slice(0, 1) as it (it.key)}{@render sample(it)}{/each}
      {#if shown.length > 1}<p class="text-muted-foreground text-[12px]">{shown.length - 1} more example{shown.length === 2 ? '' : 's'} in a wider window.</p>{/if}
    {:else}
      {#each shown as it (it.key)}{@render sample(it)}{/each}
    {/if}
  </div>
</section>
