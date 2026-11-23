<script lang="ts">
  /**
   * Loot: what drops where, from the channel's cached loot data. Two views: by area (caves,
   * derelicts, ...: what can drop there) and by item (every area it drops in), each likeliest
   * first with rough odds per spawned box. Each list is a collapsible tree built here from the
   * rows' paths (kind of place or category -> a middle level -> the row), collapsed until
   * opened (remembered per viewer) and open wherever a search matches; it is windowed, so only
   * the visible rows render. Lazy: nothing loads before the screen opens; the list call returns
   * summaries only and a row's drops load when it is picked (stale answers are ignored). The
   * drops come in groups ("Group by", remembered per view: where in it, place, or none): the
   * answer holds every group's header (count and best odds over all the drops, after the
   * server-side filter) and the rows of the first few only; opening a group loads its rows
   * (capped per group). Same-named rows show their class name. The backend builds the index
   * once per build.
   */
  import { tick as nextTick, untrack } from 'svelte';
  import PackageOpen from '@lucide/svelte/icons/package-open';
  import Search from '@lucide/svelte/icons/search';
  import ListFilter from '@lucide/svelte/icons/list-filter';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import DatabaseZap from '@lucide/svelte/icons/database-zap';
  import Info from '@lucide/svelte/icons/info';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import SelectField from '@/components/common/SelectField.svelte';
  import Segmented from '@/components/common/Segmented.svelte';
  import VirtualList from '@/components/common/VirtualList.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { LootDetail, LootDrop, LootGroupBy, LootList, LootSummary, LootView } from '@/lib/contract/types';
  import { dropWhere, groupsOf, listed, oddsText } from '@/lib/loot';
  import { ancestorsOf, leafId, openKey, parseOpen, searching, toggled, visibleTree, whereText, type TreeRow } from '@/lib/loottree';
  import { dropRows, dupNames, groupByOptions, loadGroupBy, loadedRows, missingGroups, saveGroupBy, type DropRow } from '@/lib/lootgroups';
  import { keyAction } from '@/lib/grouptree';
  import { guard } from '@/lib/loadstate';
  import { int } from '@/lib/format';
  import { toast } from '@/lib/stores/toast.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { dev } from '@/lib/stores/dev.svelte';
  import { cn } from '@/lib/utils';

  const VIEW_KEY = 'starorg.loot.view';
  function loadView(): LootView {
    try {
      return localStorage.getItem(VIEW_KEY) === 'items' ? 'items' : 'areas';
    } catch {
      return 'areas';
    }
  }
  function saveView(v: LootView) {
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      /* storage blocked: the view still switches for this session */
    }
  }
  function loadOpen(v: LootView): Set<string> {
    try {
      return new Set(parseOpen(localStorage.getItem(openKey(v))));
    } catch {
      return new Set();
    }
  }
  function saveOpen(v: LootView, ids: ReadonlySet<string>) {
    try {
      localStorage.setItem(openKey(v), JSON.stringify([...ids]));
    } catch {
      /* storage blocked: the tree still opens for this session */
    }
  }

  let view = $state<LootView>(loadView());
  let tick = $state(0);
  // Each view's list with the stamp (channel, refresh, retry) it was read under: switching views reuses it.
  let lists = $state.raw<Record<LootView, { stamp: string; list: LootList } | null>>({ areas: null, items: null });
  let listError = $state<ContractError | null>(null);
  let query = $state('');
  let group = $state('');
  let picked = $state<Record<LootView, string | null>>({ areas: null, items: null });
  // Expanded branches per view (remembered per viewer), and those closed during the current search.
  let open = $state.raw<Record<LootView, Set<string>>>({ areas: loadOpen('areas'), items: loadOpen('items') });
  let closedInSearch = $state.raw<Set<string>>(new Set());
  // The tree row with the keyboard focus, and a counter that asks the list to scroll to it.
  let focusId = $state<string | null>(null);
  let revealSeq = $state(0);
  // A row to open and show once its view's list is there (a jump from the drops table).
  let pendingReveal = $state<string | null>(null);

  // The drops filter: typed, and the value sent (after a pause).
  let dropFilter = $state('');
  let sentFilter = $state('');
  // How each view's drops are grouped (remembered per viewer).
  let groupBy = $state<Record<LootView, LootGroupBy>>({ areas: loadGroupBy('areas'), items: loadGroupBy('items') });
  // The last detail answer (its group headers) with what it was asked for; `key` names the ask.
  let detail = $state.raw<{ key: string; view: LootView; id: string; filter: string; by: LootGroupBy; d: LootDetail } | null>(null);
  let detailError = $state<ContractError | null>(null);
  let detailLoading = $state(false);
  const detailGuard = guard();
  // The answer's open groups, the rows loaded per group and the groups loading, all of `detail`.
  let openGroups = $state.raw<Set<string>>(new Set());
  let groupRows = $state.raw<Record<string, LootDrop[]>>({});
  let loadingGroups = $state.raw<Set<string>>(new Set());
  // A new detail answer starts a new epoch: a group's rows that come back after it are dropped.
  const groupEpoch = guard();
  let groupLive: () => boolean = () => false;

  const devOn = $derived(!!app.settings?.advanced.developer_mode);
  const stamp = $derived(`${app.targetKey}|${app.rev}|${tick}`);
  const list = $derived(lists[view]?.stamp === stamp ? lists[view]!.list : null);
  const hasList = $derived(list !== null);
  const ready = $derived(list?.state === 'ready');

  // Keyed on primitives only: the view and the stamp (channel key, refresh counter, Reload).
  $effect(() => {
    const v = view;
    const s = stamp;
    if (untrack(() => lists[v]?.stamp === s)) return;
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    listError = null;
    api
      .lootList(app.targetKey ? target : null, v)
      .then((l) => {
        if (stale) return;
        lists = { ...lists, [v]: { stamp: s, list: l } };
      })
      .catch((e) => !stale && (listError = toContractError(e)));
    return () => (stale = true);
  });

  const tree = $derived(visibleTree(list?.rows, query, group, open[view], closedInSearch));
  const groups = $derived([{ id: '', label: view === 'areas' ? 'All places' : 'All categories' }, ...groupsOf(list?.rows).map((g) => ({ id: g.group, label: `${g.group} (${g.count})` }))]);
  // The picked row while it is listed (a refresh can drop it).
  const current = $derived(listed(Array.isArray(list?.rows) ? list.rows : [], picked[view]));
  const currentRow = $derived<LootSummary | null>(current === null ? null : (list?.rows.find((r) => r.id === current) ?? null));
  // The row that takes Tab into the tree: the focused one, else the pick, else the first.
  const tabId = $derived.by(() => {
    const rows = tree.rows;
    if (focusId !== null && rows.some((r) => r.id === focusId)) return focusId;
    if (current !== null && rows.some((r) => r.id === leafId(current))) return leafId(current);
    return rows[0]?.id ?? '';
  });
  const revealIndex = $derived(tree.rows.findIndex((r) => r.id === tabId));

  // A new search starts with every branch open again.
  $effect(() => {
    void query;
    closedInSearch = new Set();
  });

  // Keyed on primitives only: the row to show, the view and whether its list is there.
  $effect(() => {
    const id = pendingReveal;
    const v = view;
    if (id === null || !hasList) return;
    untrack(() => {
      pendingReveal = null;
      const row = lists[v]?.list.rows.find((r) => r.id === id);
      if (!row) return;
      const up = ancestorsOf(row);
      if (up.some((a) => !open[v].has(a))) setOpen(v, new Set([...open[v], ...up]));
      // Keyboard focus follows the jump to the revealed row.
      void focusRow(leafId(id));
    });
  });

  // The typed filter is sent after a short pause.
  $effect(() => {
    const f = dropFilter.trim();
    const t = setTimeout(() => (sentFilter = f), 220);
    return () => clearTimeout(t);
  });

  const by = $derived(groupBy[view]);

  // Keyed on primitives only: the view, the picked id, the sent filter, the grouping, readiness
  // and the stamp. A newer pick, filter or grouping makes the older request stale, so a slow
  // answer never replaces a newer one. The answer holds every group's header and the rows of
  // the groups it opened (the first few, or a filter's matches).
  $effect(() => {
    const v = view;
    const id = current;
    const f = sentFilter;
    const g = by;
    const s = stamp;
    if (id === null || !ready) {
      detailGuard.drop();
      groupEpoch.drop();
      detail = null;
      detailLoading = false;
      return;
    }
    const fresh = detailGuard.next();
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    detailLoading = true;
    detailError = null;
    api
      .lootDetail(target, v, id, f || null, g)
      .then((d) => {
        if (!fresh()) return;
        const rows = loadedRows(d.groups);
        groupLive = groupEpoch.next();
        groupRows = rows;
        openGroups = new Set(Object.keys(rows));
        loadingGroups = new Set();
        detail = { key: `${v}|${id}|${f}|${g}|${s}`, view: v, id, filter: f, by: g, d };
      })
      .catch((e) => fresh() && (detailError = toContractError(e)))
      .finally(() => fresh() && (detailLoading = false));
    return () => detailGuard.drop();
  });

  // The open groups without rows yet, as one primitive (what to load next).
  const missing = $derived(missingGroups(openGroups, groupRows, loadingGroups).join('\u001f'));

  // Keyed on primitives only: the answer and the open groups still to load. Each opened group's
  // rows load once; several loads may run together, and all of them are dropped when a newer
  // answer arrives (another pick, filter or grouping).
  $effect(() => {
    const keys = missing;
    const k = detail?.key ?? '';
    if (!keys || !k) return;
    untrack(() => {
      const at = detail;
      if (!at) return;
      const want = keys.split('\u001f');
      const live = groupLive;
      loadingGroups = new Set([...loadingGroups, ...want]);
      const target = app.target ? $state.snapshot(app.target) : null;
      const settle = (got: Record<string, LootDrop[]>) => {
        if (!live()) return;
        // A group the answer does not hold any more loads as empty, so it is never asked again.
        const next = { ...groupRows };
        for (const w of want) next[w] = got[w] ?? [];
        groupRows = next;
        loadingGroups = new Set([...loadingGroups].filter((x) => !want.includes(x)));
      };
      api
        .lootDetail(target, at.view, at.id, at.filter || null, at.by, want)
        .then((d) => settle(loadedRows(d.groups)))
        .catch((e) => {
          if (!live()) return;
          // Close the groups again without rows, so opening one asks once more.
          openGroups = new Set([...openGroups].filter((x) => !want.includes(x)));
          loadingGroups = new Set([...loadingGroups].filter((x) => !want.includes(x)));
          toast('error', 'Could not read the drops', toContractError(e).message);
        });
    });
  });

  // The answer for the current pick (an older filter's or grouping's while a newer one loads, never another row's).
  const shownDetail = $derived(detail && detail.view === view && detail.id === current ? detail.d : null);
  // An answer that knows nothing of the row (the data went missing, or a refresh dropped it).
  const emptyAnswer = $derived(!!shownDetail && (shownDetail.state !== 'ready' || shownDetail.entry === null));
  const flat = $derived(shownDetail?.group_by === 'none');
  const drawn = $derived<DropRow[]>(shownDetail ? dropRows(shownDetail.groups, openGroups, groupRows, flat) : []);
  // Names more than one loaded row carries: those rows show their id (class name) too.
  const dups = $derived(dupNames(drawn.flatMap((r) => (r.kind === 'drop' ? [r.drop] : []))));
  // Ungrouped: the one group's rows sent and its size (the footer says when it is capped).
  const flatShown = $derived(flat ? (groupRows[shownDetail?.groups[0]?.key ?? ''] ?? []).length : 0);

  function setGroupBy(g: LootGroupBy) {
    groupBy = { ...groupBy, [view]: g };
    saveGroupBy(view, g);
  }
  function toggleGroup(key: string) {
    openGroups = toggled(openGroups, key, !openGroups.has(key));
  }

  function setView(v: LootView) {
    view = v;
    query = '';
    group = '';
    focusId = null;
    resetDropFilter();
    saveView(v);
  }
  function resetDropFilter() {
    dropFilter = '';
    sentFilter = '';
  }
  function pick(id: string) {
    if (picked[view] !== id) resetDropFilter();
    picked = { ...picked, [view]: id };
    focusId = leafId(id);
  }
  function setOpen(v: LootView, ids: Set<string>) {
    open = { ...open, [v]: ids };
    saveOpen(v, ids);
  }
  /** Open or close a branch: while searching only for this search, else remembered. */
  function setBranch(id: string, on: boolean) {
    if (searching(query)) closedInSearch = toggled(closedInSearch, id, !on);
    else setOpen(view, toggled(open[view], id, on));
  }
  /** Jump to the other view with that row picked, its branches opened and in view. */
  function jump(id: string) {
    const other: LootView = view === 'areas' ? 'items' : 'areas';
    picked = { ...picked, [other]: id };
    view = other;
    group = '';
    query = '';
    resetDropFilter();
    pendingReveal = id;
    saveView(other);
  }
  function openRecord(guid: string) {
    dev.openRecord(guid, () => app.go('explorer'));
  }

  let treeEl = $state<HTMLElement | null>(null);
  let typed = '';
  let typedAt = 0;
  async function focusRow(id: string) {
    focusId = id;
    revealSeq++;
    await nextTick();
    // The windowed list draws the row once it has scrolled to it: wait a frame or two for it.
    for (let i = 0; i < 4; i++) {
      if (focusId !== id) return;
      const el = treeEl?.querySelector<HTMLElement>(`[data-tree-id="${CSS.escape(id)}"]`);
      if (el) return el.focus();
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    }
  }
  function activate(r: TreeRow) {
    if (r.node.kind === 'branch') setBranch(r.id, !r.expanded);
    else pick(r.node.row.id);
  }
  function onTreeKey(e: KeyboardEvent) {
    if (e.altKey || e.metaKey) return;
    const rows = tree.rows;
    if (!rows.length) return;
    let buf = '';
    if (e.key.length === 1 && e.key !== ' ' && !e.ctrlKey) {
      const now = Date.now();
      typed = now - typedAt > 700 ? e.key : typed + e.key;
      typedAt = now;
      buf = typed;
    }
    const a = keyAction(rows, tabId, e.key, e.ctrlKey, buf);
    switch (a.kind) {
      case 'focus':
        void focusRow(a.id);
        break;
      case 'expand':
      case 'collapse':
        setBranch(a.id, a.kind === 'expand');
        break;
      case 'select-only':
      case 'toggle': {
        const r = rows.find((x) => x.id === a.id);
        if (r) activate(r);
        break;
      }
      default:
        return;
    }
    e.preventDefault();
  }

  // "Read game data" (a JobKind::Extract job): the Where to get datasets the loot index is built from.
  let reading = $state(false);
  async function readGameData() {
    if (!app.target) return;
    reading = true;
    try {
      await api.readGameData($state.snapshot(app.target));
      await app.refresh();
    } catch (e) {
      toast('error', 'Could not read the game data', toContractError(e).message);
    } finally {
      reading = false;
    }
  }

  const unit = (n: number, one: string, many: string) => `${int(n)} ${n === 1 ? one : many}`;
  const LROW = 34;
  const DROW = 40;
  const DGRID = 'grid grid-cols-[minmax(0,1fr)_minmax(0,0.9fr)_5.5rem] items-center gap-3';
</script>

{#if !app.target}
  <FullEmpty icon={PackageOpen} title="No channel yet">Loot lists come from a channel's game data. Set up a channel on Home first.</FullEmpty>
{:else if listError}
  <ErrorState title="Could not read the loot data" error={listError} onretry={() => tick++} />
{:else if list && list.state === 'missing'}
  <FullEmpty icon={PackageOpen} title="No loot data yet">
    The loot lists are built from the game data of {app.target.channel} (loot tables and where they spawn). Nothing is written to the game; the first read of a build takes a minute or two.
    {#snippet actions()}
      <Button disabled={reading || !app.channel?.set_up} onclick={readGameData}>
        <DatabaseZap class={cn(reading && 'animate-pulse motion-reduce:animate-none')} aria-hidden="true" />
        {reading ? 'Reading game data…' : 'Read game data'}
      </Button>
    {/snippet}
  </FullEmpty>
{:else}
  <div class="grid gap-3">
    <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px]" role="note">
      <Info class="size-3.5 shrink-0" aria-hidden="true" />
      <span>Odds are rough, per spawned box: the chance one crate, rack or loose spot of that kind holds the item, assuming an even pick inside each loot pool.</span>
      {#if list?.build}<span aria-hidden="true">·</span><span>{list.build}</span>{/if}
    </p>

    <div class="grid items-start gap-3 lg:grid-cols-[minmax(15rem,22rem)_minmax(0,1fr)]">
      <section class="bg-card flex min-h-0 flex-col rounded-lg border" aria-labelledby="loot-list-title">
        <h2 id="loot-list-title" class="sr-only">{view === 'areas' ? 'Loot areas' : 'Items in loot'}</h2>
        <div class="grid gap-2 border-b p-2.5">
          <Segmented
            ariaLabel="List"
            size="sm"
            options={[
              { id: 'areas', label: 'By area', hint: 'Caves, derelicts, outposts: what can drop there' },
              { id: 'items', label: 'By item', hint: 'Every item that drops in an area: where' },
            ]}
            value={view}
            onchange={(v) => setView(v as LootView)}
          />
          <div class="relative">
            <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
            <Input
              type="search"
              aria-label={view === 'areas' ? 'Search areas' : 'Search items'}
              placeholder={view === 'areas' ? 'Search areas' : 'Search items'}
              class="h-8 pl-8"
              value={query}
              oninput={(e) => (query = (e.currentTarget as HTMLInputElement).value)}
              onkeydown={(e) => e.key === 'Escape' && query && (e.preventDefault(), (query = ''))}
            />
          </div>
          <div class="flex items-center gap-2 text-[12px]">
            <SelectField ariaLabel={view === 'areas' ? 'Kind of place' : 'Category'} options={groups} value={group} onchange={(v) => (group = v)} class="min-w-36 flex-1" />
            {#if list}<span class="text-muted-foreground shrink-0" aria-live="polite">{int(tree.matched)} of {int(list.rows.length)}</span>{/if}
          </div>
        </div>
        {#if !list}
          <div class="grid gap-1.5 p-2.5" aria-busy="true" aria-label="Loading">
            {#each Array.from({ length: 10 }, (_, i) => i) as i (i)}<Skeleton class="h-7 w-full" />{/each}
          </div>
        {:else if !tree.rows.length}
          <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">{list.rows.length ? `Nothing matches${query ? ` "${query}"` : ''}.` : view === 'areas' ? 'No loot area in this build.' : 'No item drops in a loot area in this build.'}</p>
        {:else}
          <div bind:this={treeEl} class="contents">
            <VirtualList
              items={tree.rows}
              rowHeight={LROW}
              role="tree"
              label={view === 'areas' ? 'Loot areas by kind of place' : 'Items that drop in loot, by category'}
              resetKey={`${view}|${query}|${group}`}
              reveal={revealIndex}
              revealKey={revealSeq}
              key={(r) => r.id}
              onkeydown={onTreeKey}
              class="h-[calc(100vh-19rem)] min-h-64 py-1"
            >
              {#snippet row(r)}
                {@const leaf = r.node.kind === 'leaf' ? r.node.row : null}
                {@const branch = r.node.kind === 'branch' ? r.node : null}
                {@const on = leaf !== null && leaf.id === current}
                <!-- Keys are handled once on the tree (onTreeKey). -->
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  role="treeitem"
                  data-tree-id={r.id}
                  aria-level={r.level}
                  aria-setsize={r.size}
                  aria-posinset={r.pos}
                  aria-expanded={branch ? r.expanded : undefined}
                  aria-selected={leaf ? on : undefined}
                  aria-label={branch ? `${r.label}, ${unit(branch.count, view === 'areas' ? 'area' : 'item', view === 'areas' ? 'areas' : 'items')}` : undefined}
                  tabindex={r.id === tabId ? 0 : -1}
                  class={cn(
                    'focus-visible:ring-ring/60 flex h-full cursor-pointer items-center gap-1.5 pr-3 outline-none select-none focus-visible:ring-2 focus-visible:ring-inset',
                    on ? 'bg-primary/15 text-foreground' : 'hover:bg-muted',
                  )}
                  style="padding-left: {0.375 + (r.level - 1) * 1}rem"
                  onclick={() => {
                    focusId = r.id;
                    activate(r);
                  }}
                  onfocus={() => (focusId = r.id)}
                >
                  {#if branch}
                    <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', r.expanded && 'rotate-90')} aria-hidden="true" />
                    <span class={cn('min-w-0 flex-1 truncate text-[13px]', r.level === 1 && 'font-medium')} title={r.label}>{r.label}</span>
                    <span class="mono text-muted-foreground shrink-0 text-[11px]" title={unit(branch.count, view === 'areas' ? 'area' : 'item', view === 'areas' ? 'areas' : 'items')}>{int(branch.count)}</span>
                    <span class="mono text-muted-foreground w-12 shrink-0 text-right text-[11px]" title="Best rough odds per box below">{oddsText(branch.best)}</span>
                  {:else if leaf}
                    <span class="size-3.5 shrink-0" aria-hidden="true"></span>
                    {#if r.node.kind === 'leaf' && r.node.clash}
                      <span class="flex min-w-0 flex-1 items-baseline gap-1.5" title="{leaf.name} ({leaf.id})">
                        <span class={cn('min-w-0 shrink truncate text-[13px]', on && 'font-medium')}>{leaf.name}</span>
                        <span class="mono text-muted-foreground min-w-0 shrink-[2] truncate text-[10.5px]">{leaf.id}</span>
                      </span>
                    {:else}
                      <span class={cn('min-w-0 flex-1 truncate text-[13px]', on && 'font-medium')} title={leaf.name}>{leaf.name}</span>
                    {/if}
                    <span class="text-muted-foreground shrink-0 text-[11px]">{view === 'areas' ? unit(leaf.count, 'item', 'items') : unit(leaf.count, 'area', 'areas')}</span>
                    <span class="mono text-muted-foreground w-12 shrink-0 text-right text-[11px]" title="Best rough odds per box">{oddsText(leaf.best)}</span>
                  {/if}
                </div>
              {/snippet}
            </VirtualList>
          </div>
        {/if}
      </section>

      <section class="bg-card grid min-w-0 gap-3 rounded-lg border p-3" aria-labelledby="loot-title" aria-busy={detailLoading}>
        {#if !currentRow}
          <p class="text-muted-foreground py-6 text-center">{view === 'areas' ? 'Pick an area to see what can drop there.' : 'Pick an item to see where it drops.'}</p>
        {:else}
          <header class="flex flex-wrap items-start gap-x-3 gap-y-1">
            <div class="grid min-w-0 flex-1 gap-0.5">
              <p class="text-muted-foreground truncate text-[11.5px]">{whereText(currentRow)}</p>
              <h2 id="loot-title" class="text-[16px] font-semibold">{currentRow.name}</h2>
              <p class="text-muted-foreground text-[12px]">
                {view === 'areas' ? `${unit(currentRow.count, 'item', 'items')} can drop here` : `drops in ${unit(currentRow.count, 'area', 'areas')}`} · best {oddsText(currentRow.best)} per box
                <span class="mono block truncate text-[11px]" title={currentRow.id}>{currentRow.id}</span>
              </p>
            </div>
            {#if view === 'items' && shownDetail?.record}
              {@const rec = shownDetail.record}
              {#if devOn}
                <Button variant="outline" size="sm" onclick={() => openRecord(rec.guid)} title="Open {rec.class_name} in the Explorer">
                  <ExternalLink aria-hidden="true" /> Open in Explorer
                </Button>
              {:else}
                <span class="text-muted-foreground text-[11.5px]" title="Turn on Developer mode in Settings -> Advanced to open records in the Explorer">{rec.class_name}</span>
              {/if}
            {/if}
          </header>
          {#if detailError}
            <ErrorState title="Could not read the drops" error={detailError} onretry={() => tick++} />
          {:else if emptyAnswer}
            <p class="text-muted-foreground py-6 text-center text-[12px]">No drops listed.</p>
          {:else}
            <div class="flex flex-wrap items-center gap-2">
              <div class="relative min-w-48 flex-1">
                <ListFilter class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
                <Input
                  type="search"
                  aria-label={view === 'areas' ? 'Filter the items' : 'Filter the areas'}
                  placeholder={view === 'areas' ? 'Filter items, containers, rarity' : 'Filter areas, containers, rarity'}
                  class="h-8 pl-8"
                  value={dropFilter}
                  oninput={(e) => (dropFilter = (e.currentTarget as HTMLInputElement).value)}
                  onkeydown={(e) => e.key === 'Escape' && dropFilter && (e.preventDefault(), resetDropFilter())}
                />
              </div>
              <div class="flex items-center gap-1.5 text-[12px]">
                <span class="text-muted-foreground" aria-hidden="true">Group by</span>
                <Segmented ariaLabel="Group the drops by" size="sm" options={groupByOptions(view)} value={by} onchange={(g) => setGroupBy(g)} />
              </div>
            </div>
            <div role="table" aria-label={view === 'areas' ? `What can drop in ${currentRow.name}` : `Where ${currentRow.name} drops`} class="text-[12.5px]">
              <div role="rowgroup">
                <div role="row" class={cn(DGRID, 'text-muted-foreground border-b px-1 py-1 text-[11px]')}>
                  <span role="columnheader">{view === 'areas' ? 'Item' : 'Area'}</span>
                  <span role="columnheader">Where in it</span>
                  <span role="columnheader" class="text-right" title="Rough odds one spawned box holds the item">Per box</span>
                </div>
              </div>
              {#if !shownDetail}
                <div role="rowgroup" class="grid gap-1.5 pt-2" aria-label="Loading">
                  {#each Array.from({ length: Math.min(8, Math.max(3, currentRow.count)) }, (_, i) => i) as i (i)}<Skeleton class="h-7 w-full" />{/each}
                </div>
              {:else if !shownDetail.groups.length}
                <p class="text-muted-foreground py-6 text-center text-[12px]">{detail?.filter ? `No drop matches "${detail.filter}".` : 'No drops listed.'}</p>
              {:else}
                <VirtualList
                  items={drawn}
                  rowHeight={DROW}
                  role="rowgroup"
                  label={flat ? 'Drops, likeliest first' : `Drops in ${int(shownDetail.groups.length)} groups, best odds first`}
                  resetKey={`${view}|${current}|${detail?.filter ?? ''}|${detail?.by ?? ''}`}
                  key={(r) => r.id}
                  class="max-h-[calc(100vh-24rem)] min-h-48"
                >
                  {#snippet row(r)}
                    {#if r.kind === 'head'}
                      {@const g = r.group}
                      <div role="row" class="border-border/50 bg-muted/40 h-full border-b">
                        <span role="cell" class="block h-full">
                          <button
                            type="button"
                            class={cn(DGRID, 'hover:bg-muted focus-visible:ring-ring/60 h-full w-full px-1 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset')}
                            aria-expanded={r.open}
                            aria-label="{g.label}, {unit(g.count, view === 'areas' ? 'item' : 'area', view === 'areas' ? 'items' : 'areas')}, best {oddsText(g.best)} per box"
                            onclick={() => toggleGroup(g.key)}
                          >
                            <span class="flex min-w-0 items-center gap-1.5 [grid-column:span_2]">
                              <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', r.open && 'rotate-90')} aria-hidden="true" />
                              <span class="min-w-0 truncate font-medium" title={g.label}>{g.label}</span>
                              <span class="text-muted-foreground shrink-0 text-[11px]">{unit(g.count, view === 'areas' ? 'item' : 'area', view === 'areas' ? 'items' : 'areas')}</span>
                            </span>
                            <span class="text-muted-foreground text-right text-[11px] tabular-nums" title="Best rough odds per box in this group">best {oddsText(g.best)}</span>
                          </button>
                        </span>
                      </div>
                    {:else if r.kind === 'drop'}
                      {@const d = r.drop}
                      {@const twin = dups.has(d.name)}
                      <div role="row" class={cn(DGRID, 'border-border/50 h-full border-b px-1', !flat && 'pl-6')}>
                        <span role="cell" class="grid min-w-0">
                          <button
                            type="button"
                            class="text-primary focus-visible:ring-ring/60 truncate rounded-sm text-left underline-offset-2 outline-none hover:underline focus-visible:ring-2"
                            title={view === 'areas' ? `Where else ${d.name} drops${twin ? ` (${d.id})` : ''}` : `What else drops in ${d.name}${twin ? ` (${d.id})` : ''}`}
                            onclick={() => jump(d.id)}>{d.name}</button
                          >
                          <span class="text-muted-foreground truncate text-[11px]" title={twin ? `${d.id} · ${d.group}` : d.group}>
                            {#if twin}<span class="mono">{d.id}</span> · {/if}{d.group}
                          </span>
                        </span>
                        <span role="cell" class="text-muted-foreground truncate" title={dropWhere(d)}>{dropWhere(d)}</span>
                        <span role="cell" class="text-right font-medium tabular-nums">{oddsText(d.odds)}</span>
                      </div>
                    {:else if r.kind === 'more'}
                      <div role="row" class="border-border/50 text-muted-foreground flex h-full items-center border-b pr-1 pl-6 text-[11.5px]">
                        <span role="cell">{int(r.group.count - r.shown)} more in {r.group.label}; filter to narrow.</span>
                      </div>
                    {:else}
                      <div role="row" class="border-border/50 flex h-full items-center border-b pr-1 pl-6" aria-busy="true">
                        <span role="cell" class="w-full"><Skeleton class="h-5 w-2/3" /><span class="sr-only">Loading</span></span>
                      </div>
                    {/if}
                  {/snippet}
                </VirtualList>
              {/if}
              {#if shownDetail && shownDetail.groups.length && (detail?.filter || (flat && shownDetail.total > flatShown))}
                <p class="text-muted-foreground pt-2 text-[11.5px]" aria-live="polite">
                  {#if detail?.filter}
                    {int(shownDetail.total)} of {int(shownDetail.all)} drops match{flat && shownDetail.total > flatShown ? `; the likeliest ${int(flatShown)} shown` : ''}.
                  {:else}
                    The likeliest {int(flatShown)} of {int(shownDetail.total)} shown; filter to reach the rest.
                  {/if}
                </p>
              {/if}
            </div>
          {/if}
        {/if}
      </section>
    </div>
  </div>
{/if}
