<script lang="ts">
  /** Live in-game preview of sample keys, before and after (spec 9.4 Enhancements). */
  import InGamePreview from '@/components/common/InGamePreview.svelte';
  import { Skeleton } from '@/components/ui/skeleton';
  import type { PreviewItem } from '@/lib/contract/types';

  let { items, loading = false, focus = null }: { items: PreviewItem[] | null; loading?: boolean; focus?: string | null } = $props();
  const shown = $derived(Array.isArray(items) ? [...items].sort((a, b) => Number(b.category === focus) - Number(a.category === focus)) : []);
</script>

<section aria-labelledby="preview-title" aria-busy={loading} class="bg-card rounded-lg border">
  <div class="flex items-center justify-between border-b px-4 py-2.5">
    <h2 id="preview-title" class="eyebrow text-muted-foreground">In-game preview</h2>
    <span class="text-muted-foreground text-[11.5px]" aria-live="polite">{loading ? 'Updating…' : 'Sample keys'}</span>
  </div>
  <div class="grid gap-4 p-4">
    {#if !items}
      <Skeleton class="h-24 w-full" /><Skeleton class="h-12 w-full" />
    {:else}
      {#each shown as it (it.key)}
        <article class="grid gap-1.5" aria-label={it.label}>
          <div class="flex items-baseline justify-between gap-2">
            <h3 class="font-semibold">{it.label}</h3>
            <span class="mono text-muted-foreground truncate text-[11px]">{it.key}</span>
          </div>
          {#if it.original === it.result}
            <InGamePreview text={it.result} />
            <p class="text-muted-foreground text-[11.5px]">Unchanged with these settings.</p>
          {:else}
            <div class="text-muted-foreground text-[11px] font-medium uppercase">Before</div>
            <InGamePreview text={it.original} class="opacity-80" />
            <div class="text-primary text-[11px] font-medium uppercase">After</div>
            <InGamePreview text={it.result} />
          {/if}
        </article>
      {/each}
    {/if}
  </div>
</section>
