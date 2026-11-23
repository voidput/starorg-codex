<script lang="ts" generics="T">
  /**
   * A windowed list of fixed-height rows: only the rows in view (plus a few either side) are
   * in the DOM, so thousands of rows scroll smoothly. `resetKey` (a primitive) scrolls back to
   * the top when it changes (a new filter or view); `reveal` (a row index, with `revealKey` to
   * repeat it) scrolls that row into view, as the keyboard moves through a tree. The rows are
   * absolutely placed in a spacer as tall as the whole list. `role: 'tree'` leaves the rows'
   * roles to the snippet (each renders its own `treeitem`).
   */
  import type { Snippet } from 'svelte';
  import { revealTop, windowRange } from '@/lib/virtual';
  import { cn } from '@/lib/utils';

  let {
    items,
    rowHeight,
    key,
    row,
    resetKey = '',
    reveal = -1,
    revealKey = '',
    label,
    role = 'list',
    onkeydown,
    class: className = '',
  }: {
    items: T[];
    rowHeight: number;
    key: (item: T, index: number) => string | number;
    row: Snippet<[T, number]>;
    resetKey?: string | number;
    reveal?: number;
    revealKey?: string | number;
    label: string;
    role?: 'list' | 'rowgroup' | 'tree';
    onkeydown?: (e: KeyboardEvent) => void;
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

  // Keyed on primitives only: the row to show and its key. Each key scrolls once, as soon as
  // its row is listed; `top` is set at once so the row is drawn before the scroll event
  // arrives (the caller focuses it after a tick).
  let revealed: string | number | null = null;
  $effect(() => {
    const i = reveal;
    const k = revealKey;
    if (!scroller || i < 0 || k === revealed) return;
    revealed = k;
    const t = revealTop(i, rowHeight, scroller.scrollTop, scroller.clientHeight);
    if (t === null) return;
    scroller.scrollTop = t;
    top = t;
  });
</script>

<div bind:this={scroller} bind:clientHeight={viewH} class={cn('relative min-h-0 overflow-y-auto', className)} onscroll={() => (top = scroller?.scrollTop ?? 0)}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div {role} aria-label={label} class="relative" style="height: {items.length * rowHeight}px" {onkeydown}>
    {#each shown as it, k (key(it, range.first + k))}
      <div role={role === 'list' ? 'listitem' : undefined} class="absolute right-0 left-0" style="top: {(range.first + k) * rowHeight}px; height: {rowHeight}px">
        {@render row(it, range.first + k)}
      </div>
    {/each}
  </div>
</div>
