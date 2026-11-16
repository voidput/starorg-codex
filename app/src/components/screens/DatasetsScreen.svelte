<script lang="ts">
  /**
   * Datasets (addendum 7.5, 7.8): sources (this build's GameData, local folders of `*.json`,
   * the published dump behind Settings > Network) and their datasets on the left; the picked
   * table as a virtualised grid (pages of 200) with search, column filters (value chips, min/max,
   * "has a value"), sort and a column picker remembered per dataset and table; a detail sheet
   * with the row's JSON tree, its loc text and "Open in Explorer" links. Cells that hold ids
   * (GUIDs, record names, `@key`s, manufacturer codes) show what they read as first, the raw id
   * after it ("Show IDs" switches back, 7.14); search matches both. Everything is parsed by the
   * backend as data; values are shown as text. Between the sources and the grid, a group
   * tree (addendum 7.15) groups the table by a preset (type › subType › size, manufacturer,
   * path folders, ...; remembered per table): picking a node filters the grid.
   */
  import { tick, untrack } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import Trash from '@lucide/svelte/icons/trash-2';
  import Columns from '@lucide/svelte/icons/columns-3';
  import Funnel from '@lucide/svelte/icons/funnel';
  import X from '@lucide/svelte/icons/x';
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right';
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import Sheet_ from '@lucide/svelte/icons/sheet';
  import ListTree from '@lucide/svelte/icons/list-tree';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open';
  import DataTree from '@/components/dev/DataTree.svelte';
  import { NONE, groupByKey, levelsOf, pickGroupBy, readPref, writePref } from '@/lib/datatree';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Skeleton } from '@/components/ui/skeleton';
  import SelectField from '@/components/common/SelectField.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import Sheet from '@/components/common/Sheet.svelte';
  import JsonTree from '@/components/dev/JsonTree.svelte';
  import ReadableValue from '@/components/dev/ReadableValue.svelte';
  import ShowIdsToggle from '@/components/dev/ShowIdsToggle.svelte';
  import { devApi, type Column, type DatasetInfo, type DevRowDetail, type DevSource, type GroupPage, type Row, type RowFilter, type RowSort, type TableInfo } from '@/lib/contract/dev';
  import { toContractError, type ContractError } from '@/lib/contract/client';
  import { bytes } from '@/lib/devnav';
  import { app } from '@/lib/stores/app.svelte';
  import { dev } from '@/lib/stores/dev.svelte';
  import { numberCell } from '@/lib/readable';
  import { toast } from '@/lib/stores/toast.svelte';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';

  const ROW = 32;
  const PAGE = 200;
  const OVERSCAN = 8;
  const COLW = 168;

  const target = $derived(app.target);
  const tkey = $derived(app.targetKey);

  let sources = $state<DevSource[]>([]);
  let srcError = $state<ContractError | null>(null);
  let srcLoading = $state(true);
  async function loadSources(f?: () => Promise<DevSource[]>) {
    if (!target) return;
    try {
      sources = await (f ? f() : devApi.sources($state.snapshot(target)));
      srcError = null;
    } catch (e) {
      srcError = toContractError(e);
    } finally {
      srcLoading = false;
    }
  }
  $effect(() => {
    void tkey;
    untrack(() => void loadSources());
  });
  async function addFolder() {
    if (!target) return;
    const before = sources.length;
    await loadSources(() => devApi.addSource($state.snapshot(target!)));
    if (sources.length === before) toast('info', 'No folder added', 'The folder dialog was closed.');
  }
  async function removeSource(id: string) {
    if (!target) return;
    await loadSources(() => devApi.removeSource({ target: $state.snapshot(target!), source: id }));
    if (pick?.source === id) pick = null;
  }

  // --- The picked dataset and table ------------------------------------------------------
  let pick = $state<{ source: string; dataset: string } | null>(null);
  /** The table to show once a hand-off's dataset has loaded (instead of its first table). */
  let wantTable: string | null = null;
  let info = $state<DatasetInfo | null>(null);
  let tableName = $state<string | null>(null);
  let table = $state<TableInfo | null>(null);
  let dsError = $state<ContractError | null>(null);
  $effect(() => {
    const p = pick;
    untrack(async () => {
      info = null;
      table = null;
      tableName = null;
      if (!p || !target) return;
      try {
        info = await devApi.dataset({ target: $state.snapshot(target), ...p });
        const want = wantTable;
        wantTable = null;
        tableName = want && info.tables.some((t) => t.name === want) ? want : (info.tables[0]?.name ?? null);
        dsError = null;
      } catch (e) {
        dsError = toContractError(e);
      }
    });
  });
  const tkeyTable = $derived(pick && tableName ? `${pick.source}|${pick.dataset}|${tableName}` : '');
  $effect(() => {
    const k = tkeyTable;
    untrack(async () => {
      table = null;
      if (!k || !pick || !tableName || !target) return;
      try {
        table = await devApi.table({ target: $state.snapshot(target), ...pick, table: tableName });
        shownCols = readCols(k, table);
        groupId = pickGroupBy(readPref(groupByKey(pick.dataset, tableName)), table.group_presets);
        node = null;
        open = [];
        filters = [];
        sort = null;
        search = '';
        debounced = '';
      } catch (e) {
        dsError = toContractError(e);
      }
    });
  });
  const tableOptions = $derived(
    (info?.tables ?? []).map((t) => ({ id: t.name, label: `${t.kind === 'index' ? 'Index · ' : t.kind === 'tree' ? 'JSON · ' : ''}${t.name} (${int(t.rows)})` })),
  );

  // --- Columns (remembered per dataset + table in local storage) ----------------------------
  let shownCols = $state<string[]>([]);
  const COLS = (k: string) => `starorg.dev.columns.${k}`;
  function readCols(k: string, t: TableInfo): string[] {
    try {
      const v = JSON.parse(localStorage.getItem(COLS(k)) ?? 'null') as unknown;
      if (Array.isArray(v)) {
        const ok = v.filter((c): c is string => typeof c === 'string' && t.columns.some((x) => x.name === c));
        if (ok.length) return ok;
      }
    } catch {
      /* storage blocked or stale */
    }
    return [...t.default_columns];
  }
  $effect(() => {
    const v = JSON.stringify(shownCols);
    if (!tkeyTable || !table) return;
    try {
      localStorage.setItem(COLS(tkeyTable), v);
    } catch {
      /* ignore */
    }
  });
  let colPop = $state(false);
  let colFind = $state('');
  function toggleCol(c: string) {
    shownCols = shownCols.includes(c) ? shownCols.filter((x) => x !== c) : (table?.columns ?? []).map((x) => x.name).filter((x) => x === c || shownCols.includes(x));
  }

  // --- Search, filters, sort, rows ------------------------------------------------------------
  let search = $state('');
  let debounced = $state('');
  $effect(() => {
    const s = search;
    const t = setTimeout(() => (debounced = s.trim()), 200);
    return () => clearTimeout(t);
  });
  let filters = $state<RowFilter[]>([]);
  let sort = $state<RowSort | null>(null);
  let filterPop = $state(false);
  let filterCol = $state<string | null>(null);
  const filterColumn = $derived<Column | null>(table?.columns.find((c) => c.name === filterCol) ?? null);
  function setFilter(column: string, part: Partial<RowFilter>) {
    const cur = filters.find((f) => f.column === column) ?? { column, values: [], min: null, max: null, has: null };
    const next = { ...cur, ...part };
    const empty = !next.values.length && next.min === null && next.max === null && next.has === null;
    filters = [...filters.filter((f) => f.column !== column), ...(empty ? [] : [next])];
  }
  function chipLabel(f: RowFilter) {
    const parts = [];
    if (f.values.length) parts.push(f.values.join(', '));
    if (f.min !== null || f.max !== null) parts.push(`${f.min ?? '…'} – ${f.max ?? '…'}`);
    if (f.has !== null) parts.push(f.has ? 'has a value' : 'empty');
    return `${f.column}: ${parts.join('; ')}`;
  }

  // --- Group tree (addendum 7.15) ----------------------------------------------------------
  const presets = $derived(table?.group_presets ?? []);
  const showTree = $derived(!!table && presets.length > 0);
  let groupId = $state<string>(NONE);
  const levels = $derived(levelsOf(groupId, presets));
  let node = $state<string | null>(null);
  let nodeLabel = $state('');
  let open = $state<string[]>([]);
  let groups = $state<GroupPage | null>(null);
  let groupsLoading = $state(false);
  let railed = $state(readPref('starorg.dev.tree.railed') === '1');
  $effect(() => writePref('starorg.dev.tree.railed', railed ? '1' : '0'));
  const groupOptions = $derived([...presets.map((p) => ({ id: p.id, label: p.label })), { id: NONE, label: 'No grouping' }]);
  function setGroupBy(v: string) {
    groupId = v;
    node = null;
    open = [];
    if (pick && tableName) writePref(groupByKey(pick.dataset, tableName), v);
  }
  function selectNode(id: string | null) {
    node = id;
    nodeLabel = id === null ? '' : (groups?.nodes.find((n) => n.id === id)?.label ?? '');
  }
  let groupSeq = 0;
  const gkey = $derived(JSON.stringify([tkeyTable, !!table, groupId, debounced, filters, shownCols, open]));
  $effect(() => {
    void gkey;
    untrack(async () => {
      const seq = ++groupSeq;
      if (!table || !pick || !tableName || !target || !levels.length) {
        groups = null;
        return;
      }
      groupsLoading = true;
      try {
        const g = await devApi.groups({
          target: $state.snapshot(target),
          ...pick,
          table: tableName,
          by: $state.snapshot(levels),
          open: [...open],
          columns: [...shownCols],
          search: debounced,
          filters: $state.snapshot(filters),
        });
        if (seq === groupSeq) groups = g;
      } catch (e) {
        if (seq === groupSeq) toast('error', 'Could not group the table', toContractError(e).message);
      } finally {
        if (seq === groupSeq) groupsLoading = false;
      }
    });
  });

  let total = $state<number | null>(null);
  let all = $state(0);
  let pageCols = $state<string[]>([]);
  let pages = $state<Record<number, Row[]>>({});
  let rowsError = $state<ContractError | null>(null);
  let gen = 0;
  const loading = new Set<number>();
  const qkey = $derived(JSON.stringify([tkeyTable, !!table, debounced, filters, sort, shownCols, groupId, node]));
  async function loadPage(p: number, g: number) {
    if (loading.has(p) || pages[p] || !pick || !tableName || !target || !table) return;
    loading.add(p);
    try {
      const r = await devApi.rows({
        target: $state.snapshot(target),
        ...pick,
        table: tableName,
        columns: [...shownCols],
        search: debounced,
        filters: $state.snapshot(filters),
        sort: sort ? { ...sort } : null,
        offset: p * PAGE,
        limit: PAGE,
        group: node !== null && levels.length ? { by: $state.snapshot(levels), node } : null,
      });
      if (g !== gen) return;
      pages[p] = r.rows;
      total = r.total;
      all = r.all;
      pageCols = r.columns;
      rowsError = null;
    } catch (e) {
      if (g === gen) rowsError = toContractError(e);
    } finally {
      loading.delete(p);
    }
  }
  $effect(() => {
    void qkey;
    untrack(() => {
      gen++;
      pages = {};
      loading.clear();
      total = null;
      active = 0;
      if (scroller) scroller.scrollTop = 0;
      scrollTop = 0;
      if (table) void loadPage(0, gen);
    });
  });
  let scroller = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(400);
  $effect(() => {
    if (!scroller) return;
    const ro = new ResizeObserver(() => (viewH = scroller!.clientHeight));
    ro.observe(scroller);
    return () => ro.disconnect();
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
    const out: { i: number; row: Row | null }[] = [];
    for (let i = first; i <= last; i++) out.push({ i, row: pages[Math.floor(i / PAGE)]?.[i % PAGE] ?? null });
    return out;
  });
  let active = $state(0);
  const firstShown = $derived(Math.min(Math.max(0, (total ?? 1) - 1), Math.floor(scrollTop / ROW)));
  const tabRow = $derived(active >= first && active <= last ? active : firstShown);
  const gridCols = $derived(`grid-template-columns: repeat(${Math.max(1, pageCols.length)}, minmax(${COLW}px, 1fr))`);
  const colMeta = (name: string) => table?.columns.find((c) => c.name === name);
  async function focusRow(i: number) {
    if (!scroller || !total) return;
    active = Math.max(0, Math.min(total - 1, i));
    const top = active * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + scroller.clientHeight) scroller.scrollTop = top + ROW - scroller.clientHeight;
    scrollTop = scroller.scrollTop;
    await tick();
    scroller.querySelector<HTMLElement>(`[data-row="${active}"]`)?.focus();
  }
  function onGridKey(e: KeyboardEvent) {
    if (total === null || e.altKey || e.ctrlKey || e.metaKey) return;
    const per = Math.max(1, Math.floor(viewH / ROW) - 1);
    const map: Record<string, number> = { ArrowDown: active + 1, ArrowUp: active - 1, PageDown: active + per, PageUp: active - per, Home: 0, End: total - 1 };
    if (e.key in map) {
      e.preventDefault();
      void focusRow(map[e.key]!);
    } else if (e.key === 'Enter') {
      const r = pages[Math.floor(active / PAGE)]?.[active % PAGE];
      if (r) {
        e.preventDefault();
        void openRow(r.id, r.title);
      }
    }
  }
  function toggleSort(c: string) {
    if (!colMeta(c)?.kind || !['text', 'number', 'bool'].includes(colMeta(c)!.kind)) return;
    sort = sort?.column === c ? (sort.desc ? null : { column: c, desc: true }) : { column: c, desc: false };
  }

  // --- Detail ------------------------------------------------------------------------------
  let sheet = $state(false);
  let detail = $state<DevRowDetail | null>(null);
  let detailTitle = $state<string | null>(null);
  async function openRow(row: number, title: string | null = null) {
    if (!pick || !tableName || !target) return;
    try {
      detail = await devApi.row({ target: $state.snapshot(target), ...pick, table: tableName, row });
      detailTitle = title;
      sheet = true;
    } catch (e) {
      toast('error', 'Could not open the row', toContractError(e).message);
    }
  }
  /** Show a row of any source, dataset and table: the grid moves there, the sheet opens. */
  async function goToRow(at: { source: string; dataset: string; table: string; row: number }) {
    if (!target) return;
    if (pick?.source !== at.source || pick?.dataset !== at.dataset) {
      wantTable = at.table;
      pick = { source: at.source, dataset: at.dataset };
    } else tableName = at.table;
    try {
      detail = await devApi.row({ target: $state.snapshot(target), ...at });
      detailTitle = null;
      sheet = true;
    } catch (e) {
      toast('error', 'Could not open the row', toContractError(e).message);
    }
  }
  // Hand-offs from Diff ("Open in Datasets", a linked row there).
  $effect(() => {
    void dev.seq;
    const p = dev.pendingRow;
    if (!p) return;
    untrack(() => {
      dev.pendingRow = null;
      void goToRow(p);
    });
  });

  /**
   * The open row in the loaded comparison, when Diff compares this source: its record and how
   * many fields changed, or that it is the same on both sides (checked before offering the jump).
   */
  let diffAt = $state<{ dataset: string; table: string; id: string; changed: number } | null>(null);
  let diffSame = $state<string | null>(null);
  const srcLabel = (id: string) => sources.find((x) => x.id === id)?.label ?? id;
  $effect(() => {
    const d = detail;
    const p = pick;
    const tn = tableName;
    diffAt = null;
    diffSame = null;
    if (!d || !p || !tn || !target) return;
    let live = true;
    untrack(async () => {
      try {
        const sum = await devApi.diffSummary($state.snapshot(target!));
        if (!live || (sum.a !== p.source && sum.b !== p.source)) return;
        const t = sum.tables.find((x) => x.dataset === p.dataset && x.table === tn);
        if (!t) return;
        let id = `#${d.row}`;
        if (t.identity) {
          const v = (JSON.parse(d.json) as Record<string, unknown>)[t.identity];
          if (v === undefined || v === null) return;
          id = typeof v === 'string' ? v : JSON.stringify(v);
        }
        const at = { dataset: p.dataset, table: tn, id };
        try {
          const f = await devApi.diffRecord({ target: $state.snapshot(target!), ...at, identity: null });
          if (live) diffAt = { ...at, changed: f.changes.length };
        } catch {
          if (live) diffSame = `Same in ${srcLabel(sum.a)} and ${srcLabel(sum.b)}`;
        }
      } catch {
        /* no comparison loaded: no link */
      }
    });
    return () => {
      live = false;
    };
  });
  function showInDiff() {
    if (!diffAt) return;
    sheet = false;
    const { dataset, table, id } = diffAt;
    dev.openDiff({ dataset, table, id }, () => app.go('diff'));
  }

  function openInExplorer(what: string) {
    sheet = false;
    dev.openRecord(what, () => app.go('explorer'));
  }
  function openLoc(key: string) {
    sheet = false;
    app.stringsSearch = key.replace(/^@/, '');
    app.go('strings');
  }

  // Popovers close on an outside click.
  let colEl = $state<HTMLElement | null>(null);
  let filterEl = $state<HTMLElement | null>(null);
  $effect(() => {
    if (!colPop && !filterPop) return;
    const close = (e: MouseEvent) => {
      // A select inside a popover renders its list in a portal: not an outside click.
      if ((e.target as Element | null)?.closest?.('[data-slot=select-content]')) return;
      if (colPop && colEl && !colEl.contains(e.target as Node)) colPop = false;
      if (filterPop && filterEl && !filterEl.contains(e.target as Node)) filterPop = false;
    };
    window.addEventListener('pointerdown', close, true);
    return () => window.removeEventListener('pointerdown', close, true);
  });
  function popKey(e: KeyboardEvent, close: () => void) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
    }
  }

  // F6 / Shift+F6: sources -> group tree -> filter bar -> grid.
  let rootEl = $state<HTMLElement | null>(null);
  function onF6(e: KeyboardEvent) {
    if (e.key !== 'F6' || !rootEl) return;
    const stops = [
      rootEl.querySelector<HTMLElement>('[data-pane=sources] button'),
      // The tree's stop is its focused item; the pane's first button only when it is railed.
      rootEl.querySelector<HTMLElement>('[data-pane=tree] [role=treeitem][tabindex="0"]') ?? rootEl.querySelector<HTMLElement>('[data-pane=tree] button'),
      rootEl.querySelector<HTMLElement>('[data-pane=bar] input[type=search]'),
      rootEl.querySelector<HTMLElement>('[role=grid] [role=row][tabindex="0"]'),
    ].filter((x): x is HTMLElement => !!x);
    if (!stops.length) return;
    e.preventDefault();
    const at = stops.findIndex((x) => x === document.activeElement || (x.closest('[data-pane]')?.contains(document.activeElement) ?? false));
    stops[at < 0 ? 0 : (at + (e.shiftKey ? stops.length - 1 : 1)) % stops.length]!.focus();
  }
  const stateTone = { ready: 'success', off: 'muted', missing: 'warning', error: 'destructive' } as const;
  const stateLabel = { ready: 'Ready', off: 'Off', missing: 'Not read', error: 'Error' } as const;
</script>

{#if !target}
  <FullEmpty icon={Sheet_} title="No channel">Pick a game channel first.</FullEmpty>
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div bind:this={rootEl} class="flex h-full min-h-[420px] gap-3" onkeydown={onF6}>
    <aside data-pane="sources" class="bg-card flex w-56 shrink-0 xl:w-[260px] flex-col overflow-hidden rounded-lg border" aria-label="Sources">
      <div class="flex h-9 items-center gap-1 border-b pr-1 pl-3">
        <h2 class="eyebrow text-muted-foreground flex-1">Sources</h2>
        <Button variant="ghost" size="xs" onclick={() => void addFolder()}><FolderPlus aria-hidden="true" /> Add folder…</Button>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto p-1.5">
        {#if srcLoading}
          <div class="grid gap-1 p-1">{#each { length: 5 } as _, i (i)}<Skeleton class="h-6 w-full" />{/each}</div>
        {:else if srcError}
          <ErrorState title="Could not list sources" error={srcError} onretry={() => void loadSources()} />
        {:else}
          <ul class="grid gap-2">
            {#each sources as s (s.id)}
              <li class="grid gap-0.5">
                <div class="flex items-center gap-1.5 px-1.5">
                  <span class="min-w-0 flex-1 truncate text-[12.5px] font-medium" title={s.detail ?? s.label}>{s.label}</span>
                  <ToneBadge tone={stateTone[s.state]} label={stateLabel[s.state]} />
                  {#if s.kind === 'folder'}
                    <Button variant="ghost" size="icon-xs" aria-label="Remove {s.label}" title="Remove from the list (the folder stays)" onclick={() => void removeSource(s.id)}><Trash aria-hidden="true" /></Button>
                  {/if}
                </div>
                {#if s.state !== 'ready' && s.detail}
                  <p class="text-muted-foreground px-1.5 text-[11.5px]">{s.detail}</p>
                {/if}
                <ul class="grid">
                  {#each s.datasets as d (d.file)}
                    {@const on = pick?.source === s.id && pick?.dataset === d.file}
                    <li>
                      <button
                        type="button"
                        aria-current={on ? 'true' : undefined}
                        class={cn('hover:bg-accent/60 focus-visible:ring-ring/60 flex w-full items-center gap-2 rounded-md px-2 py-1 text-left outline-none focus-visible:ring-2', on && 'bg-accent text-primary')}
                        onclick={() => (pick = { source: s.id, dataset: d.file })}
                      >
                        <span class="mono min-w-0 flex-1 truncate text-[12px]">{d.file}</span>
                        <span class="text-muted-foreground mono shrink-0 text-[10.5px]">{int(d.rows)} rows · {bytes(d.bytes)}</span>
                      </button>
                    </li>
                  {/each}
                </ul>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </aside>

    {#if pick && showTree}
      {#if railed}
        <div data-pane="tree" class="bg-card flex w-9 shrink-0 flex-col items-center rounded-lg border py-1">
          <Button variant="ghost" size="icon-sm" aria-label="Show groups" title="Show groups" onclick={() => (railed = false)}>
            <PanelLeftOpen aria-hidden="true" />
          </Button>
          <ListTree class="text-muted-foreground mt-2 size-4" aria-hidden="true" />
          {#if node !== null}<span class="bg-primary mt-1 size-1.5 rounded-full" title="A group is picked" aria-hidden="true"></span>{/if}
        </div>
      {:else}
        <aside data-pane="tree" class="bg-card flex w-48 shrink-0 flex-col overflow-hidden rounded-lg border xl:w-60" aria-label="Groups">
          <div class="flex h-9 items-center gap-1 border-b pr-1 pl-3">
            <h2 class="eyebrow text-muted-foreground flex-1">Groups</h2>
            <Button variant="ghost" size="icon-xs" aria-label="Hide groups" title="Hide groups" onclick={() => (railed = true)}>
              <PanelLeftClose aria-hidden="true" />
            </Button>
          </div>
          <div class="border-b p-1.5">
            <SelectField ariaLabel="Group by" options={groupOptions} value={groupId} onchange={setGroupBy} class="w-full" />
          </div>
          {#if groupId === NONE}
            <p class="text-muted-foreground p-3 text-[12px]">Pick a grouping to split the table into groups.</p>
          {:else if !groups}
            <div class="grid gap-1 p-2">{#each { length: 6 } as _, i (i)}<Skeleton class="h-6 w-full" />{/each}</div>
          {:else}
            <DataTree nodes={groups.nodes} total={groups.total} selected={node} onselect={selectNode} bind:open label="Groups of {tableName}" allLabel="All rows" loading={groupsLoading} class="flex-1 px-1" />
          {/if}
        </aside>
      {/if}
    {/if}

    <div class="flex min-w-0 flex-1 flex-col gap-2.5">
      {#if dsError}
        <ErrorState title="Could not read the dataset" error={dsError} />
      {:else if !pick}
        <FullEmpty icon={Sheet_} title="Pick a dataset">This build's game data is always there. Add a folder of JSON files (an sc-gamedata <span class="mono">data</span> folder, for example) to view and compare it. Files are only read.</FullEmpty>
      {:else if !info}
        <Skeleton class="h-24 w-full" />
      {:else}
        <div class="flex flex-wrap items-baseline gap-x-4 gap-y-1">
          <h2 class="mono text-[14px] font-semibold">{info.file}</h2>
          <span class="text-muted-foreground text-[12px]">{bytes(info.bytes)}</span>
          {#each info.meta as m (m.key)}<span class="text-muted-foreground text-[11.5px]"><span class="mono">{m.key}</span> {m.value}</span>{/each}
        </div>
        {#if info.facts.length}
          <dl class="text-muted-foreground flex flex-wrap gap-x-4 gap-y-0.5 text-[11.5px]">
            {#each info.facts as f (f.key)}<div><dt class="inline">{f.key}</dt> <dd class="text-foreground mono inline">{f.value}</dd></div>{/each}
          </dl>
        {/if}
        <div data-pane="bar" class="flex flex-wrap items-center gap-2" role="search" aria-label="Filter rows">
          <SelectField ariaLabel="Table" options={tableOptions} value={tableName} onchange={(v) => (tableName = v)} class="min-w-48" />
          <div class="relative max-w-sm min-w-48 flex-1">
            <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
            <Input type="search" aria-label="Search the shown columns" placeholder="Search names or ids" class="h-8 pl-8" bind:value={search} />
          </div>
          <div class="relative" bind:this={colEl}>
            <Button variant="outline" size="sm" aria-expanded={colPop} aria-controls="col-pop" onclick={() => (colPop = !colPop)}><Columns aria-hidden="true" /> Columns ({shownCols.length})</Button>
            {#if colPop && table}
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <div id="col-pop" role="dialog" aria-label="Columns" tabindex="-1" class="bg-popover absolute top-full left-0 z-30 mt-1 flex max-h-[min(60vh,420px)] w-72 flex-col rounded-lg border p-2 shadow-lg" onkeydown={(e) => popKey(e, () => (colPop = false))}>
                <Input type="search" aria-label="Find a column" placeholder="Find a column" class="mb-1.5 h-7 text-[12px]" bind:value={colFind} />
                <ul class="min-h-0 flex-1 overflow-y-auto">
                  {#each table.columns.filter((c) => c.name.toLowerCase().includes(colFind.trim().toLowerCase())) as c (c.name)}
                    <li class="flex items-center gap-2 px-1 py-0.5">
                      <Checkbox id="col-{c.name}" checked={shownCols.includes(c.name)} onCheckedChange={() => toggleCol(c.name)} />
                      <label for="col-{c.name}" class="mono min-w-0 flex-1 truncate text-[12px]">{c.name}</label>
                      <span class="text-muted-foreground text-[10.5px]">{c.kind}</span>
                    </li>
                  {/each}
                </ul>
                <Button variant="ghost" size="xs" class="mt-1 self-start" onclick={() => (shownCols = [...(table?.default_columns ?? [])])}>Default columns</Button>
              </div>
            {/if}
          </div>
          <div class="relative" bind:this={filterEl}>
            <Button variant="outline" size="sm" aria-expanded={filterPop} aria-controls="filter-pop" onclick={() => (filterPop = !filterPop)}><Funnel aria-hidden="true" /> Filter</Button>
            {#if filterPop && table}
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <div id="filter-pop" role="dialog" aria-label="Filter a column" tabindex="-1" class="bg-popover absolute top-full right-0 z-30 mt-1 grid max-h-[min(60vh,420px)] w-80 gap-2 overflow-y-auto rounded-lg border p-2.5 shadow-lg" onkeydown={(e) => popKey(e, () => (filterPop = false))}>
                <SelectField ariaLabel="Column to filter" options={table.columns.map((c) => ({ id: c.name, label: `${c.name} (${c.kind})` }))} value={filterCol} placeholder="Column…" onchange={(v) => (filterCol = v)} />
                {#if filterColumn}
                  {@const cur = filters.find((f) => f.column === filterColumn.name)}
                  {#if filterColumn.distinct}
                    <div role="group" aria-label="Values" class="flex flex-wrap gap-1">
                      {#each filterColumn.distinct as v (v.value)}
                        {@const on = cur?.values.includes(v.value) ?? false}
                        <button
                          type="button"
                          aria-pressed={on}
                          class={cn('focus-visible:ring-ring/60 inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11.5px] outline-none focus-visible:ring-2', on ? 'border-primary bg-primary/10' : 'text-muted-foreground')}
                          onclick={() => setFilter(filterColumn.name, { values: on ? (cur?.values ?? []).filter((x) => x !== v.value) : [...(cur?.values ?? []), v.value] })}
                        >{v.value} <span class="mono text-[10px]">{int(v.count)}</span></button>
                      {/each}
                    </div>
                  {/if}
                  {#if filterColumn.kind === 'number'}
                    <div class="flex items-center gap-2 text-[12px]">
                      <Input type="number" aria-label="Minimum" placeholder={String(filterColumn.min ?? 'min')} class="h-7 w-24" value={cur?.min ?? ''} onchange={(e: Event) => setFilter(filterColumn.name, { min: (e.currentTarget as HTMLInputElement).value === '' ? null : Number((e.currentTarget as HTMLInputElement).value) })} />
                      <span>to</span>
                      <Input type="number" aria-label="Maximum" placeholder={String(filterColumn.max ?? 'max')} class="h-7 w-24" value={cur?.max ?? ''} onchange={(e: Event) => setFilter(filterColumn.name, { max: (e.currentTarget as HTMLInputElement).value === '' ? null : Number((e.currentTarget as HTMLInputElement).value) })} />
                    </div>
                  {/if}
                  <label class="flex items-center gap-2 text-[12px]">
                    <Checkbox checked={cur?.has === true} onCheckedChange={(v) => setFilter(filterColumn.name, { has: v ? true : null })} /> Has a value ({int(filterColumn.filled)} rows)
                  </label>
                {/if}
              </div>
            {/if}
          </div>
          <ShowIdsToggle />
          <span class="text-muted-foreground ml-auto text-[12px]" aria-live="polite">{total === null ? 'Loading…' : `${int(total)} / ${int(all)} rows`}</span>
        </div>
        {#if filters.length || sort || node !== null}
          <div role="group" aria-label="Active filters" class="flex flex-wrap items-center gap-1.5">
            {#if node !== null}
              <span class="border-primary bg-primary/10 inline-flex h-7 items-center gap-1 rounded-full border pr-1 pl-2.5 text-[12px]">
                <ListTree class="size-3.5" aria-hidden="true" /><span class="max-w-60 truncate">{nodeLabel || 'Group'}</span>
                <button type="button" aria-label="Show all groups" class="text-muted-foreground hover:text-foreground focus-visible:ring-ring/60 grid size-5 place-items-center rounded-full outline-none focus-visible:ring-2" onclick={() => selectNode(null)}><X class="size-3" aria-hidden="true" /></button>
              </span>
            {/if}
            {#each filters as f (f.column)}
              <span class="border-primary bg-primary/10 inline-flex h-7 items-center gap-1 rounded-full border pr-1 pl-2.5 text-[12px]">
                <span class="mono">{chipLabel(f)}</span>
                <button type="button" aria-label="Remove filter {f.column}" class="text-muted-foreground hover:text-foreground focus-visible:ring-ring/60 grid size-5 place-items-center rounded-full outline-none focus-visible:ring-2" onclick={() => (filters = filters.filter((x) => x.column !== f.column))}><X class="size-3" aria-hidden="true" /></button>
              </span>
            {/each}
            {#if sort}<span class="text-muted-foreground text-[12px]">Sorted by <span class="mono">{sort.column}</span> {sort.desc ? '↓' : '↑'}</span>{/if}
            <Button variant="ghost" size="xs" onclick={() => ((filters = []), (sort = null), selectNode(null))}>Clear</Button>
          </div>
        {/if}

        {#if rowsError}
          <ErrorState title="Could not read the rows" error={rowsError} />
        {:else}
          <div role="grid" aria-label={tableName ?? 'Rows'} aria-rowcount={(total ?? 0) + 1} aria-colcount={pageCols.length} tabindex="-1" class="bg-card flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border" onkeydown={onGridKey}>
            <div class="min-h-0 flex-1 overflow-x-auto">
              <div class="flex h-full min-w-max flex-col" style="min-width: {pageCols.length * COLW}px">
                <div role="rowgroup" class="border-b">
                  <div role="row" aria-rowindex={1} class="text-muted-foreground grid h-8 items-center gap-3 px-3 text-[12px] font-medium" style={gridCols}>
                    {#each pageCols as c (c)}
                      {@const m = colMeta(c)}
                      <span role="columnheader" aria-sort={sort?.column === c ? (sort.desc ? 'descending' : 'ascending') : undefined} class="truncate">
                        {#if m && ['text', 'number', 'bool'].includes(m.kind)}
                          <button type="button" class="hover:text-foreground mono truncate" title="Sort by {c}" onclick={() => toggleSort(c)}>{c}{sort?.column === c ? (sort.desc ? ' ↓' : ' ↑') : ''}</button>
                        {:else}<span class="mono">{c}</span>{/if}
                      </span>
                    {/each}
                  </div>
                </div>
                <div bind:this={scroller} role="rowgroup" class="relative min-h-0 flex-1 overflow-y-auto" onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}>
                  {#if total === null}
                    <div class="grid gap-1 p-3">{#each { length: 8 } as _, i (i)}<Skeleton class="h-6 w-full" />{/each}</div>
                  {:else if total === 0}
                    <p class="text-muted-foreground py-10 text-center">No rows match. Clear the search or the filters.</p>
                  {:else}
                    <div style="height: {total * ROW}px" class="relative">
                      {#each visible as v (v.i)}
                        {@const r = v.row}
                        <!-- svelte-ignore a11y_click_events_have_key_events -->
                        <div
                          role="row"
                          data-row={v.i}
                          aria-rowindex={v.i + 2}
                          aria-selected={active === v.i}
                          tabindex={tabRow === v.i ? 0 : -1}
                          class={cn('hover:bg-accent/60 focus-visible:ring-ring/60 absolute right-0 left-0 grid cursor-pointer items-center gap-3 border-b border-border/60 px-3 outline-none focus-visible:ring-2 focus-visible:ring-inset', active === v.i && 'bg-accent/70')}
                          style="top: {v.i * ROW}px; height: {ROW}px; {gridCols}"
                          onclick={() => {
                            active = v.i;
                            if (r) void openRow(r.id, r.title);
                          }}
                          onfocus={() => (active = v.i)}
                        >
                          {#if r}
                            {#each r.cells as c, k (k)}
                              {@const lab = r.labels?.[k] ?? null}
                              {#if c !== null && lab}
                                <span role="gridcell" class="flex min-w-0 text-[12px]"><ReadableValue raw={c} label={lab} /></span>
                              {:else}
                                {@const meta = colMeta(pageCols[k] ?? '')}
                                <span role="gridcell" class={cn('truncate text-[12px]', c === null && 'text-muted-foreground', meta?.kind === 'number' && 'mono text-right')} title={c ?? ''}>{(meta?.kind === 'number' ? numberCell(c, meta.unit, dev.showIds) : c) ?? '—'}</span>
                              {/if}
                            {/each}
                          {:else}
                            <span role="gridcell" class="col-span-full"><Skeleton class="h-4 w-2/3" /></span>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  {/if}
                </div>
              </div>
            </div>
          </div>
          <p class="text-muted-foreground text-[11.5px]">Arrow keys move, Enter opens a row. Click a column name to sort. Text cells are cut at 200 characters; the row shows everything.</p>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<Sheet bind:open={sheet} title={detail ? `${tableName} · ${detailTitle && !dev.showIds ? `${detailTitle} (row ${detail.row + 1})` : `row ${detail.row + 1}`}` : 'Row'} width="w-[min(720px,calc(100vw-48px))]">
  {#if detail}
    <div class="flex h-[calc(100vh-140px)] min-h-0 flex-col gap-3">
      {#if diffAt}
        <div><Button variant="outline" size="xs" title={`${diffAt.table} · ${diffAt.id}`} onclick={showInDiff}><GitCompare aria-hidden="true" /> Show changes in Diff<span class="text-muted-foreground mono">{diffAt.changed} {diffAt.changed === 1 ? 'field' : 'fields'}</span></Button></div>
      {:else if diffSame}
        <p class="text-muted-foreground flex items-center gap-1.5 text-[12px]"><GitCompare class="size-3.5" aria-hidden="true" />{diffSame}</p>
      {/if}
      {#if detail.row_links.length && pick}
        {@const at = pick}
        <div class="grid gap-1">
          <h3 class="eyebrow text-muted-foreground">Rows it names</h3>
          <ul class="flex flex-wrap gap-1.5">
            {#each detail.row_links as k (k.table + '/' + k.row)}
              <li>
                <Button variant="outline" size="xs" title={`${k.pointer}\n${k.value}`} onclick={() => void goToRow({ ...at, table: k.table, row: k.row })}>
                  <ArrowUpRight aria-hidden="true" />{k.title && !dev.showIds ? k.title : k.value}{#if k.count > 1}<span class="mono">×{k.count}</span>{/if}<span class="text-muted-foreground mono text-[10.5px]">{k.table}</span>
                </Button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if detail.links.length}
        <div class="grid gap-1">
          <h3 class="eyebrow text-muted-foreground">In the explorer</h3>
          <ul class="flex flex-wrap gap-1.5">
            {#each detail.links as l (l.pointer)}
              {@const lab = dev.showIds ? null : (detail.labels[l.value] ?? null)}
              <li><Button variant="outline" size="xs" title={`${l.pointer}\n${l.value}`} onclick={() => openInExplorer(l.what)}><ArrowUpRight aria-hidden="true" /> Open {lab ? lab.text : l.value} in Explorer{#if lab}<span class="text-muted-foreground mono text-[10.5px]">{l.value}</span>{/if}</Button></li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if detail.loc.length}
        <div class="grid gap-1">
          <h3 class="eyebrow text-muted-foreground">Text</h3>
          <ul class="grid gap-0.5 text-[12px]">
            {#each detail.loc as l (l.key)}
              <li class="flex gap-2"><button type="button" class="text-primary mono shrink-0 underline-offset-2 hover:underline" title="Open in Strings" onclick={() => openLoc(l.key)}>{l.key}</button><span class="text-muted-foreground truncate">{l.text}</span></li>
            {/each}
          </ul>
        </div>
      {/if}
      <JsonTree id="row" json={detail.json} label="row" labels={detail.labels} units onfollow={(r) => openInExplorer(r.guid)} onloc={openLoc} class="min-h-0 flex-1 rounded-md border" />
    </div>
  {/if}
</Sheet>
