<script lang="ts">
  /**
   * A foldable card at the top of the Explorer's Data tab (Combat, Sources): a title row that
   * opens and closes the body, remembered per viewer under `storageKey`. `open` is bindable so
   * a card can load only while open; `aside` sits at the right of the title row.
   */
  import type { Snippet } from 'svelte';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { loadOpen, saveOpen } from '@/lib/cardopen';
  import { storage } from '@/lib/finder';
  import { cn } from '@/lib/utils';

  let {
    id,
    title,
    subtitle,
    storageKey,
    defaultOpen,
    open = $bindable(),
    aside,
    children,
    class: className = '',
    bodyClass = '',
  }: {
    /** Prefix of the heading and body element ids. */
    id: string;
    title: string;
    subtitle: string;
    storageKey: string;
    defaultOpen: boolean;
    open?: boolean;
    aside?: Snippet<[boolean]>;
    children: Snippet;
    class?: string;
    bodyClass?: string;
  } = $props();

  const store = storage();
  // Read once; later changes come from the toggle.
  // svelte-ignore state_referenced_locally
  open = loadOpen(store, storageKey, defaultOpen);

  function toggle() {
    open = !open;
    saveOpen(store, storageKey, open);
  }
</script>

<section class={cn('bg-card shrink-0 overflow-auto border-b', className)} aria-labelledby="{id}-h">
  <h3 id="{id}-h" class="m-0">
    <button
      type="button"
      aria-expanded={open}
      aria-controls="{id}-body"
      class="hover:bg-accent/50 focus-visible:ring-ring/60 flex w-full min-w-0 items-center gap-2 px-2 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset"
      onclick={toggle}
    >
      <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', open && 'rotate-90')} aria-hidden="true" />
      <span class="text-[12.5px] font-semibold">{title}</span>
      <span class="text-muted-foreground min-w-0 truncate text-[11px]">{subtitle}</span>
      {@render aside?.(!!open)}
    </button>
  </h3>
  {#if open}
    <div id="{id}-body" class={bodyClass}>
      {@render children()}
    </div>
  {/if}
</section>
