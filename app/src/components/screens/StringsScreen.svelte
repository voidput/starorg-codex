<script lang="ts">
  /**
   * Strings (spec 9.4): every key of the channel (about 91,000) in a virtualised grid. Rows are
   * fetched in pages as they scroll into view, so the DOM holds about 40 rows at any time.
   * Search, source facets, groups and "changed only" filter on the backend. Keyboard: arrows,
   * Page Up/Down, Home/End move the row; Enter opens the detail sheet. One row is always in the
   * tab order: the active row, or the first visible one once the active row has scrolled out of
   * the window. Columns follow the grid's width (Group goes first, then Original).
   *
   * Groups (addendum 8.5): a 240 px group tree beside the grid (a "Groups" popover below
   * 1,000 px, a 36 px rail when collapsed), group-by None / Type / Manufacturer / Size, and a
   * removable chip per selected group, manufacturer and size. Selection, open nodes and
   * group-by are remembered per channel in this webview's local storage. F6 / Shift+F6 move
   * between the tree, the filter bar and the grid.
   */
  import Search from '@lucide/svelte/icons/search';
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import ListTree from '@lucide/svelte/icons/list-tree';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Button } from '@/components/ui/button';
  import GroupTree from '@/components/strings/GroupTree.svelte';
  import { tick, untrack } from 'svelte';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import StringDetailSheet from '@/components/strings/StringDetailSheet.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { FacetCount, GroupBy, GroupNode, GroupState, StringRow, StringSort, StringSource } from '@/lib/contract/types';
  import { CATEGORY_LABEL, int, SOURCE_LABEL, SOURCE_TONE } from '@/lib/format';
  import { chips as groupChips, parsePrefs, sizeId, sizeOf, type GroupPrefs } from '@/lib/grouptree';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  const ROW = 34;
  const PAGE = 200;
  const OVERSCAN = 8;

  let search = $state('');
  let debounced = $state('');
  let sources = $state<StringSource[]>([]);
  let changedOnly = $state(false);
  let sort = $state<StringSort>('group');

  let total = $state<number | null>(null);
  let all = $state(0);
  let sourceFacets = $state<FacetCount[]>([]);
  let groupNodes = $state<GroupNode[]>([]);
  let groupState = $state<GroupState>('exact');
  /** `dim:id` -> label of every node seen, for chips of a dimension the tree is not showing. */
  const known = new Map<string, string>();
  let pages = $state<Record<number, StringRow[]>>({});
  let error = $state<ContractError | null>(null);
  let gen = 0;
  const loading = new Set<number>();

  let scroller = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(600);
  let gridW = $state(1000);
  let grid = $state<HTMLElement | null>(null);
  let active = $state(0);
  let openKey = $state<string | null>(null);
  let sheet = $state(false);

  $effect(() => {
    const s = search;
    const t = setTimeout(() => (debounced = s.trim()), 200);
    return () => clearTimeout(t);
  });

  const target = $derived(app.target);
  const tkey = $derived(target ? `${target.install}/${target.channel}` : '');

  // Group prefs per channel (local storage; may be blocked or stale, so always validated).
  const PREFS = (k: string) => `starorg.strings.groups.${k}`;
  const RAIL = 'starorg.strings.tree-collapsed';
  let prefs = $state<GroupPrefs>(parsePrefs(null));
  let prefsFor = '';
  let railed = $state(readRail());
  function readRail(): boolean {
    try {
      return localStorage.getItem(RAIL) === '1';
    } catch {
      return false;
    }
  }
  $effect(() => {
    const k = tkey;
    if (!k || k === prefsFor) return;
    untrack(() => {
      try {
        prefs = parsePrefs(localStorage.getItem(PREFS(k)));
      } catch {
        prefs = parsePrefs(null);
      }
      prefsFor = k;
    });
  });
  $effect(() => {
    const v = JSON.stringify(prefs);
    if (!prefsFor) return;
    try {
      localStorage.setItem(PREFS(prefsFor), v);
    } catch {
      /* storage blocked: prefs last for this session only */
    }
  });
  $effect(() => {
    const v = railed ? '1' : '0';
    try {
      localStorage.setItem(RAIL, v);
    } catch {
      /* ignore */
    }
  });

  let groupsRev = $state(0);
  const groupBy = $derived<GroupBy>(prefs.groupBy);
  const effSort = $derived<StringSort>(groupBy === 'none' && sort === 'group' ? 'key' : sort);
  const qkey = $derived(
    JSON.stringify([tkey, prefsFor === tkey, debounced, sources, changedOnly, effSort, prefs.groupBy, prefs.groups, prefs.manufacturers, prefs.sizes, app.rev, groupsRev]),
  );

  function query(offset: number) {
    return {
      target: $state.snapshot(target!),
      search: debounced,
      sources: [...sources],
      categories: [],
      changed_only: changedOnly,
      sort: effSort,
      offset,
      limit: PAGE,
      groups: [...prefs.groups],
      manufacturers: [...prefs.manufacturers],
      sizes: [...prefs.sizes],
      group_by: prefs.groupBy,
    };
  }

  async function loadPage(p: number, g: number) {
    if (loading.has(p) || pages[p] || !target) return;
    loading.add(p);
    try {
      const r = await api.queryStrings(query(p * PAGE));
      if (g !== gen) return;
      pages[p] = r.rows;
      total = r.total;
      all = r.all;
      sourceFacets = r.sources;
      groupNodes = r.groups;
      groupState = r.group_state;
      const dim = r.groups.length && prefs.groupBy !== 'none' ? ({ type: 'groups', manufacturer: 'manufacturers', size: 'sizes' } as const)[prefs.groupBy] : null;
      if (dim) for (const n of r.groups) known.set(`${dim}:${n.id}`, n.label);
      error = null;
    } catch (e) {
      if (g === gen) error = toContractError(e);
    } finally {
      loading.delete(p);
    }
  }

  // A new query resets the cache and scrolls to the top.
  $effect(() => {
    void qkey;
    if (!tkey || prefsFor !== tkey) return;
    untrack(() => {
      gen++;
      pages = {};
      loading.clear();
      total = null;
      active = 0;
      if (scroller) scroller.scrollTop = 0;
      scrollTop = 0;
      void loadPage(0, gen);
    });
  });

  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const last = $derived(Math.min((total ?? 0) - 1, Math.ceil((scrollTop + viewH) / ROW) + OVERSCAN));
  $effect(() => {
    if (total === null) return;
    const lo = Math.floor(first / PAGE);
    const hi = Math.floor(Math.max(last, 0) / PAGE);
    untrack(() => {
      for (let p = lo; p <= hi; p++) void loadPage(p, gen);
    });
  });
  const visible = $derived.by(() => {
    const out: { i: number; row: StringRow | null }[] = [];
    for (let i = first; i <= last; i++) out.push({ i, row: pages[Math.floor(i / PAGE)]?.[i % PAGE] ?? null });
    return out;
  });

  $effect(() => {
    if (!scroller) return;
    const ro = new ResizeObserver(() => (viewH = scroller!.clientHeight));
    ro.observe(scroller);
    return () => ro.disconnect();
  });
  $effect(() => {
    if (!grid) return;
    const ro = new ResizeObserver(() => (gridW = grid!.clientWidth));
    ro.observe(grid);
    return () => ro.disconnect();
  });
  const showOriginal = $derived(gridW >= 620);
  const showGroup = $derived(gridW >= 880);
  const colCount = $derived(3 + Number(showOriginal) + Number(showGroup));
  const cols = $derived(
    showGroup
      ? 'grid grid-cols-[minmax(200px,1.1fr)_minmax(0,1fr)_minmax(0,1fr)_96px_128px]'
      : showOriginal
        ? 'grid grid-cols-[minmax(180px,1.1fr)_minmax(0,1fr)_minmax(0,1fr)_92px]'
        : 'grid grid-cols-[minmax(160px,1.2fr)_minmax(0,1.4fr)_92px]',
  );
  /** The row in the tab order: the active one while it is in the DOM, else the first visible one. */
  const firstShown = $derived(Math.min(Math.max(0, (total ?? 1) - 1), Math.floor(scrollTop / ROW)));
  const tabRow = $derived(active >= first && active <= last ? active : firstShown);

  function rowAt(i: number) {
    return pages[Math.floor(i / PAGE)]?.[i % PAGE] ?? null;
  }

  async function focusRow(i: number) {
    if (!scroller || total === null || total === 0) return;
    active = Math.max(0, Math.min(total - 1, i));
    const top = active * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + scroller.clientHeight) scroller.scrollTop = top + ROW - scroller.clientHeight;
    scrollTop = scroller.scrollTop;
    await tick();
    scroller.querySelector<HTMLElement>(`[data-row="${active}"]`)?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (total === null) return;
    const per = Math.max(1, Math.floor(viewH / ROW) - 1);
    const map: Record<string, number> = { ArrowDown: active + 1, ArrowUp: active - 1, PageDown: active + per, PageUp: active - per, Home: 0, End: total - 1 };
    if (e.key in map) {
      e.preventDefault();
      void focusRow(map[e.key]!);
    } else if (e.key === 'Enter' || e.key === ' ') {
      const r = rowAt(active);
      if (r) {
        e.preventDefault();
        openRow(r.key);
      }
    }
  }

  function openRow(key: string) {
    openKey = key;
    sheet = true;
  }

  function toggleSource(s: StringSource) {
    sources = sources.includes(s) ? sources.filter((x) => x !== s) : [...sources, s];
  }

  function patchRow(row: StringRow) {
    for (const [p, rows] of Object.entries(pages)) {
      const i = rows.findIndex((r) => r.key === row.key);
      if (i >= 0) pages[Number(p)]![i] = row;
    }
  }

  const facetCount = (id: string) => sourceFacets.find((f) => f.id === id)?.count ?? 0;
  const sorts = $derived<{ id: StringSort; label: string }[]>([
    ...(groupBy === 'none' ? [] : [{ id: 'group' as const, label: 'Group' }]),
    { id: 'key', label: 'Key A–Z' },
    { id: 'key-desc', label: 'Key Z–A' },
    { id: 'category', label: 'Generator' },
    { id: 'source', label: 'Source' },
  ]);
  const groupBys: { id: GroupBy; label: string }[] = [
    { id: 'none', label: 'No grouping' },
    { id: 'type', label: 'Group by type' },
    { id: 'manufacturer', label: 'Group by manufacturer' },
    { id: 'size', label: 'Group by size' },
  ];

  // The tree shows the dimension of group-by; its selection is that dimension's filter.
  const treeSelected = $derived(
    groupBy === 'manufacturer' ? prefs.manufacturers : groupBy === 'size' ? prefs.sizes.map(sizeId) : prefs.groups,
  );
  function onTreeSelect(ids: string[]) {
    if (groupBy === 'manufacturer') prefs.manufacturers = ids;
    else if (groupBy === 'size') prefs.sizes = ids.map(sizeOf);
    else prefs.groups = ids;
  }
  const chipList = $derived.by(() => {
    void groupNodes;
    return groupChips(prefs, known);
  });
  function removeChip(dim: 'groups' | 'manufacturers' | 'sizes', id: string) {
    if (dim === 'sizes') prefs.sizes = prefs.sizes.filter((n) => n !== sizeOf(id));
    else prefs[dim] = prefs[dim].filter((x) => x !== id);
  }
  function clearGroups() {
    prefs.groups = [];
    prefs.manufacturers = [];
    prefs.sizes = [];
  }
  const allLabel = $derived(groupBy === 'manufacturer' ? 'All manufacturers' : groupBy === 'size' ? 'All sizes' : 'All groups');
  /** "All groups" count: the facet total of the tree's dimension. */
  const treeTotal = $derived(groupNodes.filter((n) => !n.parent).reduce((a, n) => a + n.count, 0));

  // Narrow windows get a popover instead of the pane.
  let winW = $state(1280);
  const narrow = $derived(winW < 1000);
  const showPane = $derived(groupBy !== 'none' && !narrow);
  let pop = $state(false);
  let popEl = $state<HTMLElement | null>(null);
  /** Open the popover towards the left when the button sits near the right edge. */
  let popRight = $state(false);
  function togglePop() {
    if (!pop && popEl) {
      const r = popEl.getBoundingClientRect();
      const edge = (root?.getBoundingClientRect().right ?? innerWidth) - 8;
      popRight = r.left + 288 > edge;
    }
    pop = !pop;
  }
  $effect(() => {
    if (!pop) return;
    const close = (e: MouseEvent) => {
      if (popEl && !popEl.contains(e.target as Node)) pop = false;
    };
    window.addEventListener('pointerdown', close, true);
    return () => window.removeEventListener('pointerdown', close, true);
  });
  $effect(() => {
    if (!narrow || groupBy === 'none') pop = false;
  });

  // Approximate groups: read the game data once per channel and session (addendum 8.2).
  let readFor = $state('');
  let reading = $state(false);
  let readError = $state<ContractError | null>(null);
  async function readGroups() {
    if (!target) return;
    const t = $state.snapshot(target);
    reading = true;
    readError = null;
    try {
      const st = await api.readStringGroups(t);
      if (st === 'exact') groupsRev++;
    } catch (e) {
      readError = toContractError(e);
    } finally {
      reading = false;
    }
  }
  $effect(() => {
    if (groupState !== 'approximate' || total === null || groupBy === 'none') return;
    const k = tkey;
    untrack(() => {
      if (readFor === k) return;
      readFor = k;
      void readGroups();
    });
  });

  // F6 / Shift+F6: tree -> filter bar -> grid.
  let root = $state<HTMLElement | null>(null);
  function onF6(e: KeyboardEvent) {
    if (e.key !== 'F6' || !root) return;
    const tree = root.querySelector<HTMLElement>('[role=tree] [tabindex="0"]');
    const bar = root.querySelector<HTMLElement>('input[type=search]');
    const row = root.querySelector<HTMLElement>('[role=grid] [role=row][tabindex="0"]');
    const stops = [tree, bar, row].filter((x): x is HTMLElement => !!x);
    if (!stops.length) return;
    e.preventDefault();
    const at = stops.findIndex((x) => x === document.activeElement || (x.closest('[role=tree],[role=search],[role=grid]')?.contains(document.activeElement) ?? false));
    const next = at < 0 ? 0 : (at + (e.shiftKey ? stops.length - 1 : 1)) % stops.length;
    stops[next]!.focus();
  }
  const SRC: StringSource[] = ['unmodified', 'enhanced', 'patched', 'edited'];
</script>

<svelte:window bind:innerWidth={winW} />

{#snippet approx()}
  {#if groupState === 'approximate'}
    <div class="text-muted-foreground flex items-start gap-1.5 px-2 py-1.5 text-[11.5px]" role="status">
      {#if reading}
        <LoaderCircle class="mt-0.5 size-3.5 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" />
        <span>Approximate groups – reading game data…</span>
      {:else if readError}
        <span class="min-w-0 flex-1">Approximate groups. {readError.code === 'busy' ? 'Another extraction is running.' : 'Could not read the game data.'}</span>
        <Button variant="link" size="xs" class="h-auto p-0" onclick={() => void readGroups()}>Retry</Button>
      {:else}
        <span>Approximate groups (from key names).</span>
      {/if}
    </div>
  {/if}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={root} class="flex h-full min-h-[420px] gap-3" onkeydown={onF6}>
  {#if showPane}
    {#if railed}
      <div class="bg-card flex w-9 shrink-0 flex-col items-center rounded-lg border py-1.5">
        <Button variant="ghost" size="icon-sm" aria-label="Show groups" title="Show groups" onclick={() => (railed = false)}>
          <PanelLeftOpen aria-hidden="true" />
        </Button>
        <ListTree class="text-muted-foreground mt-2 size-4" aria-hidden="true" />
        {#if treeSelected.length}<span class="bg-primary mt-1 size-1.5 rounded-full" title="Groups selected" aria-hidden="true"></span>{/if}
      </div>
    {:else}
      <aside class="bg-card flex w-60 shrink-0 flex-col overflow-hidden rounded-lg border" aria-label="Groups">
        <div class="flex h-9 items-center gap-1 border-b pr-1 pl-3">
          <h2 class="eyebrow text-muted-foreground flex-1">{groupBy === 'manufacturer' ? 'Manufacturers' : groupBy === 'size' ? 'Sizes' : 'Groups'}</h2>
          <Button variant="ghost" size="icon-xs" aria-label="Hide groups" title="Hide groups" onclick={() => (railed = true)}>
            <PanelLeftClose aria-hidden="true" />
          </Button>
        </div>
        {@render approx()}
        <GroupTree nodes={groupNodes} total={treeTotal} selected={treeSelected} onselect={onTreeSelect} bind:open={prefs.open} label="Groups" {allLabel} class="flex-1 px-1" />
      </aside>
    {/if}
  {/if}
<div class="flex min-w-0 flex-1 flex-col gap-3">
  <div class="flex flex-wrap items-center gap-x-3 gap-y-2" role="search" aria-label="Filter strings">
    <div class="relative w-full max-w-sm min-w-56 flex-1">
      <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
      <Input type="search" aria-label="Search keys and text" placeholder="Search keys and text" class="h-8 pl-8" bind:value={search} />
    </div>
    <SelectField ariaLabel="Group by" options={groupBys} value={groupBy} onchange={(v) => (prefs.groupBy = v)} class="min-w-44" />
    {#if narrow && groupBy !== 'none'}
      <div class="relative" bind:this={popEl}>
        <Button variant="outline" size="sm" aria-expanded={pop} aria-controls="groups-pop" onclick={togglePop}>
          <ListTree aria-hidden="true" /> Groups{treeSelected.length ? ` (${treeSelected.length})` : ''}
        </Button>
        {#if pop}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            id="groups-pop"
            role="dialog"
            aria-label="Groups"
            tabindex="-1"
            class="bg-popover text-popover-foreground absolute top-full z-30 {popRight ? 'right-0' : 'left-0'} mt-1 flex max-h-[min(60vh,480px)] w-72 flex-col rounded-lg border p-1 shadow-lg"
            onkeydown={(e) => {
              if (e.key === 'Escape' && !e.defaultPrevented) {
                e.stopPropagation();
                pop = false;
              }
            }}
          >
            {@render approx()}
            <GroupTree nodes={groupNodes} total={treeTotal} selected={treeSelected} onselect={onTreeSelect} bind:open={prefs.open} label="Groups" {allLabel} class="flex-1" />
          </div>
        {/if}
      </div>
    {/if}
    <SelectField ariaLabel="Sort" options={sorts} value={effSort} onchange={(v) => (sort = v)} />
    <SwitchRow id="changed-only" size="sm" label="Changed only" checked={changedOnly} onchange={(v) => (changedOnly = v)} class="py-0" />
  </div>
  <div class="flex flex-wrap items-center gap-2">
    <span class="text-muted-foreground text-[12px]" id="src-label">Source</span>
    <div role="group" aria-labelledby="src-label" class="flex flex-wrap gap-1.5">
      {#each SRC as s (s)}
        <button
          type="button"
          aria-pressed={sources.includes(s)}
          class={cn(
            'focus-visible:ring-ring/60 inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-[12px] outline-none focus-visible:ring-2',
            sources.includes(s) ? 'border-primary bg-primary/10 text-foreground' : 'text-muted-foreground hover:text-foreground',
          )}
          onclick={() => toggleSource(s)}
        >
          {#if sources.includes(s)}<Check class="text-primary size-3.5" aria-hidden="true" />{/if}
          {SOURCE_LABEL[s]} <span class="mono text-[11px]">{int(facetCount(s))}</span>
        </button>
      {/each}
    </div>
    {#if chipList.length}
      <span class="bg-border mx-1 h-4 w-px" aria-hidden="true"></span>
      <div role="group" aria-label="Group filters" class="flex flex-wrap items-center gap-1.5">
        {#each chipList as c (c.dim + c.id)}
          <span class="border-primary bg-primary/10 inline-flex h-7 items-center gap-1 rounded-full border pr-1 pl-2.5 text-[12px]">
            {c.label}
            <button
              type="button"
              data-chip={c.dim + ':' + c.id}
              aria-label="Remove filter {c.label}"
              class="text-muted-foreground hover:text-foreground focus-visible:ring-ring/60 grid size-5 place-items-center rounded-full outline-none focus-visible:ring-2"
              onclick={() => removeChip(c.dim, c.id)}
            >
              <X class="size-3" aria-hidden="true" />
            </button>
          </span>
        {/each}
        <Button variant="ghost" size="xs" onclick={clearGroups}>Clear</Button>
      </div>
    {/if}
    <span class="flex-1"></span>
    <span class="text-muted-foreground text-[12px]" aria-live="polite">
      {total === null ? 'Loading…' : `${int(total)} of ${int(all)} keys`}
    </span>
  </div>

  {#if error}
    <ErrorState title="Could not read the strings" {error} onretry={() => app.refresh()} />
  {:else}
    <div
      role="grid"
      aria-label="Strings"
      aria-rowcount={(total ?? 0) + 1}
      aria-colcount={colCount}
      bind:this={grid}
      tabindex="-1"
      class="bg-card flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border"
      onkeydown={onKey}
    >
      <div role="rowgroup" class="border-b">
        <div role="row" aria-rowindex={1} class={cn(cols, 'text-muted-foreground h-8 items-center gap-3 px-3 text-[12px] font-medium')}>
          <span role="columnheader">Key</span>
          {#if showOriginal}<span role="columnheader">Original</span>{/if}
          <span role="columnheader">Result</span>
          <span role="columnheader">Source</span>
          {#if showGroup}<span role="columnheader">Group</span>{/if}
        </div>
      </div>
      <div bind:this={scroller} role="rowgroup" class="relative min-h-0 flex-1 overflow-y-auto" onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}>
        {#if total === null}
          <div class="grid gap-1 p-3">{#each { length: 8 } as _, i (i)}<Skeleton class="h-6 w-full" />{/each}</div>
        {:else if total === 0}
          <p class="text-muted-foreground py-10 text-center">No keys match. Clear the search or the filters.</p>
        {:else}
          <div style="height: {total * ROW}px" class="relative">
            {#each visible as v (v.i)}
              {@const r = v.row}
              <!-- Keys are handled once on the grid (onKey): arrows, Page Up/Down, Home/End, Enter. -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                role="row"
                data-row={v.i}
                aria-rowindex={v.i + 2}
                aria-selected={active === v.i}
                tabindex={tabRow === v.i ? 0 : -1}
                class={cn(
                  cols,
                  'hover:bg-accent/60 focus-visible:ring-ring/60 absolute right-0 left-0 cursor-pointer items-center gap-3 border-b border-border/60 px-3 outline-none focus-visible:ring-2 focus-visible:ring-inset',
                  active === v.i && 'bg-accent/70',
                )}
                style="top: {v.i * ROW}px; height: {ROW}px"
                onclick={() => {
                  active = v.i;
                  if (r) openRow(r.key);
                }}
                onfocus={() => (active = v.i)}
              >
                {#if r}
                  <span role="gridcell" class="mono truncate text-[12px]" title={r.key}>{r.key}</span>
                  {#if showOriginal}<span role="gridcell" class="text-muted-foreground truncate">{r.original}</span>{/if}
                  <span role="gridcell" class="truncate">{r.result}</span>
                  <span role="gridcell"><ToneBadge tone={SOURCE_TONE[r.source]} label={SOURCE_LABEL[r.source]} icon={null} /></span>
                  {#if showGroup}<span role="gridcell" class="text-muted-foreground truncate text-[12px]" title={r.group_label}>{r.group_label || (CATEGORY_LABEL[r.category] ?? r.category)}</span>{/if}
                {:else}
                  <span role="gridcell" class="col-span-full"><Skeleton class="h-4 w-2/3" /></span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
    <p class="text-muted-foreground text-[11.5px]">Arrow keys move, Enter opens a key. Edits are saved per channel and written on the next apply.</p>
  {/if}
</div>

</div>

<StringDetailSheet bind:open={sheet} target={target} rowKey={openKey} onchanged={patchRow} />
