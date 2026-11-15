<script lang="ts">
  /**
   * Datasets and Diff group tree (addendum 7.13), with the look and keys of the Strings group
   * tree: `role="tree"`, one tab stop (roving tabindex); Up/Down, Home/End, Right/Left (open,
   * close, child, parent), typeahead; Enter or Space selects the node, Escape or "All"
   * clears. Zero-count nodes are dimmed, not removed. Opening a node asks the backend for its
   * children (`open` is bound). The list scrolls on its own.
   */
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { tick } from 'svelte';
  import type { DataGroup } from '@/lib/contract/dev';
  import { compact } from '@/lib/grouptree';
  import { ALL, collapse, dataItemName, dataItems, dataKeyAction, type DataTreeAction, type DataTreeItem } from '@/lib/datatree';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';

  let {
    nodes,
    total,
    selected,
    onselect,
    open = $bindable([]),
    label = 'Groups',
    allLabel = 'All rows',
    unit = 'rows',
    loading = false,
    class: className = '',
  }: {
    nodes: DataGroup[];
    total: number;
    /** The selected node id, null = all. */
    selected: string | null;
    onselect: (id: string | null) => void;
    open?: string[];
    label?: string;
    allLabel?: string;
    /** What counts count, for the accessible names. */
    unit?: string;
    loading?: boolean;
    class?: string;
  } = $props();

  let focused = $state<string>(ALL);
  let list = $state<HTMLElement | null>(null);
  let typed = '';
  let typedAt = 0;

  const expanded = $derived(new Set(open));
  const items = $derived(dataItems(nodes, expanded, total, allLabel));
  const tabId = $derived(items.some((x) => x.id === focused) ? focused : ALL);

  async function focusItem(id: string) {
    focused = id;
    await tick();
    const el = list?.querySelector<HTMLElement>(`[data-nid="${CSS.escape(id)}"]`);
    el?.focus({ preventScroll: true });
    el?.scrollIntoView({ block: 'nearest' });
  }

  function run(a: DataTreeAction) {
    switch (a.kind) {
      case 'focus':
        void focusItem(a.id);
        break;
      case 'expand':
        if (!open.includes(a.id)) open = [...open, a.id];
        break;
      case 'collapse':
        open = collapse(open, a.id);
        break;
      case 'select':
        onselect(a.id);
        break;
      case 'clear':
        onselect(null);
        break;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.altKey || e.metaKey || e.ctrlKey) return;
    let buf = '';
    if (e.key.length === 1 && e.key !== ' ') {
      const now = Date.now();
      typed = now - typedAt > 700 ? e.key : typed + e.key;
      typedAt = now;
      buf = typed;
    }
    const a = dataKeyAction(items, tabId, e.key, buf);
    if (a.kind === 'none') return;
    if (e.key === 'Escape' && selected === null) return;
    e.preventDefault();
    run(a);
  }

  function onClick(x: DataTreeItem) {
    focused = x.id;
    run(x.id === ALL ? { kind: 'clear' } : { kind: 'select', id: x.id });
  }

  function toggleOpen(e: MouseEvent, x: DataTreeItem) {
    e.stopPropagation();
    run(x.expanded ? { kind: 'collapse', id: x.id } : { kind: 'expand', id: x.id });
  }
</script>

<ul bind:this={list} role="tree" aria-label={label} aria-busy={loading} class={cn('min-h-0 overflow-y-auto py-1 text-[13px]', className)} onkeydown={onKey}>
  {#each items as x (x.id)}
    {@const sel = x.id === ALL ? selected === null : selected === x.id}
    <!-- Keys are handled once on the tree (onKey). -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <li
      role="treeitem"
      data-nid={x.id}
      aria-level={x.depth}
      aria-expanded={x.hasChildren ? x.expanded : undefined}
      aria-selected={sel}
      aria-label={dataItemName(x, unit)}
      tabindex={tabId === x.id ? 0 : -1}
      class={cn(
        'focus-visible:ring-ring/60 group relative flex h-7 cursor-pointer items-center gap-1 rounded-md pr-2 outline-none select-none focus-visible:ring-2 focus-visible:ring-inset',
        sel ? 'bg-primary/12 text-foreground' : 'hover:bg-accent/60',
        x.count === 0 && x.id !== ALL && 'opacity-45',
      )}
      style="padding-left: {4 + (x.depth - 1) * 14}px"
      onclick={() => onClick(x)}
      onfocus={() => (focused = x.id)}
    >
      {#if x.hasChildren}
        <button type="button" tabindex="-1" aria-hidden="true" class="text-muted-foreground hover:text-foreground grid size-5 shrink-0 place-items-center rounded" onclick={(e) => toggleOpen(e, x)}>
          <ChevronRight class={cn('size-3.5 transition-transform motion-reduce:transition-none', x.expanded && 'rotate-90')} />
        </button>
      {:else}
        <span class="size-5 shrink-0" aria-hidden="true"></span>
      {/if}
      {#if sel && x.id !== ALL}<span class="bg-primary absolute top-1 bottom-1 left-0 w-0.5 rounded-full" aria-hidden="true"></span>{/if}
      <span class={cn('min-w-0 flex-1 truncate', x.depth === 1 && x.id !== ALL && 'font-medium', x.id === ALL && 'text-muted-foreground')} title={x.label}>{x.label}</span>
      {#if x.diff}
        {#if x.diff.added}<span class="text-success mono shrink-0 text-[10.5px]" aria-hidden="true">+{compact(x.diff.added)}</span>{/if}
        {#if x.diff.removed}<span class="text-destructive mono shrink-0 text-[10.5px]" aria-hidden="true">−{compact(x.diff.removed)}</span>{/if}
        {#if x.diff.changed}<span class="text-warning mono shrink-0 text-[10.5px]" aria-hidden="true">~{compact(x.diff.changed)}</span>{/if}
      {:else}
        <span class="text-muted-foreground mono w-12 shrink-0 text-right text-[11px] tabular-nums" aria-hidden="true">{int(x.count)}</span>
      {/if}
    </li>
  {/each}
</ul>
{#if items.some((x) => x.expanded && x.more)}
  <p class="text-muted-foreground border-t px-3 py-1 text-[11px]">Large groups list their first 500 children; search to narrow.</p>
{/if}
