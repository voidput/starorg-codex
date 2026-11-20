<script lang="ts">
  /**
   * Explorer (addendum 7.4, 7.8): the channel's DataCore in three panes — tree (by type or by
   * path, children fetched 500 at a time) | search hits or the selected node's records
   * (virtualised) | the record (header, copy buttons, exports, the JSON tree with clickable
   * references and loc text). Below 1,100 px the tree is a drawer. Each pane scrolls inside
   * itself. Keys: F6 / Shift+F6 cycle the panes, `/` or Ctrl+F focuses search, Esc returns to
   * the list, Alt+Left / Alt+Right (and the mouse's back/forward buttons) walk the history.
   * The worker starts when the screen opens; a crash is a "stopped" banner with Retry.
   */
  import { tick, untrack } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import Copy from '@lucide/svelte/icons/copy';
  import Download from '@lucide/svelte/icons/download';
  import ListTree from '@lucide/svelte/icons/list-tree';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Binary from '@lucide/svelte/icons/binary';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import JobProgress from '@/components/common/JobProgress.svelte';
  import JsonTree from '@/components/dev/JsonTree.svelte';
  import ShowIdsToggle from '@/components/dev/ShowIdsToggle.svelte';
  import AssetPanel from '@/components/assets/AssetPanel.svelte';
  import TriggerPanel from '@/components/assets/TriggerPanel.svelte';
  import { hasModel } from '@/lib/contract/assets';
  import ItemFinder from '@/components/dev/ItemFinder.svelte';
  import CircleHelp from '@lucide/svelte/icons/circle-help';
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import * as F from '@/lib/finder';
  import { devApi, EXPORT_WARN_RECORDS, type RecordView, type SearchHit, type TreeNode, type TreeRoot } from '@/lib/contract/dev';
  import { toContractError, type ContractError } from '@/lib/contract/client';
  import { guidOf, treeKey, treeRows, typeahead, bytes, type Loaded } from '@/lib/devnav';
  import type { Json, RefInfo } from '@/lib/jsontree';
  import { app } from '@/lib/stores/app.svelte';
  import { dev } from '@/lib/stores/dev.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';

  const PAGE = 500;
  const TROW = 28;
  const LROW = 44;
  const OVERSCAN = 8;

  const target = $derived(app.target);
  const tkey = $derived(app.targetKey);
  let root = $state<TreeRoot>('type');
  let mainOnly = $state(true);

  // --- Mode and the item finder's view (7.17) ----------------------------------------------
  const store = F.storage();
  let view = $state<F.FinderView>({ ...F.DEFAULT_VIEW, mode: F.loadMode(store) });
  // The finder's folded row of the open record, for the header's Variant picker.
  let variantGroup = $state<F.VariantGroup | null>(null);
  let finderQuery = $state('');
  let trigger = $state<string | null>(null);
  let help = $state(false);
  // Recent searches (7.17): kept when a search is confirmed with Enter or a result is opened.
  let recent = $state<string[]>(F.loadRecent(store));
  const remember = (q: string) => {
    if (q.trim().length >= 2) recent = F.pushRecent(store, q);
  };
  function setMode(m: F.Mode) {
    F.saveMode(store, m);
    const media = view.node.startsWith('m:');
    view = { ...view, mode: m, node: m === 'assets' && !media ? 'm:models:[]' : m === 'items' && media ? '' : view.node, collapsed: [] };
    if (m !== 'assets') trigger = null;
  }
  const finding = $derived(view.mode !== 'raw');
  // Compare with another channel (7.17): GameData diffed in the backend, marks in the worker.
  let compareWith = $state('');
  let comparing = $state(false);
  const compared = $derived(dev.status?.compared ?? null);
  const others = $derived(app.channels.map((c) => c.channel).filter((c) => c !== target?.channel));
  async function compare(other: string) {
    if (!target) return;
    comparing = true;
    try {
      const r = await devApi.compare({ target: $state.snapshot(target), other: other || null });
      if (r) toast('success', `Compared with ${r.other}`, `${int(r.added)} added, ${int(r.changed)} changed, ${int(r.removed)} only in ${r.other} (${r.ms} ms)`);
      await dev.ensure($state.snapshot(target), false);
    } catch (e) {
      compareWith = '';
      toast('error', 'Could not compare', toContractError(e).message);
    } finally {
      comparing = false;
    }
  }

  // --- Worker --------------------------------------------------------------------------
  let ready = $state(false);
  async function start() {
    if (!target) return;
    ready = false;
    const s = await dev.ensure($state.snapshot(target), true);
    ready = s?.state === 'ready';
    if (ready) void loadRoot();
  }
  $effect(() => {
    void tkey;
    untrack(() => {
      loaded = new Map();
      open = new Set();
      void start();
    });
  });

  // --- Tree ----------------------------------------------------------------------------
  let loaded = $state.raw(new Map<string, Loaded>());
  let open = $state.raw(new Set<string>());
  let treeActive = $state('');
  let treeError = $state<ContractError | null>(null);
  async function fetchChildren(node: string, offset: number) {
    if (!target) return;
    try {
      const r = await devApi.tree({ target: $state.snapshot(target), root, node, offset, limit: PAGE, main_only: mainOnly });
      const prev = offset ? loaded.get(node)?.nodes ?? [] : [];
      const n = new Map(loaded);
      n.set(node, { nodes: [...prev, ...r.nodes], total: r.total });
      loaded = n;
      treeError = null;
    } catch (e) {
      treeError = toContractError(e);
    }
  }
  async function loadRoot() {
    loaded = new Map();
    open = new Set();
    await fetchChildren('', 0);
    treeActive = loaded.get('')?.nodes[0]?.id ?? '';
  }
  $effect(() => {
    void root;
    void mainOnly;
    untrack(() => {
      if (ready) void loadRoot();
    });
  });
  const rows = $derived(treeRows(loaded, open));
  async function toggle(id: string, v: boolean) {
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
  let typed = '';
  let typedAt = 0;
  function activate(n: TreeNode) {
    if (n.kind === 'record') {
      void openRecord(guidOf(n.id)!);
      if (narrow) drawer = false;
    } else selectNode(n);
  }
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
    } else if (a.kind === 'open') void toggle(a.id, true);
    else if (a.kind === 'close') void toggle(a.id, false);
    else if (a.kind === 'more') void fetchChildren(a.parent, loaded.get(a.parent)?.nodes.length ?? 0);
    else if (a.kind === 'activate') {
      const n = rows.find((r) => r.id === a.id)?.node;
      if (n) activate(n);
    }
    // Enter on a type or folder also lists its records in the middle pane.
    if (e.key === 'Enter') {
      const n = rows.find((r) => r.id === treeActive)?.node;
      if (n && n.kind !== 'record') selectNode(n);
    }
  }

  // --- Middle pane: search hits or the selected node's records ------------------------
  let search = $state('');
  let debounced = $state('');
  $effect(() => {
    const s = search;
    const t = setTimeout(() => (debounced = s.trim()), 200);
    return () => clearTimeout(t);
  });
  let selected = $state<TreeNode | null>(null);
  let hits = $state<SearchHit[]>([]);
  let hitTotal = $state<number | null>(null);
  let listNodes = $state<TreeNode[]>([]);
  let listTotal = $state(0);
  let listError = $state<ContractError | null>(null);
  let listGen = 0;
  $effect(() => {
    const q = debounced;
    void tkey;
    untrack(() => {
      if (!ready || !target) return;
      const g = ++listGen;
      if (!q) {
        hits = [];
        hitTotal = null;
        return;
      }
      void devApi
        .search({ target: $state.snapshot(target), query: q, limit: 500 })
        .then((r) => {
          if (g !== listGen) return;
          hits = r.hits;
          hitTotal = r.total;
          listActive = 0;
          listError = null;
        })
        .catch((e) => {
          if (g === listGen) listError = toContractError(e);
        });
    });
  });
  async function selectNode(n: TreeNode) {
    selected = n;
    search = '';
    debounced = '';
    listActive = 0;
    if (!target) return;
    try {
      const r = await devApi.tree({ target: $state.snapshot(target), root, node: n.id, offset: 0, limit: PAGE, main_only: mainOnly });
      listNodes = r.nodes;
      listTotal = r.total;
      listError = null;
    } catch (e) {
      listError = toContractError(e);
    }
  }
  async function moreList() {
    if (!target || !selected) return;
    const r = await devApi.tree({ target: $state.snapshot(target), root, node: selected.id, offset: listNodes.length, limit: PAGE, main_only: mainOnly });
    listNodes = [...listNodes, ...r.nodes];
  }
  interface Item {
    guid: string | null;
    name: string;
    sub: string;
    main: boolean;
    node: TreeNode | null;
  }
  const items = $derived<Item[]>(
    debounced
      ? hits.map((h) => ({ guid: h.guid, name: h.name, sub: `${h.type} · ${h.path}`, main: h.main, node: null }))
      : listNodes.map((n) => ({ guid: guidOf(n.id), name: n.label, sub: n.kind === 'record' ? (n.main ? 'record' : 'sub-record') : `${n.kind} · ${int(n.count)} records`, main: n.main, node: n })),
  );
  let listActive = $state(0);
  let listScroller = $state<HTMLElement | null>(null);
  let listTop = $state(0);
  let listH = $state(400);
  const lFirst = $derived(Math.max(0, Math.floor(listTop / LROW) - OVERSCAN));
  const lLast = $derived(Math.min(items.length - 1, Math.ceil((listTop + listH) / LROW) + OVERSCAN));
  function revealList(i: number) {
    if (!listScroller) return;
    const top = i * LROW;
    if (top < listScroller.scrollTop) listScroller.scrollTop = top;
    else if (top + LROW > listScroller.scrollTop + listScroller.clientHeight) listScroller.scrollTop = top + LROW - listScroller.clientHeight;
    listTop = listScroller.scrollTop;
  }
  function onListKey(e: KeyboardEvent) {
    if (e.altKey || e.ctrlKey || e.metaKey || !items.length) return;
    const per = Math.max(1, Math.floor(listH / LROW) - 1);
    const map: Record<string, number> = { ArrowDown: listActive + 1, ArrowUp: listActive - 1, PageDown: listActive + per, PageUp: listActive - per, Home: 0, End: items.length - 1 };
    if (e.key in map) {
      e.preventDefault();
      listActive = Math.max(0, Math.min(items.length - 1, map[e.key]!));
      revealList(listActive);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(items[listActive]!);
    }
  }
  function pick(it: Item) {
    if (it.guid) void openRecord(it.guid);
    else if (it.node) void selectNode(it.node);
  }

  // --- Record --------------------------------------------------------------------------
  let rec = $state<RecordView | null>(null);
  /** The record pane's tab: its data, or (entities) the Model tab's 3D view (addendum 7.20). */
  let recTab = $state<'data' | 'model'>('data');
  /** three.js and the viewer load the first time the Model tab opens (a chunk of their own). */
  let viewer: Promise<typeof import('@/components/assets/ModelViewer.svelte')> | null = null;
  const loadViewer = () => (viewer ??= import('@/components/assets/ModelViewer.svelte'));
  let recError = $state<ContractError | null>(null);
  let recLoading = $state(false);
  async function show(what: string): Promise<boolean> {
    if (!target) return false;
    recLoading = true;
    try {
      rec = await devApi.record({ target: $state.snapshot(target), what, pointer: '', depth: null });
      recError = null;
      return true;
    } catch (e) {
      recError = toContractError(e);
      return false;
    } finally {
      recLoading = false;
    }
  }
  async function openRecord(what: string) {
    trigger = null;
    if (await show(what)) {
      // The finder's view rides along, so Back/Forward brings the list and filters back.
      dev.history.visit(F.encodeEntry(rec!.header.guid, finding ? $state.snapshot(view) : null));
      dev.histRev++;
    }
  }
  async function restore(entry: string | null) {
    if (!entry) return;
    const e = F.decodeEntry(entry);
    if (e.view) {
      view = e.view;
      finderQuery = e.view.query;
    }
    trigger = null;
    await show(e.guid);
  }
  async function back() {
    const g = dev.history.goBack();
    dev.histRev++;
    await restore(g);
  }
  async function forward() {
    const g = dev.history.goForward();
    dev.histRev++;
    await restore(g);
  }
  const canBack = $derived((void dev.histRev, dev.history.canBack));
  const canForward = $derived((void dev.histRev, dev.history.canForward));
  async function loadAt(pointer: string): Promise<Json> {
    if (!target || !rec) return null;
    const r = await devApi.record({ target: $state.snapshot(target), what: rec.header.guid, pointer, depth: 3 });
    // Labels of the part just loaded join the record's.
    if (rec) rec = { ...rec, labels: { ...rec.labels, ...r.labels } };
    return JSON.parse(r.json) as Json;
  }
  function follow(r: RefInfo) {
    void openRecord(r.guid);
  }
  function openLoc(key: string) {
    app.stringsSearch = key.replace(/^@/, '');
    app.go('strings');
  }
  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast('success', `Copied ${what}`);
    } catch {
      toast('error', 'The clipboard is not available here');
    }
  }

  // --- Exports -------------------------------------------------------------------------
  let exportOpen = $state(false);
  let exportType = $state<{ name: string; count: number } | null>(null);
  let exporting = $state(false);
  async function exportRecord() {
    if (!target || !rec) return;
    try {
      const r = await devApi.exportRecord({ target: $state.snapshot(target), what: rec.header.guid });
      if (r.cancelled) toast('info', 'Export cancelled', 'Nothing was written.');
      else toast('success', `Exported ${rec.header.name}`, r.path);
    } catch (e) {
      toast('error', 'Export failed', toContractError(e).message);
    }
  }
  async function askExportType() {
    if (!rec || !target) return;
    const types = await devApi.types($state.snapshot(target)).catch(() => []);
    const t = types.find((x) => x.name === rec!.header.type);
    exportType = { name: rec.header.type, count: mainOnly ? (t?.main ?? 0) : (t?.records ?? 0) };
    exportOpen = true;
  }
  async function runExportType() {
    if (!target || !exportType) return;
    exporting = true;
    exportOpen = false;
    try {
      const r = await devApi.exportType({ target: $state.snapshot(target), type_name: exportType.name, main_only: mainOnly });
      if (r.cancelled) toast('info', 'Export cancelled', 'Nothing was written.');
      else toast('success', `Exported ${int(r.records)} ${exportType.name} records`, `${bytes(r.bytes)} · ${r.path}`);
    } catch (e) {
      const err = toContractError(e);
      if (err.code === 'cancelled') toast('info', 'Export stopped', 'The partial file was removed.');
      else toast('error', 'Export failed', err.message);
    } finally {
      exporting = false;
    }
  }

  // --- Hand-offs from Ctrl+K and the dataset viewer --------------------------------------
  let searchEl = $state<HTMLInputElement | null>(null);
  $effect(() => {
    void dev.seq;
    const r = ready;
    untrack(() => {
      if (!r) return;
      if (dev.pendingSearch !== null) {
        if (finding) finderQuery = dev.pendingSearch;
        else search = dev.pendingSearch;
        dev.pendingSearch = null;
        void tick().then(() => searchEl?.focus());
      }
      if (dev.pendingOpen !== null) {
        const w = dev.pendingOpen;
        dev.pendingOpen = null;
        void openRecord(w);
      }
    });
  });

  // --- Layout, panes and keys -------------------------------------------------------------
  let winW = $state(1280);
  const narrow = $derived(winW < 1100);
  let drawer = $state(false);
  let treeW = $state(280);
  let listW = $state(340);
  // Items/Assets mode: the finder's width (the record pane takes the rest), kept per machine.
  const FINDER_W = 'starorg.explorer.finderW';
  let finderW = $state(readW(FINDER_W, 760));
  function readW(k: string, d: number): number {
    try {
      const v = Number(localStorage.getItem(k));
      return Number.isFinite(v) && v >= 200 ? v : d;
    } catch {
      return d;
    }
  }
  function keepW(k: string, v: number) {
    try {
      localStorage.setItem(k, String(Math.round(v)));
    } catch {
      // Storage off: the width resets next start.
    }
  }
  const clampFinder = (w: number) => Math.min(Math.max(480, winW - 380), Math.max(480, w));
  let rootEl = $state<HTMLElement | null>(null);
  function resize(which: 'tree' | 'list' | 'finder', e: KeyboardEvent) {
    const d = e.key === 'ArrowLeft' ? -16 : e.key === 'ArrowRight' ? 16 : 0;
    if (!d) return;
    e.preventDefault();
    if (which === 'finder') keepW(FINDER_W, (finderW = clampFinder(finderW + d * 2)));
    else if (which === 'tree') treeW = Math.min(480, Math.max(200, treeW + d));
    else listW = Math.min(560, Math.max(240, listW + d));
  }
  let dragging: 'tree' | 'list' | 'finder' | null = null;
  let dragX = 0;
  let dragW = 0;
  function startDrag(which: 'tree' | 'list' | 'finder', e: PointerEvent) {
    dragging = which;
    dragX = e.clientX;
    dragW = which === 'finder' ? finderW : which === 'tree' ? treeW : listW;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }
  function drag(e: PointerEvent) {
    if (!dragging) return;
    const w = dragW + e.clientX - dragX;
    if (dragging === 'finder') finderW = clampFinder(w);
    else if (dragging === 'tree') treeW = Math.min(480, Math.max(200, w));
    else listW = Math.min(560, Math.max(240, w));
  }
  function panes(): HTMLElement[] {
    if (!rootEl) return [];
    const tree = rootEl.querySelector<HTMLElement>('[data-pane=tree] [role=tree]');
    const list = rootEl.querySelector<HTMLElement>('[data-pane=list] [role=listbox]');
    const detail = rootEl.querySelector<HTMLElement>('[data-pane=detail] [role=tree]');
    return [tree, list, detail].filter((x): x is HTMLElement => !!x && x.offsetParent !== null);
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === 'F6') {
      const stops = panes();
      if (!stops.length) return;
      e.preventDefault();
      const at = stops.findIndex((x) => x.contains(document.activeElement));
      stops[at < 0 ? 0 : (at + (e.shiftKey ? stops.length - 1 : 1)) % stops.length]!.focus();
      return;
    }
    if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
      e.preventDefault();
      void (e.key === 'ArrowLeft' ? back() : forward());
      return;
    }
    const inField = (e.target as HTMLElement | null)?.closest('input, textarea, select');
    if (!inField && (e.key === '/' || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f'))) {
      // Inside the record, Ctrl+F searches the record (handled there).
      if ((e.target as HTMLElement | null)?.closest('[data-pane=detail]')) return;
      e.preventDefault();
      searchEl?.focus();
    }
  }
  function onSearchKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && finding) remember(finderQuery);
    if (e.key === 'Escape' || e.key === 'ArrowDown') {
      e.preventDefault();
      rootEl?.querySelector<HTMLElement>('[data-pane=list] [role=listbox]')?.focus();
    }
  }
  function onMouse(e: MouseEvent) {
    if (e.button === 3) void back();
    else if (e.button === 4) void forward();
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
  const status = $derived(dev.status);
  const failed = $derived(status?.state === 'failed' || (!!dev.error && !dev.opening && !ready));
</script>

<svelte:window bind:innerWidth={winW} />

{#snippet treePane()}
  <div class="flex h-9 items-center gap-2 border-b px-2">
    <Segmented size="sm" ariaLabel="Tree" options={[{ id: 'type', label: 'By type' }, { id: 'path', label: 'By path' }] as { id: TreeRoot; label: string }[]} bind:value={root} />
    <span class="flex-1"></span>
  </div>
  <div class="border-b px-2.5 py-1">
    <SwitchRow id="dev-main-only" size="sm" label="Main records only" checked={mainOnly} onchange={(v) => (mainOnly = v)} class="py-0.5" />
  </div>
  {#if treeError}
    <p class="text-destructive p-3 text-[12px]">{treeError.message}</p>
  {/if}
  <div bind:this={treeScroller} class="relative min-h-0 flex-1 overflow-auto" onscroll={() => (treeTop = treeScroller?.scrollTop ?? 0)}>
    {#if !loaded.has('')}
      <div class="grid gap-1 p-2">{#each { length: 10 } as _, i (i)}<Skeleton class="h-5 w-full" />{/each}</div>
    {:else}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        role="tree"
        aria-label="DataCore {root === 'type' ? 'types' : 'folders'}"
        tabindex="0"
        aria-activedescendant={rows[tIndex] ? `dt-${tIndex}` : undefined}
        class="focus-visible:ring-ring/60 relative outline-none focus-visible:ring-2 focus-visible:ring-inset"
        style="height: {rows.length * TROW}px"
        onkeydown={onTreeKey}
      >
        {#each rows.slice(tFirst, tLast + 1) as r, k (r.id)}
          {@const i = tFirst + k}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id="dt-{i}"
            role="treeitem"
            aria-level={r.level}
            aria-posinset={r.pos}
            aria-setsize={r.size}
            aria-expanded={r.node && r.node.kind !== 'record' ? r.expanded : undefined}
            aria-selected={i === tIndex}
            aria-label={r.node ? `${r.node.label}${r.node.kind !== 'record' ? `, ${r.node.count} records` : ''}` : `Show ${Math.min(PAGE, r.more)} more`}
            tabindex="-1"
            class={cn('absolute right-0 left-0 flex cursor-pointer items-center gap-1 pr-2 text-[12.5px] whitespace-nowrap', i === tIndex && 'bg-accent/70', selected?.id === r.id && 'text-primary font-medium')}
            style="top: {i * TROW}px; height: {TROW}px; padding-left: {4 + (r.level - 1) * 14}px"
            onclick={() => {
              treeActive = r.id;
              if (!r.node) void fetchChildren(r.parent, loaded.get(r.parent)?.nodes.length ?? 0);
              else if (r.node.kind === 'record') activate(r.node);
              else {
                void toggle(r.id, !r.expanded);
                selectNode(r.node);
              }
            }}
          >
            {#if r.node && r.node.kind !== 'record'}
              <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', r.expanded && 'rotate-90')} aria-hidden="true" />
              <span class="truncate">{r.node.label}</span>
              <span class="text-muted-foreground mono ml-auto pl-2 text-[11px]">{int(r.node.count)}</span>
            {:else if r.node}
              <span class="w-3.5 shrink-0"></span>
              <span class={cn('mono truncate text-[12px]', !r.node.main && 'text-muted-foreground')}>{r.node.label}</span>
            {:else}
              <span class="w-3.5 shrink-0"></span>
              <span class="text-primary text-[12px]">Show {int(Math.min(PAGE, r.more))} more</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#if !target}
  <FullEmpty icon={Binary} title="No channel">Pick a game channel to explore its data.</FullEmpty>
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div bind:this={rootEl} class="flex h-full min-h-[420px] flex-col gap-3" onkeydown={onKey} onmouseup={onMouse}>
    <div class="flex flex-wrap items-center gap-2">
      {#if narrow && !finding}
        <Button variant="outline" size="sm" aria-expanded={drawer} aria-controls="dev-drawer" onclick={() => (drawer = !drawer)}><ListTree aria-hidden="true" /> Tree</Button>
      {/if}
      <Segmented size="sm" ariaLabel="Explorer mode" value={view.mode} onchange={setMode} options={[{ id: 'items', label: 'Items', hint: 'Find items and ships by section, with filters and stats' }, { id: 'assets', label: 'Assets', hint: 'Entities with models or sounds; search sound events' }, { id: 'raw', label: 'Raw', hint: 'DataCore by type or path, as stored' }] as { id: F.Mode; label: string; hint: string }[]} />
      <Button variant="ghost" size="icon-sm" aria-label="Back (Alt+Left)" title="Back (Alt+Left)" disabled={!canBack} onclick={() => void back()}><ArrowLeft aria-hidden="true" /></Button>
      <Button variant="ghost" size="icon-sm" aria-label="Forward (Alt+Right)" title="Forward (Alt+Right)" disabled={!canForward} onclick={() => void forward()}><ArrowRight aria-hidden="true" /></Button>
      <div class={cn('relative min-w-48 flex-1', finding ? 'max-w-xl basis-72' : 'max-w-md')} role="search" aria-label="Find records">
        <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
        {#if finding}
          <Input bind:ref={searchEl} type="search" list="finder-recent" title={finderQuery} aria-label={view.mode === 'assets' && view.sounds ? 'Find sound events by name' : 'Find items: words and prefixes like type:, maker:, size:'} placeholder={view.mode === 'assets' && view.sounds ? 'Sound event: gladius, quantum, explosion…' : 'Name, or type:cooler maker:aegis size:1…'} class="h-8 pl-8" bind:value={finderQuery} onkeydown={onSearchKey} disabled={!ready} />
        {:else}
          <Input bind:ref={searchEl} type="search" aria-label="Find records by name, path or GUID" placeholder="Name, path or GUID" class="h-8 pl-8" bind:value={search} onkeydown={onSearchKey} disabled={!ready} />
        {/if}
      </div>
      {#if finding}
        <Button variant="ghost" size="icon-sm" aria-label="Search help" title="Search help" aria-expanded={help} aria-controls="finder-help" onclick={() => (help = !help)}><CircleHelp aria-hidden="true" /></Button>
      {/if}
      <label class="text-muted-foreground flex items-center gap-1 text-[11.5px]" title="Mark what changed against another channel's build (its game data must have been read)">
        <GitCompare class="size-3.5" aria-hidden="true" />
        <select class="border-input bg-background text-foreground h-7 rounded-md border px-1 text-[11.5px]" aria-label="Compare with channel" disabled={!ready || comparing || others.length === 0} bind:value={compareWith} onchange={() => void compare(compareWith)}>
          <option value="">{compared ? 'No comparison' : 'Compare with…'}</option>
          {#each others as c (c)}<option value={c}>vs {c}</option>{/each}
        </select>
      </label>
      <span class="flex-1"></span>
      {#if app.job?.kind === 'dev'}
        <JobProgress job={app.job} compact />
      {:else if status?.state === 'ready'}
        <span class="text-muted-foreground mono text-[11.5px]">{int(status.records)} records · {int(status.types)} types{status.memory ? ` · ${bytes(status.memory)}` : ''}{status.start_ms !== null ? ` · opened in ${(status.start_ms / 1000).toFixed(1)} s` : ''}</span>
      {/if}
    </div>

    <datalist id="finder-recent">{#each recent as r (r)}<option value={r}></option>{/each}</datalist>
    {#if help && finding}
      <div id="finder-help" class="bg-card grid gap-1 rounded-lg border px-3 py-2 text-[12px]">
        <p>Every word must appear in the readable name, the record name, the type, manufacturer, class or tag. Only when nothing matches as typed, close spellings are tried (<span class="mono">galdius</span> finds Gladius; words of 5+ letters), then abbreviations (<span class="mono">glds</span>); the list says so. Searches you confirm with Enter or open a result from are offered again. Prefixes narrow the search:</p>
        <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-0.5">
          {#each F.PREFIXES as [k, v] (k)}<dt class="mono text-primary">{k}</dt><dd class="text-muted-foreground">{v}</dd>{/each}
        </dl>
      </div>
    {/if}
    {#if failed}
      <div class="border-destructive/50 bg-destructive/5 flex flex-wrap items-center gap-3 rounded-lg border px-4 py-3" role="alert">
        <TriangleAlert class="text-destructive size-4" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <div class="font-medium">The explorer stopped</div>
          <div class="text-muted-foreground text-[12px]">{status?.error ?? dev.error?.message ?? 'It could not read the game data.'} The app is fine; only the explorer process ended.</div>
        </div>
        <Button variant="outline" size="sm" onclick={() => void start()}>Retry</Button>
      </div>
    {:else if !ready}
      <div class="text-muted-foreground flex items-center gap-2 text-[12.5px]" role="status">
        <LoaderCircle class="size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Opening the game data of {target.channel}… (read only; about a second and 0.9 GB while it starts)
      </div>
    {/if}

    <div class="relative flex min-h-0 flex-1 gap-0">
      {#if finding && target}
        <div class={cn('flex min-h-0 min-w-0', narrow ? 'flex-[2] basis-0' : 'shrink-0')} style={narrow ? undefined : `width: ${clampFinder(finderW)}px`}>
          <ItemFinder {target} {ready} {narrow} {compared} bind:view bind:query={finderQuery} bind:variants={variantGroup} onremember={remember} selected={rec?.header.guid ?? null} selectedTrigger={trigger} onopen={(g) => void openRecord(g)} onopentrigger={(n) => (trigger = n)} />
        </div>
        {#if narrow}
          <div class="w-2 shrink-0"></div>
        {:else}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <div role="separator" aria-orientation="vertical" aria-label="Resize item list" aria-valuenow={clampFinder(finderW)} aria-valuemin={480} tabindex="0" class="hover:bg-primary/40 focus-visible:bg-primary/60 mx-1 w-1.5 shrink-0 cursor-col-resize rounded outline-none" onkeydown={(e) => resize('finder', e)} onpointerdown={(e) => startDrag('finder', e)} onpointermove={drag} onpointerup={() => { dragging = null; keepW(FINDER_W, finderW); }}></div>
        {/if}
      {:else}
      {#if !narrow}
        <aside data-pane="tree" class="bg-card flex shrink-0 flex-col overflow-hidden rounded-lg border" style="width: {treeW}px" aria-label="Record tree">
          {@render treePane()}
        </aside>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <div role="separator" aria-orientation="vertical" aria-label="Resize tree" aria-valuenow={treeW} aria-valuemin={200} aria-valuemax={480} tabindex="0" class="hover:bg-primary/40 focus-visible:bg-primary/60 mx-1 w-1.5 shrink-0 cursor-col-resize rounded outline-none" onkeydown={(e) => resize('tree', e)} onpointerdown={(e) => startDrag('tree', e)} onpointermove={drag} onpointerup={() => (dragging = null)}></div>
      {:else if drawer}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <aside
          id="dev-drawer"
          data-pane="tree"
          class="bg-popover absolute top-0 bottom-0 left-0 z-20 flex w-[min(300px,80%)] flex-col overflow-hidden rounded-lg border shadow-lg"
          aria-label="Record tree"
          onkeydown={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation();
              drawer = false;
            }
          }}
        >
          {@render treePane()}
        </aside>
      {/if}

      <section data-pane="list" class="bg-card flex shrink-0 flex-col overflow-hidden rounded-lg border" style="width: {narrow ? Math.min(listW, 300) : listW}px" aria-label="Records">
        <div class="flex h-9 items-center gap-2 border-b px-3 text-[12px]">
          <span class="truncate font-medium">{debounced ? `Search “${debounced}”` : selected ? selected.label : 'Records'}</span>
          <span class="text-muted-foreground mono ml-auto shrink-0" aria-live="polite">
            {debounced ? (hitTotal === null ? '…' : `${int(hits.length)} of ${int(hitTotal)}`) : selected ? `${int(listNodes.length)} of ${int(listTotal)}` : ''}
          </span>
        </div>
        {#if listError}
          <div class="p-3"><ErrorState title="Could not list records" error={listError} /></div>
        {:else if !items.length}
          <p class="text-muted-foreground p-4 text-[12.5px]">{debounced ? 'No record matches. Every word must appear in the name or path; a GUID matches from its start.' : 'Pick a type or folder in the tree, or search.'}</p>
        {:else}
          <div bind:this={listScroller} class="relative min-h-0 flex-1 overflow-auto" onscroll={() => (listTop = listScroller?.scrollTop ?? 0)}>
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <div
              role="listbox"
              aria-label="Records"
              tabindex="0"
              aria-activedescendant={`dl-${listActive}`}
              class="focus-visible:ring-ring/60 relative outline-none focus-visible:ring-2 focus-visible:ring-inset"
              style="height: {items.length * LROW + (!debounced && listNodes.length < listTotal ? LROW : 0)}px"
              onkeydown={onListKey}
            >
              {#each items.slice(lFirst, lLast + 1) as it, k (k + lFirst)}
                {@const i = lFirst + k}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  id="dl-{i}"
                  role="option"
                  aria-selected={i === listActive}
                  tabindex="-1"
                  class={cn('hover:bg-accent/60 absolute right-0 left-0 grid cursor-pointer content-center gap-0.5 border-b border-border/60 px-3', i === listActive && 'bg-accent/70', rec?.header.guid === it.guid && 'border-l-primary border-l-2')}
                  style="top: {i * LROW}px; height: {LROW}px"
                  onclick={() => {
                    listActive = i;
                    pick(it);
                  }}
                >
                  <span class={cn('mono truncate text-[12px]', !it.main && it.guid && 'text-muted-foreground')}>{it.name}</span>
                  <span class="text-muted-foreground truncate text-[11px]">{it.sub}</span>
                </div>
              {/each}
              {#if !debounced && listNodes.length < listTotal}
                <div class="absolute right-0 left-0 px-3 py-2" style="top: {items.length * LROW}px">
                  <Button variant="link" size="xs" class="h-auto p-0" onclick={() => void moreList()}>Show {int(Math.min(PAGE, listTotal - listNodes.length))} more</Button>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </section>
      {#if !narrow}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <div role="separator" aria-orientation="vertical" aria-label="Resize record list" aria-valuenow={listW} aria-valuemin={240} aria-valuemax={560} tabindex="0" class="hover:bg-primary/40 focus-visible:bg-primary/60 mx-1 w-1.5 shrink-0 cursor-col-resize rounded outline-none" onkeydown={(e) => resize('list', e)} onpointerdown={(e) => startDrag('list', e)} onpointermove={drag} onpointerup={() => (dragging = null)}></div>
      {:else}
        <div class="w-2 shrink-0"></div>
      {/if}
      {/if}

      <section data-pane="detail" class={cn('bg-card flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border', finding && 'min-w-[300px] basis-0')} aria-label="Record">
        {#if trigger && target}
          <TriggerPanel {target} name={trigger} />
        {:else if recError}
          <div class="p-3"><ErrorState title="Could not open the record" error={recError} /></div>
        {:else if rec}
          {@const h = rec.header}
          {@const own = dev.showIds ? null : (rec.labels[h.guid] ?? null)}
          <div class="grid grid-cols-[minmax(0,1fr)] gap-1.5 border-b px-3 py-2.5">
            <div class="flex flex-wrap items-center gap-2">
              {#if own && own.text !== h.name}
                <h2 class="flex min-w-0 flex-1 basis-56 items-baseline gap-2 truncate" title={h.name}>
                  <span class="truncate text-[14px] font-semibold">{own.text}</span>
                  <span class="text-muted-foreground mono truncate text-[12px]">{h.name}</span>
                </h2>
              {:else}
                <h2 class="mono min-w-0 flex-1 basis-56 truncate text-[13.5px] font-semibold" title={h.name}>{h.name}</h2>
              {/if}
              {#if recLoading}<LoaderCircle class="text-muted-foreground size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />{/if}
              {#if finding && variantGroup && Array.isArray(variantGroup.members) && variantGroup.members.some((m) => m.guid === h.guid)}
                <label class="text-muted-foreground flex items-center gap-1 text-[11.5px]" title="Records that read as {variantGroup.label}">Variant
                  <select class="border-input bg-background text-foreground mono h-6 max-w-[16rem] rounded-md border px-1 text-[11.5px]" value={h.guid} onchange={(e) => void openRecord(e.currentTarget.value)}>
                    {#each variantGroup.members as m (m.guid)}<option value={m.guid}>{m.short}{m.variant ? ' (AI/test)' : ''}</option>{/each}
                  </select>
                </label>
              {/if}
              <div class="flex flex-wrap gap-1.5">
                <ShowIdsToggle size="xs" />
                <Button variant="outline" size="xs" onclick={() => void exportRecord()}><Download aria-hidden="true" /> Export record…</Button>
                <Button variant="outline" size="xs" disabled={exporting} title="Export all {h.type} records" onclick={() => void askExportType()}><Download aria-hidden="true" /> Export type…</Button>
              </div>
            </div>
            <!-- The Model tab needs the height: the record's type, path and GUID stay with Data. -->
            <dl class={cn('text-muted-foreground grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-0.5 text-[11.5px]', recTab === 'model' && target && hasModel(h.type) && 'hidden')}>
              <dt>Type</dt><dd class="mono text-foreground truncate">{h.type}{h.main ? '' : ' (sub-record)'}{h.tag ? ` · tag ${h.tag}` : ''} · {bytes(h.bytes)} JSON</dd><span></span>
              <dt>Path</dt><dd class="mono truncate" title={h.path}>{h.path}</dd>
              <Button variant="ghost" size="icon-xs" aria-label="Copy path" title="Copy path" onclick={() => void copy(h.path, 'the path')}><Copy aria-hidden="true" /></Button>
              <dt>GUID</dt><dd class="mono truncate">{h.guid}</dd>
              <Button variant="ghost" size="icon-xs" aria-label="Copy GUID" title="Copy GUID" onclick={() => void copy(h.guid, 'the GUID')}><Copy aria-hidden="true" /></Button>
            </dl>
            {#if rec.truncated}<p class="text-[11.5px] text-amber-700 dark:text-amber-300">Large record: deeper parts load when you open them.</p>{/if}
            {#if target}<AssetPanel {target} guid={h.guid} name={h.name} type={h.type} />{/if}
          </div>
          {#if target && hasModel(h.type)}
            <div
              class="flex gap-1 border-b px-3 pt-1.5"
              role="tablist"
              aria-label="Record view"
              tabindex="-1"
              onkeydown={(e) => {
                if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
                e.preventDefault();
                recTab = recTab === 'data' ? 'model' : 'data';
                document.getElementById(`rec-tab-${recTab}`)?.focus();
              }}
            >
              {#each [{ id: 'data', label: 'Data' }, { id: 'model', label: 'Model' }] as t (t.id)}
                <button
                  type="button"
                  role="tab"
                  id="rec-tab-{t.id}"
                  aria-selected={recTab === t.id}
                  aria-controls="rec-panel-{t.id}"
                  tabindex={recTab === t.id ? 0 : -1}
                  class={cn('-mb-px rounded-t-md border border-transparent px-3 py-1 text-[12.5px]', recTab === t.id ? 'bg-card border-border border-b-card text-foreground font-medium' : 'text-muted-foreground hover:text-foreground')}
                  onclick={() => (recTab = t.id as 'data' | 'model')}
                >{t.label}</button>
              {/each}
            </div>
          {/if}
          {#if target && hasModel(h.type) && recTab === 'model'}
            <div id="rec-panel-model" role="tabpanel" aria-labelledby="rec-tab-model" class="flex min-h-0 flex-1 flex-col">
              {#await loadViewer()}
                <p class="text-muted-foreground flex items-center gap-2 p-4 text-[12.5px]"><LoaderCircle class="size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Loading the 3D viewer…</p>
              {:then m}
                {@const Viewer = m.default}
                <Viewer {target} guid={h.guid} name={h.name} />
              {:catch e}
                <div class="p-3"><ErrorState title="Could not load the 3D viewer" error={toContractError(e)} /></div>
              {/await}
            </div>
          {:else}
            {@const tabbed = !!target && hasModel(h.type)}
            <div id={tabbed ? 'rec-panel-data' : undefined} role={tabbed ? 'tabpanel' : undefined} aria-labelledby={tabbed ? 'rec-tab-data' : undefined} class="flex min-h-0 flex-1 flex-col">
              <JsonTree id="rec" json={rec.json} label={h.name} loc={rec.loc} labels={rec.labels} onfollow={follow} onload={loadAt} onloc={openLoc} class="flex-1" />
            </div>
          {/if}
        {:else}
          <p class="text-muted-foreground p-4 text-[12.5px]">Open a record from the list or the tree. References in a record are links; Alt+Left goes back.</p>
        {/if}
      </section>
    </div>
  </div>
{/if}

<Dialog.Root bind:open={exportOpen}>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>Export all {exportType?.name} records</Dialog.Title>
      <Dialog.Description>
        {int(exportType?.count ?? 0)} {mainOnly ? 'main ' : ''}records as one JSON array, in StarBreaker's own form. CIG's game data: for your own use.
      </Dialog.Description>
    </Dialog.Header>
    {#if (exportType?.count ?? 0) > EXPORT_WARN_RECORDS}
      <p class="text-[12.5px] text-amber-700 dark:text-amber-300">That is a lot of records: the file can be very large (EntityClassDefinition is about 1.3 GB). The export runs in the background and can be stopped.</p>
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (exportOpen = false)}>Cancel</Button>
      <Button onclick={() => void runExportType()}>Choose file…</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
