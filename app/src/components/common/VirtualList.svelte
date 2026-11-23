<script lang="ts" generics="T">
  /**
   * A windowed list of fixed-height rows: only the rows in view (plus a few either side) are
   * in the DOM, so thousands of rows scroll smoothly. `resetKey` (a primitive) scrolls back to
   * the top when it changes (a new filter or view). The rows are absolutely placed in a
   * spacer as tall as the whole list.
   */
  import type { Snippet } from 'svelte';
  import { windowRange } from '@/lib/virtual';
  import { cn } from '@/lib/utils';

  let {
    items,
    rowHeight,
    key,
    row,
    resetKey = '',
    label,
    role = 'list',
    class: className = '',
  }: {
    items: T[];
    rowHeight: number;
    key: (item: T, index: number) => string | number;
    row: Snippet<[T, number]>;
    resetKey?: string | number;
    label: string;
    role?: 'list' | 'rowgroup';
    class?: string;
  } = $props();

  let scroller = $state<HTMLElement | null>(null);
  let top = $state(0);
  let viewH = $state(480);
  const range = $derived(windowRange(top, viewH, rowHeight, items.length));
  const shown = $derived(items.slice(range.first, range.end));

  // Keyed on the primitive reset key only.
  $effect(() => {
    void resetKey;
    if (scroller) scroller.scrollTop = 0;
    top = 0;
  });
</script>

<div bind:this={scroller} bind:clientHeight={viewH} class={cn('relative min-h-0 overflow-y-auto', className)} onscroll={() => (top = scroller?.scrollTop ?? 0)}>
  <div {role} aria-label={label} class="relative" style="height: {items.length * rowHeight}px">
    {#each shown as it, k (key(it, range.first + k))}
      <div role={role === 'list' ? 'listitem' : undefined} class="absolute right-0 left-0" style="top: {(range.first + k) * rowHeight}px; height: {rowHeight}px">
        {@render row(it, range.first + k)}
      </div>
    {/each}
  </div>
</div>
