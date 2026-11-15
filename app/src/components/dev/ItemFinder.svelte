<script lang="ts">
  /**
   * The Explorer's item finder (addendum 7.17): Items and Assets modes. Left: the Sections tree
   * (sections > type / sub-type / size / manufacturer > records; Assets: Models and Sounds by
   * section) or the facet sidebar (every facet with counts that follow the other filters and
   * the search). Middle: quick filters, active chips, saved presets, sort and grouping, then
   * the rich list (icon, readable name, short id, maker, size, stats, model/sound/change
   * badges), virtualised, with collapsible group headers. All filtering runs in the worker.
   * Assets mode can also search sound events (ATL triggers) by name. Keys: the tree as in Raw
   * mode; the list Up/Down/PageUp/PageDown/Home/End, Enter opens (or folds a header).
   */
  import { tick, untrack } from 'svelte';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Box from '@lucide/svelte/icons/box';
  import Volume2 from '@lucide/svelte/icons/volume-2';
  import X from '@lucide/svelte/icons/x';
  import Bookmark from '@lucide/svelte/icons/bookmark';
  import ListFilter from '@lucide/svelte/icons/list-filter';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import Segmented from '@/components/common/Segmented.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import SectionIcon from '@/components/dev/SectionIcon.svelte';
  import { devApi, type DevCompare, type FacetValue, type ListItem, type ListPage, type TreeNode } from '@/lib/contract/dev';
  import { assetApi, type TriggerHit } from '@/lib/contract/assets';
  import { toContractError, type ContractError } from '@/lib/contract/client';
  import type { ChannelRef } from '@/lib/contract/types';
  import { guidOf, treeKey, treeRows, typeahead, type Loaded } from '@/lib/devnav';
  import * as F from '@/lib/finder';
  import { dev } from '@/lib/stores/dev.svelte';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';

  let {
    target,
    ready,
    view = $bindable(),
    selected = null,
    selectedTrigger = null,
    compared = null,
    narrow = false,
    query = $bindable(''),
    onopen,
    onopentrigger,
    onremember,
  }: {
    target: ChannelRef;
    ready: boolean;
    view: F.FinderView;
    selected?: string | null;
    selectedTrigger?: string | null;
    compared?: DevCompare | null;
    narrow?: boolean;
    /** The search box text (owned by the Explorer's top bar). */
    query?: string;
    onopen: (guid: string) => void;
    onopentrigger: (name: string) => void;
    /** A result was opened: the search that found it is worth remembering. */
    onremember?: (query: string) => void;
  } = $props();

  const PAGE = 500;
  const TROW = 28;
  const LROW = 52;
  const HROW = 30;
  const OVERSCAN = 8;
  const store = F.storage();
  const tkey = $derived(`${target.install}/${target.channel}`);
  const assets = $derived(view.mode === 'assets');

  // --- Sections tree -------------------------------------------------------------------
  let loaded = $state.raw(new Map<string, Loaded>());
  let open = $state.raw(new Set<string>());
  let treeActive = $state('');
  let treeError = $state<ContractError | null>(null);
  async function fetchChildren(node: string, offset: number) {
    try {
      const r = await devApi.tree({ target: $state.snapshot(target), root: 'section', node, offset, limit: PAGE, main_only: true, player_only: view.filters.player_only });
      // Assets mode shows the Models and Sounds views only.
      const nodes = node === '' && assets ? r.nodes.filter((n) => n.id.startsWith('m:')) : r.nodes;
      const prev = offset ? (loaded.get(node)?.nodes ?? []) : [];
      const n = new Map(loaded);
      n.set(node, { nodes: [...prev, ...nodes], total: node === '' && assets ? nodes.length : r.total });
      loaded = n;
      treeError = null;
    } catch (e) {
      treeError = toContractError(e);
    }
  }
  const playerOnly = $derived(view.filters.player_only);
  $effect(() => {
    void tkey;
    void playerOnly;
    void assets;
    const r = ready;
    untrack(() => {
      loaded = new Map();
      open = new Set();
      if (r) void fetchChildren('', 0).then(() => (treeActive = loaded.get('')?.nodes[0]?.id ?? ''));
    });
  });
  const rows = $derived(treeRows(loaded, open));
  async function toggleNode(id: string, v: boolean) {
    const n = new Set(open);
    if (v) {
      n.add(id);
      if (!loaded.has(id)) await fetchChildren(id, 0);
    } else n.delete(id);
    open = n;
  }
  let treeScroller = $state<HTMLElement | null>(null);
  let treeTop = $state(0);
  let treeH = $state(400);
  const tFirst = $derived(Math.max(0, Math.floor(treeTop / TROW) - OVERSCAN));
  const tLast = $derived(Math.min(rows.length - 1, Math.ceil((treeTop + treeH) / TROW) + OVERSCAN));
  const tIndex = $derived(Math.max(0, rows.findIndex((r) => r.id === treeActive)));
  function revealTree(i: number) {
    if (!treeScroller) return;
    const top = i * TROW;
    if (top < treeScroller.scrollTop) treeScroller.scrollTop = top;
    else if (top + TROW > treeScroller.scrollTop + treeScroller.clientHeight) treeScroller.scrollTop = top + TROW - treeScroller.clientHeight;
    treeTop = treeScroller.scrollTop;
  }
  /** Pick a tree node for the list; a new section brings back the filters used there. */
  function selectNode(n: TreeNode) {
    if (n.kind === 'record') {
      const g = guidOf(n.id);
      if (g) onopen(g);
      return;
    }
    const before = F.sectionOf(view.node);
    const after = F.sectionOf(n.id);
    const filters = after !== before ? (F.loadFilters(store, after) ?? { ...F.NO_FILTERS, player_only: view.filters.player_only }) : view.filters;
    view = { ...view, node: n.id, filters, collapsed: [] };
    listActive = 0;
  }
  let typed = '';
  let typedAt = 0;
  function onTreeKey(e: KeyboardEvent) {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key.length === 1 && e.key !== ' ' && e.key !== '*') {
      const now = Date.now();
      typed = now - typedAt < 700 ? typed + e.key : e.key;
      typedAt = now;
      const hit = typeahead(rows, treeActive, typed);
      if (hit) {
        treeActive = hit;
        void tick().then(() => revealTree(tIndex));
      }
      return;
    }
    const a = treeKey(rows, treeActive, e.key);
    if (!a) return;
    e.preventDefault();
    if (a.kind === 'focus') {
      treeActive = a.id;
      void tick().then(() => revealTree(tIndex));
    } else if (a.kind === 'open') void toggleNode(a.id, true);
    else if (a.kind === 'close') void toggleNode(a.id, false);
    else if (a.kind === 'more') void fetchChildren(a.parent, loaded.get(a.parent)?.nodes.length ?? 0);
    else if (a.kind === 'activate') {
      const n = rows.find((r) => r.id === a.id)?.node;
      if (n) selectNode(n);
    }
    if (e.key === 'Enter') {
      const n = rows.find((r) => r.id === treeActive)?.node;
      if (n && n.kind !== 'record') selectNode(n);
    }
  }

  // --- The list ------------------------------------------------------------------------
  let debounced = $state('');
  $effect(() => {
    const s = query;
    const t = setTimeout(() => (debounced = s.trim()), 200);
    return () => clearTimeout(t);
  });
  $effect(() => {
    const q = debounced;
    untrack(() => {
      if (q !== view.query) view = { ...view, query: q };
    });
  });
  // A restored view (Back/Forward) puts its search back in the box.
  $effect(() => {
    const q = view.query;
    untrack(() => {
      if (q !== debounced) {
        query = q;
        debounced = q;
      }
    });
  });
  let page = $state<ListPage | null>(null);
  let items = $state<ListItem[]>([]);
  let listError = $state<ContractError | null>(null);
  let listLoading = $state(false);
  let gen = 0;
  const listKey = $derived(JSON.stringify([tkey, view.node, view.query, view.filters, view.sort, view.group, view.collapsed, compared?.other ?? null, assets && view.sounds]));
  $effect(() => {
    void listKey;
    const r = ready;
    untrack(() => {
      if (!r) return;
      if (assets && view.sounds) void loadTriggers();
      else void loadList(0);
    });
  });
  async function loadList(offset: number) {
    const g = ++gen;
    listLoading = true;
    try {
      const p = await devApi.list({ target: $state.snapshot(target), query: { node: view.node, query: view.query, filters: $state.snapshot(view.filters), sort: view.sort, group: view.group, collapsed: [...view.collapsed], offset, limit: PAGE, main_only: true } });
      if (g !== gen) return;
      page = p;
      items = offset ? [...items, ...p.items] : p.items;
      listError = null;
      if (!offset) listActive = 0;
    } catch (e) {
      if (g === gen) listError = toContractError(e);
    } finally {
      if (g === gen) listLoading = false;
    }
  }
  // Filters are remembered per section.
  const filtersKey = $derived(JSON.stringify(view.filters));
  $effect(() => {
    void filtersKey;
    untrack(() => F.saveFilters(store, F.sectionOf(view.node), $state.snapshot(view.filters)));
  });

  // Sound events (Assets mode): ATL triggers by name; the asset worker starts on first use.
  let triggers = $state<TriggerHit[]>([]);
  let triggerTotal = $state(0);
  async function loadTriggers() {
    const g = ++gen;
    if (view.query.length < 2) {
      triggers = [];
      triggerTotal = 0;
      listError = null;
      return;
    }
    listLoading = true;
    try {
      const r = await assetApi.soundSearch({ target: $state.snapshot(target), query: view.query, limit: PAGE });
      if (g !== gen) return;
      triggers = r.hits;
      triggerTotal = r.total;
      listError = null;
      listActive = 0;
    } catch (e) {
      if (g === gen) listError = toContractError(e);
    } finally {
      if (g === gen) listLoading = false;
    }
  }

  type Row = { kind: 'head'; key: string; label: string; count: number; collapsed: boolean } | { kind: 'item'; it: ListItem } | { kind: 'trigger'; t: TriggerHit } | { kind: 'more'; n: number };
  const listRows = $derived.by<Row[]>(() => {
    if (assets && view.sounds) return triggers.map((t) => ({ kind: 'trigger', t }));
    const out: Row[] = [];
    if (page && page.groups.length) {
      const by = new Map<string, ListItem[]>();
      for (const it of items) {
        const k = it.group ?? '';
        if (!by.has(k)) by.set(k, []);
        by.get(k)!.push(it);
      }
      for (const g of page.groups) {
        const its = by.get(g.key) ?? [];
        if (!g.collapsed && its.length === 0) continue;
        out.push({ kind: 'head', key: g.key, label: g.label, count: g.count, collapsed: g.collapsed });
        for (const it of its) out.push({ kind: 'item', it });
      }
    } else for (const it of items) out.push({ kind: 'item', it });
    if (page && items.length < page.total) out.push({ kind: 'more', n: Math.min(PAGE, page.total - items.length) });
    return out;
  });
  const rowH = (r: Row) => (r.kind === 'head' ? HROW : r.kind === 'more' ? 40 : LROW);
  const tops = $derived.by(() => {
    const t: number[] = [];
    let y = 0;
    for (const r of listRows) {
      t.push(y);
      y += rowH(r);
    }
    t.push(y);
    return t;
  });
  let listActive = $state(0);
  let listScroller = $state<HTMLElement | null>(null);
  let listTop = $state(0);
  let listH = $state(400);
  const firstAt = (y: number) => {
    let lo = 0;
    let hi = listRows.length;
    while (lo < hi) {
      const m = (lo + hi) >> 1;
      if (tops[m + 1]! <= y) lo = m + 1;
      else hi = m;
    }
    return lo;
  };
  const lFirst = $derived(Math.max(0, firstAt(listTop) - OVERSCAN));
  const lLast = $derived(Math.min(listRows.length - 1, firstAt(listTop + listH) + OVERSCAN));
  function revealList(i: number) {
    if (!listScroller) return;
    const top = tops[i] ?? 0;
    const h = listRows[i] ? rowH(listRows[i]!) : LROW;
    if (top < listScroller.scrollTop) listScroller.scrollTop = top;
    else if (top + h > listScroller.scrollTop + listScroller.clientHeight) listScroller.scrollTop = top + h - listScroller.clientHeight;
    listTop = listScroller.scrollTop;
  }
  function activateRow(r: Row) {
    if (r.kind === 'item' || r.kind === 'trigger') onremember?.(view.query);
    if (r.kind === 'item') onopen(r.it.guid);
    else if (r.kind === 'trigger') onopentrigger(r.t.name);
    else if (r.kind === 'head') view = { ...view, collapsed: r.collapsed ? view.collapsed.filter((k) => k !== r.key) : [...view.collapsed, r.key] };
    else void loadList(items.length);
  }
  function onListKey(e: KeyboardEvent) {
    if (e.altKey || e.ctrlKey || e.metaKey || !listRows.length) return;
    const per = Math.max(1, Math.floor(listH / LROW) - 1);
    const map: Record<string, number> = { ArrowDown: listActive + 1, ArrowUp: listActive - 1, PageDown: listActive + per, PageUp: listActive - per, Home: 0, End: listRows.length - 1 };
    if (e.key in map) {
      e.preventDefault();
      listActive = Math.max(0, Math.min(listRows.length - 1, map[e.key]!));
      revealList(listActive);
    } else if (e.key === 'Enter' || (e.key === ' ' && listRows[listActive]?.kind === 'head')) {
      e.preventDefault();
      activateRow(listRows[listActive]!);
    }
  }
  $effect(() => {
    const node = treeScroller;
    if (!node) return;
    const ro = new ResizeObserver(() => (treeH = node.clientHeight));
    ro.observe(node);
    return () => ro.disconnect();
  });
  $effect(() => {
    const node = listScroller;
    if (!node) return;
    const ro = new ResizeObserver(() => (listH = node.clientHeight));
    ro.observe(node);
    return () => ro.disconnect();
  });

  // --- Filters, chips, presets -------------------------------------------------------------
  let side = $state<'sections' | 'filters'>('sections');
  let drawer = $state(false);
  let presets = $state<F.Preset[]>(F.loadPresets(store));
  let presetName = $state('');
  let showPresets = $state(false);
  const facets = $derived(page?.facets ?? null);
  const labelOf = (key: F.FacetKey, value: string) => (facets?.[key] as FacetValue[] | undefined)?.find((v) => v.value === value)?.label ?? value;
  const chips = $derived(F.activeChips(view.filters, labelOf));
  const setFilters = (f: typeof view.filters) => (view = { ...view, filters: f, collapsed: [] });
  const flip = (k: 'model' | 'sounds' | 'player_only') => setFilters({ ...view.filters, [k]: !view.filters[k] });
  let more = $state<Record<string, boolean>>({});
  function applyPreset(p: F.Preset) {
    view = { ...view, node: p.node, query: p.query, filters: F.fullFilters(p.filters), sort: p.sort, group: p.group, collapsed: [] };
    showPresets = false;
  }
  function storePreset() {
    presets = F.savePreset(store, { name: presetName, query: view.query, filters: $state.snapshot(view.filters), sort: view.sort, group: view.group, node: view.node });
    presetName = '';
  }
  let listW = $state(600);
  // Stat columns only where the name keeps room (about 300 px); the rest read inline.
  const maxCols = $derived(Math.max(0, Math.min(4, Math.floor((listW - 300) / 72))));
  const allCols = $derived(F.statColumns(items, 6));
  const cols = $derived(allCols.slice(0, maxCols));
  const inlineStats = (it: ListItem) =>
    allCols
      .slice(maxCols)
      .map((c) => {
        const st = it.stats.find((x) => x.key === c.key);
        return st ? `${c.label} ${F.statText(st)}` : '';
      })
      .filter(Boolean)
      .join(' · ');
  const words = $derived(page?.words ?? []);
  const changeLabel = (c: string | null) => (c === 'added' ? 'New' : c === 'changed' ? 'Changed' : '');
</script>

{#snippet hl(text: string)}
  {#each F.highlight(text, words) as part, k (k)}{#if part.hit}<mark class="bg-primary/25 text-foreground rounded-[2px]">{part.text}</mark>{:else}{part.text}{/if}{/each}
{/snippet}

{#snippet facetSidebar()}
  <div class="grid gap-3 p-2.5 text-[12px]" role="group" aria-label="Filters">
    {#if !facets}
      <div class="grid gap-1">{#each { length: 8 } as _, i (i)}<Skeleton class="h-4 w-full" />{/each}</div>
    {:else}
      {#each F.FACETS as fd (fd.key)}
        {@const vals = facets[fd.key] as FacetValue[]}
        {#if vals.length > 0}
          {@const shown = more[fd.key] ? vals : vals.slice(0, 8)}
          <fieldset class="grid min-w-0 gap-0.5">
            <legend class="text-muted-foreground mb-0.5 text-[11px] font-medium tracking-wide uppercase">{fd.label}</legend>
            {#each shown as v (v.value)}
              {@const on = F.has(view.filters, fd.key, v.value)}
              <label class={cn('hover:bg-accent/60 flex min-w-0 cursor-pointer items-center gap-2 rounded px-1 py-0.5', v.count === 0 && !on && 'opacity-50')}>
                <input type="checkbox" class="accent-primary size-3.5 shrink-0" checked={on} onchange={() => setFilters(F.toggle(view.filters, fd.key, v.value))} />
                {#if fd.key === 'sections'}<SectionIcon id={v.value} />{/if}
                <span class="min-w-0 flex-1 truncate" title={v.label}>{v.label}</span>
                <span class="text-muted-foreground mono text-[11px]">{int(v.count)}</span>
              </label>
            {/each}
            {#if vals.length > 8}
              <button type="button" class="text-primary justify-self-start px-1 text-[11.5px] hover:underline" onclick={() => (more = { ...more, [fd.key]: !more[fd.key] })}>{more[fd.key] ? 'Fewer' : `All ${vals.length}`}</button>
            {/if}
          </fieldset>
        {/if}
      {/each}
    {/if}
  </div>
{/snippet}

{#snippet sidePane()}
  <div class="flex h-9 items-center gap-2 border-b px-2">
    <Segmented size="sm" ariaLabel="Side pane" options={[{ id: 'sections', label: assets ? 'Media' : 'Sections' }, { id: 'filters', label: F.filterCount(view.filters) ? `Filters · ${F.filterCount(view.filters)}` : 'Filters' }] as { id: 'sections' | 'filters'; label: string }[]} bind:value={side} />
  </div>
  {#if side === 'filters'}
    <div class="min-h-0 flex-1 overflow-auto">{@render facetSidebar()}</div>
  {:else}
    {#if treeError}<p class="text-destructive p-3 text-[12px]">{treeError.message}</p>{/if}
    <div bind:this={treeScroller} class="relative min-h-0 flex-1 overflow-auto" onscroll={() => (treeTop = treeScroller?.scrollTop ?? 0)}>
      {#if !loaded.has('')}
        <div class="grid gap-1 p-2">{#each { length: 10 } as _, i (i)}<Skeleton class="h-5 w-full" />{/each}</div>
      {:else}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div
          role="tree"
          aria-label={assets ? 'Models and sounds by section' : 'Sections'}
          tabindex="0"
          aria-activedescendant={rows[tIndex] ? `ft-${tIndex}` : undefined}
          class="focus-visible:ring-ring/60 relative outline-none focus-visible:ring-2 focus-visible:ring-inset"
          style="height: {rows.length * TROW}px"
          onkeydown={onTreeKey}
        >
          {#each rows.slice(tFirst, tLast + 1) as r, k (r.id)}
            {@const i = tFirst + k}
            {@const sec = r.node ? (r.node.id.startsWith('m:') && r.level === 1 ? r.node.id.slice(2, 8) : F.sectionOf(r.node.id)) : ''}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              id="ft-{i}"
              role="treeitem"
              aria-level={r.level}
              aria-posinset={r.pos}
              aria-setsize={r.size}
              aria-expanded={r.node && r.node.kind !== 'record' ? r.expanded : undefined}
              aria-selected={i === tIndex}
              aria-label={r.node ? `${r.node.label}${r.node.kind !== 'record' ? `, ${r.node.count} records` : ''}` : `Show ${Math.min(PAGE, r.more)} more`}
              tabindex="-1"
              class={cn('absolute right-0 left-0 flex cursor-pointer items-center gap-1.5 pr-2 text-[12.5px] whitespace-nowrap', i === tIndex && 'bg-accent/70', view.node === r.id && 'text-primary font-medium', selected && r.id === `r:${selected}` && 'text-primary')}
              style="top: {i * TROW}px; height: {TROW}px; padding-left: {4 + (r.level - 1) * 14}px"
              onclick={() => {
                treeActive = r.id;
                if (!r.node) void fetchChildren(r.parent, loaded.get(r.parent)?.nodes.length ?? 0);
                else if (r.node.kind === 'record') selectNode(r.node);
                else {
                  void toggleNode(r.id, !r.expanded);
                  selectNode(r.node);
                }
              }}
            >
              {#if r.node && r.node.kind !== 'record'}
                <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', r.expanded && 'rotate-90')} aria-hidden="true" />
                {#if r.node.kind === 'section'}<SectionIcon id={sec} />{/if}
                <span class="truncate">{r.node.label}</span>
                <span class="text-muted-foreground mono ml-auto pl-2 text-[11px]">{int(r.node.count)}</span>
              {:else if r.node}
                <span class="w-3.5 shrink-0"></span>
                <span class={cn('truncate text-[12px]', !r.node.main && 'text-muted-foreground')}>{r.node.label}</span>
              {:else}
                <span class="w-3.5 shrink-0"></span>
                <span class="text-primary text-[12px]">Show {int(Math.min(PAGE, r.more))} more</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
{/snippet}

<div class="relative flex min-h-0 min-w-0 flex-1 gap-0" data-finder>
  {#if !narrow}
    <aside data-pane="tree" class="bg-card flex w-[236px] shrink-0 flex-col overflow-hidden rounded-lg border" aria-label={assets ? 'Media' : 'Sections and filters'}>
      {@render sidePane()}
    </aside>
    <div class="w-2 shrink-0"></div>
  {:else if drawer}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <aside
      id="finder-drawer"
      data-pane="tree"
      class="bg-popover absolute top-0 bottom-0 left-0 z-20 flex w-[min(280px,80%)] flex-col overflow-hidden rounded-lg border shadow-lg"
      aria-label="Sections and filters"
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          drawer = false;
        }
      }}
    >
      {@render sidePane()}
    </aside>
  {/if}

  <section data-pane="list" bind:clientWidth={listW} class="bg-card flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border" aria-label={assets && view.sounds ? 'Sound events' : 'Items'}>
    <!-- Mode-specific switches, quick filters, sort and grouping. -->
    <div class="grid gap-1.5 border-b px-2.5 py-2">
      <div class="flex flex-wrap items-center gap-1.5">
        {#if narrow}
          <Button variant="outline" size="xs" aria-expanded={drawer} aria-controls="finder-drawer" onclick={() => (drawer = !drawer)}><ListFilter aria-hidden="true" /> {assets ? 'Media' : 'Sections'} & filters{F.filterCount(view.filters) ? ` · ${F.filterCount(view.filters)}` : ''}</Button>
        {/if}
        {#if assets}
          <Segmented size="sm" ariaLabel="Find" options={[{ id: 'entities', label: 'Entities' }, { id: 'sounds', label: 'Sound events' }] as { id: 'entities' | 'sounds'; label: string }[]} value={view.sounds ? 'sounds' : 'entities'} onchange={(v) => (view = { ...view, sounds: v === 'sounds' })} />
        {/if}
        {#if !(assets && view.sounds)}
          {#each [{ k: 'player_only', label: 'Player-usable', n: facets?.player }, { k: 'model', label: 'Has model', n: facets?.model }, { k: 'sounds', label: 'Has sounds', n: facets?.sounds }] as const as q (q.k)}
            <button type="button" aria-pressed={view.filters[q.k]} class={cn('focus-visible:ring-ring/60 inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11.5px] outline-none focus-visible:ring-2', view.filters[q.k] ? 'bg-primary text-primary-foreground border-primary' : 'hover:bg-accent')} onclick={() => flip(q.k)}>
              {#if q.k === 'model'}<Box class="size-3" aria-hidden="true" />{:else if q.k === 'sounds'}<Volume2 class="size-3" aria-hidden="true" />{/if}{q.label}{#if q.n !== undefined}<span class="mono opacity-75">{int(q.n)}</span>{/if}
            </button>
          {/each}
          <Button variant="ghost" size="xs" class="ml-auto h-6" aria-expanded={showPresets} aria-controls="finder-presets" onclick={() => (showPresets = !showPresets)}><Bookmark aria-hidden="true" /> Presets{presets.length ? ` · ${presets.length}` : ''}</Button>
          {#if facets && facets.changes.length}
            {#each facets.changes as c (c.value)}
              {@const on = view.filters.changes.includes(c.value)}
              <button type="button" aria-pressed={on} class={cn('inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11.5px]', on ? 'border-amber-500 bg-amber-500 text-black' : 'hover:bg-accent border-amber-500/50')} onclick={() => setFilters(F.toggle(view.filters, 'changes', c.value))}>{c.label} vs {compared?.other}<span class="mono opacity-75">{int(c.count)}</span></button>
            {/each}
          {/if}
        {/if}
      </div>
      {#if !(assets && view.sounds) && (chips.length || page?.understood.length || view.node)}
        <div class="flex flex-wrap items-center gap-1" aria-label="Active filters">
          {#if view.node}
            <span class="bg-accent inline-flex h-6 max-w-full items-center gap-1 rounded-full px-2 text-[11.5px]">
              <SectionIcon id={view.node.startsWith('m:') ? view.node.slice(2, 8) : F.sectionOf(view.node)} />
              <span class="truncate">{page?.title || 'Selected node'}</span>
              <button type="button" class="hover:text-foreground text-muted-foreground" aria-label="Show every record" onclick={() => (view = { ...view, node: assets ? 'm:models:[]' : '', collapsed: [] })}><X class="size-3" aria-hidden="true" /></button>
            </span>
          {/if}
          {#each page?.understood ?? [] as u (u)}
            <span class="bg-primary/10 text-primary mono inline-flex h-6 items-center rounded-full px-2 text-[11px]" title="Understood search filter">{u}</span>
          {/each}
          {#each chips as c (`${c.key}:${c.value}`)}
            <span class="bg-accent inline-flex h-6 items-center gap-1 rounded-full px-2 text-[11.5px]">
              {c.label}
              <button type="button" class="text-muted-foreground hover:text-foreground" aria-label="Remove filter {c.label}" onclick={() => setFilters(F.removeChip(view.filters, c))}><X class="size-3" aria-hidden="true" /></button>
            </span>
          {/each}
          {#if chips.length}
            <Button variant="link" size="xs" class="h-6 px-1" onclick={() => setFilters({ ...F.NO_FILTERS })}>Clear all</Button>
          {/if}
        </div>
      {/if}
      {#if !(assets && view.sounds)}
        {#if page && view.query && page.facets.sections.length > 1}
        <div class="flex flex-wrap items-center gap-1.5 text-[11.5px]">
            <span class="text-muted-foreground">In:</span>
            {#each page.facets.sections as s (s.value)}
              <button type="button" class="hover:bg-accent inline-flex h-6 items-center gap-1 rounded px-1.5" title="Only {s.label}" onclick={() => setFilters({ ...view.filters, sections: [s.value] })}><SectionIcon id={s.value} />{s.label}<span class="text-muted-foreground mono">{int(s.count)}</span></button>
            {/each}
        </div>
        {/if}
        {#if showPresets}
          <div id="finder-presets" class="bg-background/60 grid gap-1.5 rounded-md border p-2 text-[12px]">
            <form class="flex gap-1.5" onsubmit={(e) => (e.preventDefault(), storePreset())}>
              <input class="border-input bg-background h-7 min-w-0 flex-1 rounded-md border px-2" placeholder="Name this search and filters" aria-label="Preset name" bind:value={presetName} />
              <Button type="submit" size="xs" disabled={!presetName.trim()}>Save</Button>
            </form>
            {#each presets as p (p.name)}
              <div class="flex items-center gap-1.5">
                <button type="button" class="hover:bg-accent min-w-0 flex-1 truncate rounded px-1 py-0.5 text-left" onclick={() => applyPreset(p)}>{p.name} <span class="text-muted-foreground mono text-[11px]">{p.query}</span></button>
                <button type="button" class="text-muted-foreground hover:text-foreground" aria-label="Delete preset {p.name}" onclick={() => (presets = F.deletePreset(store, p.name))}><X class="size-3.5" aria-hidden="true" /></button>
              </div>
            {:else}
              <p class="text-muted-foreground">No presets yet. Saved here: the search, filters, sort, grouping and section.</p>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
    <div class="flex min-h-8 flex-wrap items-center gap-x-2 gap-y-1 border-b px-3 py-1 text-[12px]">
      <span class="min-w-0 flex-1 truncate font-medium">{assets && view.sounds ? (view.query ? `Sound events “${view.query}”` : 'Sound events') : page?.title || (view.query ? `Search “${view.query}”` : 'Every record')}</span>
      {#if listLoading}<LoaderCircle class="text-muted-foreground size-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" />{/if}
      <span class="text-muted-foreground mono ml-auto shrink-0" aria-live="polite">
        {#if assets && view.sounds}{int(triggers.length)} of {int(triggerTotal)}{:else if page}{int(items.length)} of {int(page.total)}{page.matched > page.total ? ` (${int(page.matched)} with folded groups)` : ''}{/if}
      </span>
      {#if !(assets && view.sounds)}
                    <label class="text-muted-foreground flex items-center gap-1 text-[11.5px]">Sort
          <select class="border-input bg-background text-foreground h-6 rounded-md border px-1 text-[11.5px]" value={view.sort} onchange={(e) => (view = { ...view, sort: e.currentTarget.value as typeof view.sort })}>
            {#each F.SORTS as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
          </select>
        </label>
        <label class="text-muted-foreground flex items-center gap-1 text-[11.5px]">Group
          <select class="border-input bg-background text-foreground h-6 rounded-md border px-1 text-[11.5px]" value={view.group} onchange={(e) => (view = { ...view, group: e.currentTarget.value as typeof view.group, collapsed: [] })}>
            {#each F.GROUPS as g (g.id)}<option value={g.id}>{g.label}</option>{/each}
          </select>
        </label>
      {/if}
    </div>
    {#if page && page.matching !== 'exact' && !(assets && view.sounds)}
      <p class="border-b bg-amber-500/10 px-3 py-1 text-[11.5px]" role="status">Nothing matches “{view.query}” as typed; showing {page.matching === 'typo' ? 'close spellings' : 'abbreviation matches'}.</p>
    {/if}
    {#if assets && view.sounds && triggerTotal > triggers.length}
      <p class="text-muted-foreground border-b px-3 py-1 text-[11.5px]" role="status">Showing the first {int(triggers.length)} of {int(triggerTotal)} sound events; add words to narrow them.</p>
    {/if}
    {#if cols.length && !(assets && view.sounds)}
      <div class="text-muted-foreground grid h-6 items-center border-b px-3 text-[10.5px] tracking-wide uppercase" style="grid-template-columns: minmax(0,1fr) repeat({cols.length}, 76px)" aria-hidden="true">
        <span></span>
        {#each cols as c (c.key)}<span class="text-right">{c.label}</span>{/each}
      </div>
    {/if}
    {#if listError}
      <div class="p-3"><ErrorState title={assets && view.sounds ? 'Could not search sound events' : 'Could not list records'} error={listError} /></div>
    {:else if assets && view.sounds && view.query.length < 2}
      <p class="text-muted-foreground p-4 text-[12.5px]">Type a sound event name: words like <span class="mono">gladius</span>, <span class="mono">quantum</span>, <span class="mono">explosion</span>. Every word must appear in the trigger, its Wwise event or bank. The first search reads the game's audio index (a few seconds).</p>
    {:else if !listRows.length}
      <p class="text-muted-foreground p-4 text-[12.5px]">{page ? 'Nothing matches. Remove a filter, or try fewer words; prefixes like type:, maker:, size: narrow the search.' : 'Loading…'}</p>
    {:else}
      <div bind:this={listScroller} class="relative min-h-0 flex-1 overflow-auto" onscroll={() => (listTop = listScroller?.scrollTop ?? 0)}>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div
          role="listbox"
          aria-label={assets && view.sounds ? 'Sound events' : 'Items'}
          tabindex="0"
          aria-activedescendant={`fl-${listActive}`}
          class="focus-visible:ring-ring/60 relative outline-none focus-visible:ring-2 focus-visible:ring-inset"
          style="height: {tops[listRows.length]}px"
          onkeydown={onListKey}
        >
          {#each listRows.slice(lFirst, lLast + 1) as r, k (k + lFirst)}
            {@const i = lFirst + k}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              id="fl-{i}"
              role="option"
              aria-selected={i === listActive}
              aria-label={r.kind === 'head' ? `${r.label}, ${r.count} records, ${r.collapsed ? 'folded' : 'open'}` : undefined}
              tabindex="-1"
              class={cn('absolute right-0 left-0 cursor-pointer', i === listActive && 'bg-accent/70', r.kind === 'item' && selected === r.it.guid && 'border-l-primary border-l-2', r.kind === 'trigger' && selectedTrigger === r.t.name && 'border-l-primary border-l-2')}
              style="top: {tops[i]}px; height: {rowH(r)}px"
              onclick={() => {
                listActive = i;
                activateRow(r);
              }}
            >
              {#if r.kind === 'head'}
                <div class="bg-muted/60 flex h-full items-center gap-1.5 border-b px-2 text-[11.5px] font-medium">
                  <ChevronRight class={cn('size-3.5 transition-transform motion-reduce:transition-none', !r.collapsed && 'rotate-90')} aria-hidden="true" />
                  {#if view.group === 'section'}<SectionIcon id={r.key} />{/if}
                  <span class="truncate">{r.label}</span>
                  <span class="text-muted-foreground mono ml-auto">{int(r.count)}</span>
                </div>
              {:else if r.kind === 'item'}
                {@const it = r.it}
                <div class="hover:bg-accent/50 grid h-full items-center gap-x-2 border-b border-border/60 px-3" style="grid-template-columns: 16px minmax(0,1fr) {cols.length ? `repeat(${cols.length}, 76px)` : ''}">
                  <SectionIcon id={it.section} class="self-center" />
                  <div class="grid min-w-0 gap-0.5">
                    <div class="flex min-w-0 items-baseline gap-1.5">
                      <span class={cn('min-w-0 shrink truncate text-[12.5px] font-medium', it.variant && 'text-muted-foreground')} title={it.label}>{#if dev.showIds}{@render hl(it.name)}{:else}{@render hl(it.label)}{/if}</span>
                      {#if it.detail && !dev.showIds && listW >= 520}<span class="text-muted-foreground mono min-w-0 shrink-[4] truncate text-[11px]" title={it.name}>{@render hl(it.detail)}</span>{/if}
                    </div>
                    <div class="text-muted-foreground flex min-w-0 items-center gap-1.5 text-[11px]">
                      <span class="truncate">{it.kind_label ?? it.kind ?? it.type}{it.sub_type ? ` · ${it.sub_label ?? it.sub_type}` : ''}{it.size !== null ? ` · S${it.size}` : ''}{it.maker_label ? ` · ${it.maker_label}` : it.manufacturer ? ` · ${it.manufacturer}` : ''}{it.grade ? ` · Grade ${it.grade}` : ''}{it.class ? ` · ${it.class}` : ''}{inlineStats(it) ? ` · ${inlineStats(it)}` : ''}</span>
                      <span class="ml-auto flex shrink-0 items-center gap-1">
                        {#if it.change}<span class="rounded bg-amber-500/20 px-1 text-[10px] font-medium text-amber-800 dark:text-amber-300">{changeLabel(it.change)}</span>{/if}
                        {#if it.variant}<span class="rounded border px-1 text-[10px]" title="AI, NPC, test or template variant">AI/test</span>{/if}
                        {#if it.model}<Box class="text-primary size-3" aria-label="Has a model" />{/if}
                        {#if it.sounds}<Volume2 class="text-primary size-3" aria-label="Has sounds" />{/if}
                      </span>
                    </div>
                  </div>
                  {#each cols as c (c.key)}
                    <span class="mono text-right text-[11.5px] tabular-nums">{F.statText(it.stats.find((s) => s.key === c.key))}</span>
                  {/each}
                </div>
              {:else if r.kind === 'trigger'}
                <div class="hover:bg-accent/50 grid h-full content-center gap-0.5 border-b border-border/60 px-3">
                  <span class="mono truncate text-[12px] font-medium">{@render hl(r.t.name)}</span>
                  <span class="text-muted-foreground mono truncate text-[11px]">{r.t.bank}{r.t.event !== r.t.name ? ` · ${r.t.event}` : ''}{r.t.duration_type ? ` · ${r.t.duration_type}` : ''}</span>
                </div>
              {:else}
                <div class="flex h-full items-center px-3"><span class="text-primary text-[12px]">Show {int(r.n)} more</span></div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </section>
</div>
