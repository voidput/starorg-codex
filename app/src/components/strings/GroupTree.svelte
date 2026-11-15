<script lang="ts">
  /**
   * The Strings group tree (addendum 8.5): `role="tree"` with one tab stop (roving tabindex).
   * Up/Down, Home/End, Right/Left (open/close, child/parent), typeahead; Enter or Space selects
   * only this group, Ctrl+Enter / Ctrl+click adds or removes it; Escape clears. Counts follow
   * the facet rule, so zero-count groups are dimmed rather than removed and the tree does not
   * jump while typing a search. The list scrolls on its own.
   */
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { tick } from 'svelte';
  import type { GroupNode } from '@/lib/contract/types';
  import { ALL, applySelection, compact, isSelected, itemName, keyAction, visibleItems, type TreeAction, type TreeItem } from '@/lib/grouptree';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';

  let {
    nodes,
    total,
    selected,
    onselect,
    open = $bindable([]),
    label = 'Groups',
    allLabel = 'All groups',
    class: className = '',
  }: {
    nodes: GroupNode[];
    /** Rows matching the search and source filters (the "All groups" count). */
    total: number;
    selected: string[];
    onselect: (ids: string[]) => void;
    open?: string[];
    label?: string;
    /** Name of the first item, which clears the selection. */
    allLabel?: string;
    class?: string;
  } = $props();

  let focused = $state<string>(ALL);
  let list = $state<HTMLElement | null>(null);
  let typed = '';
  let typedAt = 0;

  const expanded = $derived(new Set(open));
  const items = $derived(visibleItems(nodes, expanded, total, allLabel));
  // Keep the tab stop on an item that exists (a collapse or a group-by switch can remove it).
  const tabId = $derived(items.some((x) => x.id === focused) ? focused : ALL);

  async function focusItem(id: string) {
    focused = id;
    await tick();
    const el = list?.querySelector<HTMLElement>(`[data-gid="${CSS.escape(id)}"]`);
    el?.focus();
    el?.scrollIntoView({ block: 'nearest' });
  }

  function run(a: TreeAction) {
    switch (a.kind) {
      case 'focus':
        void focusItem(a.id);
        break;
      case 'expand':
        open = [...open, a.id];
        break;
      case 'collapse':
        open = open.filter((x) => x !== a.id);
        break;
      case 'select-only':
      case 'toggle':
      case 'clear':
        onselect(applySelection(selected, a));
        break;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.altKey || e.metaKey) return;
    let buf = '';
    if (e.key.length === 1 && e.key !== ' ' && !e.ctrlKey) {
      const now = Date.now();
      typed = now - typedAt > 700 ? e.key : typed + e.key;
      typedAt = now;
      buf = typed;
    }
    const a = keyAction(items, tabId, e.key, e.ctrlKey, buf);
    if (a.kind === 'none') return;
    // Escape with nothing selected is left to the container (closes the narrow popover).
    if (e.key === 'Escape' && selected.length === 0) return;
    e.preventDefault();
    run(a);
  }

  function onClick(e: MouseEvent, x: TreeItem) {
    focused = x.id;
    if (x.id === ALL) run({ kind: 'clear' });
    else run(e.ctrlKey || e.metaKey ? { kind: 'toggle', id: x.id } : { kind: 'select-only', id: x.id });
  }

  function toggleOpen(e: MouseEvent, x: TreeItem) {
    e.stopPropagation();
    run(x.expanded ? { kind: 'collapse', id: x.id } : { kind: 'expand', id: x.id });
  }
</script>

<ul bind:this={list} role="tree" aria-label={label} aria-multiselectable="true" class={cn('min-h-0 overflow-y-auto py-1 text-[13px]', className)} onkeydown={onKey}>
  {#each items as x (x.id)}
    {@const sel = isSelected(x, selected)}
    <!-- Keys are handled once on the tree (onKey). -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <li
      role="treeitem"
      data-gid={x.id}
      aria-level={x.level}
      aria-expanded={x.hasChildren ? x.expanded : undefined}
      aria-selected={sel}
      aria-label={itemName(x)}
      tabindex={tabId === x.id ? 0 : -1}
      class={cn(
        'focus-visible:ring-ring/60 group relative flex h-7 cursor-pointer items-center gap-1 rounded-md pr-2 outline-none select-none focus-visible:ring-2 focus-visible:ring-inset',
        x.level === 2 ? 'pl-7' : 'pl-1',
        sel ? 'bg-primary/12 text-foreground' : 'hover:bg-accent/60',
        x.count === 0 && x.id !== ALL && 'opacity-45',
      )}
      onclick={(e) => onClick(e, x)}
      onfocus={() => (focused = x.id)}
    >
      {#if x.level === 1}
        {#if x.hasChildren}
          <button
            type="button"
            tabindex="-1"
            aria-hidden="true"
            class="text-muted-foreground hover:text-foreground grid size-5 shrink-0 place-items-center rounded"
            onclick={(e) => toggleOpen(e, x)}
          >
            <ChevronRight class={cn('size-3.5 transition-transform motion-reduce:transition-none', x.expanded && 'rotate-90')} />
          </button>
        {:else}
          <span class="size-5 shrink-0" aria-hidden="true"></span>
        {/if}
      {/if}
      {#if sel && x.id !== ALL}<span class="bg-primary absolute top-1 bottom-1 left-0 w-0.5 rounded-full" aria-hidden="true"></span>{/if}
      <span class={cn('min-w-0 flex-1 truncate', x.level === 1 && 'font-medium', x.id === ALL && 'text-muted-foreground')}>{x.label}</span>
      {#if x.edited}<span class="text-warning mono shrink-0 text-[10.5px]" title="{int(x.edited)} edited" aria-hidden="true">✎{compact(x.edited)}</span>{/if}
      {#if x.enhanced}<span class="text-primary mono shrink-0 text-[10.5px]" title="{int(x.enhanced)} enhanced" aria-hidden="true">+{compact(x.enhanced)}</span>{/if}
      <span class="text-muted-foreground mono w-12 shrink-0 text-right text-[11px] tabular-nums" aria-hidden="true">{int(x.count)}</span>
    </li>
  {/each}
</ul>
