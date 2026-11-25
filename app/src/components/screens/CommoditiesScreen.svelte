<script lang="ts">
  /**
   * Commodities | Mining: one screen on two tabs (the sidebar's Commodities and Mining entries
   * open it on either; `?screen=mining` deep-links the second), sharing the search.
   *
   * Commodities: every tradable resource of the channel's build. Left, the list by group
   * (`Metal > Unrefined Ores`) with a search and group chips, windowed (only the rows in view
   * render); right, the picked resource in full from a second call (`commodity_detail`):
   * forms, quality, volatility, cargo, where it is mined and UEX kiosk prices.
   *
   * Mining: every mineable resource (`mining_list`), filtered by mining kind and sorted by
   * name or best quality, windowed the same way; right, one mineral (`mining_detail`): raw to
   * refined, quality per place, rock physics, difficulty and the laser estimate. Its list
   * loads the first time the tab opens, then stays for the build like the commodity list.
   *
   * Lazy: nothing loads until this screen opens; the list call returns summaries only, and a
   * detail loads per pick (a slower one never blocks the list; an answer for a row no longer
   * picked is dropped). The backend builds both from the build's cached game data once per
   * build, so switching screens does not rebuild.
   */
  import { untrack } from 'svelte';
  import Boxes from '@lucide/svelte/icons/boxes';
  import Search from '@lucide/svelte/icons/search';
  import Info from '@lucide/svelte/icons/info';
  import Download from '@lucide/svelte/icons/download';
  import DatabaseZap from '@lucide/svelte/icons/database-zap';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import CommodityDetailView from '@/components/commodities/CommodityDetailView.svelte';
  import MiningDetailView from '@/components/commodities/MiningDetailView.svelte';
  import Segmented from '@/components/common/Segmented.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { CommodityDirectory, CommodityInfo, MiningDirectory, MiningInfo } from '@/lib/contract/types';
  import { currentId, filterCommodities, flatRows, groupChips, stepId, visibleRange } from '@/lib/commodities';
  import { filterMinerals, GRADE_LABEL, gradeTone, KINDS, kindLabel, sortMinerals, type MineralSort, type MiningKind } from '@/lib/mining';
  import { pctText } from '@/lib/sources';
  import type { CommoditiesTab } from '@/lib/nav';
  import { uexAgeDays, uexAgeText } from '@/lib/shops';
  import { fetchUexNow } from '@/lib/uex';
  import { int } from '@/lib/format';
  import { toast } from '@/lib/stores/toast.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  let { tab = 'commodities' }: { tab?: CommoditiesTab } = $props();

  /** Fixed row height (px) of the windowed list: headers and rows alike. */
  const ROW = 30;
  const TABS: { id: CommoditiesTab; label: string; hint: string }[] = [
    { id: 'commodities', label: 'Commodities', hint: 'Every tradable resource: forms, quality, cargo, kiosk prices' },
    { id: 'mining', label: 'Mining', hint: 'Every mineable resource: places, quality by place, rock, difficulty, laser' },
  ];
  const SORTS: { id: MineralSort; label: string; hint: string }[] = [
    { id: 'name', label: 'Name', hint: 'By name' },
    { id: 'quality', label: 'Best quality', hint: 'Highest chance of a roll at 700 or more (best place) first' },
  ];

  let dir = $state<CommodityDirectory | null>(null);
  let error = $state<ContractError | null>(null);
  let tick = $state(0);
  let query = $state('');
  let group = $state('');
  let picked = $state<string | null>(null);
  let info = $state<CommodityInfo | null>(null);
  let infoError = $state<ContractError | null>(null);
  let infoLoading = $state(false);
  let fetching = $state(false);
  let scroller = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(480);

  // The Mining tab: its own list, pick and detail; the search box is shared.
  let mdir = $state<MiningDirectory | null>(null);
  let mError = $state<ContractError | null>(null);
  let mKind = $state<MiningKind>('');
  let mSort = $state<MineralSort>('name');
  let mPicked = $state<string | null>(null);
  let mInfo = $state<MiningInfo | null>(null);
  let mInfoError = $state<ContractError | null>(null);
  let mInfoLoading = $state(false);
  /** The Mining tab was opened once: its list loads from then on (per build, like the commodities). */
  let miningOpened = $state(false);
  let mScroller = $state<HTMLDivElement | null>(null);
  let mScrollTop = $state(0);
  let mViewH = $state(480);

  const uexOn = $derived(!!app.settings?.network.uex);
  $effect(() => {
    if (tab === 'mining') miningOpened = true;
  });

  // Keyed on primitives only: the refresh counter, the channel key and Reload.
  $effect(() => {
    void app.rev;
    void tick;
    const key = app.targetKey;
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    api
      .commodityList(key ? target : null)
      .then((d) => {
        if (stale) return;
        dir = d;
        error = null;
      })
      .catch((e) => !stale && (error = toContractError(e)));
    return () => (stale = true);
  });

  // The Mining list, once its tab was opened; keyed on primitives like the commodity list.
  $effect(() => {
    void app.rev;
    void tick;
    const key = app.targetKey;
    if (!miningOpened) return;
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    api
      .miningList(key ? target : null)
      .then((d) => {
        if (stale) return;
        mdir = d;
        mError = null;
      })
      .catch((e) => !stale && (mError = toContractError(e)));
    return () => (stale = true);
  });

  const ready = $derived(dir?.state === 'ready');
  const chips = $derived(groupChips(dir?.commodities));
  // A group the build does not have (another channel) shows everything.
  const activeGroup = $derived(group && chips.some((c) => c.group === group) ? group : '');
  const shown = $derived(filterCommodities(dir?.commodities, query, activeGroup));
  const rows = $derived(flatRows(shown));
  const win = $derived(visibleRange(scrollTop, viewH, ROW, rows.length));
  const current = $derived(currentId(shown, picked));

  const minerals = $derived(sortMinerals(filterMinerals(mdir?.minerals, query, mKind), mSort));
  const mWin = $derived(visibleRange(mScrollTop, mViewH, ROW, minerals.length));
  /** A mineral the Commodities tab asked for, until the Mining list is there. */
  let mPending = $state<string | null>(null);
  /** An id the Commodities tab asked for that the Mining list does not have. */
  let mMissing = $state<string | null>(null);
  const mCurrent = $derived(mMissing !== null || mPending !== null ? null : currentId(minerals, mPicked));
  const mReady = $derived(mdir?.state === 'ready');
  /** The Mining datasets this build has not read yet, for the top line. */
  const notRead = $derived(
    mdir?.state === 'ready'
      ? [!mdir.spawns_read && 'mining spawns', !mdir.spots_read && 'hand-placed spots', !mdir.physics && 'rock physics', !mdir.lasers_read && 'laser power'].filter((x): x is string => !!x)
      : [],
  );

  // A new search or group starts the list at the top.
  $effect(() => {
    void query;
    void activeGroup;
    void mKind;
    void mSort;
    untrack(() => {
      if (scroller) scroller.scrollTop = 0;
      scrollTop = 0;
      if (mScroller) mScroller.scrollTop = 0;
      mScrollTop = 0;
    });
  });

  // The mineral detail, as the commodity detail: an answer for a row no longer current is dropped.
  $effect(() => {
    void app.rev;
    void tick;
    const id = mCurrent;
    const key = app.targetKey;
    if (id === null || !mReady || tab !== 'mining') {
      if (id === null || !mReady) mInfo = null;
      mInfoLoading = false;
      return;
    }
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    mInfoLoading = true;
    api
      .miningDetail(key ? target : null, id)
      .then((r) => {
        if (stale) return;
        mInfo = r;
        mInfoError = null;
      })
      .catch((e) => !stale && (mInfoError = toContractError(e)))
      .finally(() => !stale && (mInfoLoading = false));
    return () => (stale = true);
  });
  const mineral = $derived(mInfo?.mineral && mInfo.mineral.id === mCurrent ? mInfo.mineral : null);

  // The detail of the current row; an answer for a row no longer current is dropped.
  $effect(() => {
    void app.rev;
    void tick;
    const id = current;
    const key = app.targetKey;
    if (id === null || !ready || tab !== 'commodities') {
      if (id === null || !ready) info = null;
      infoLoading = false;
      return;
    }
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    infoLoading = true;
    api
      .commodityDetail(key ? target : null, id)
      .then((r) => {
        if (stale) return;
        info = r;
        infoError = null;
      })
      .catch((e) => !stale && (infoError = toContractError(e)))
      .finally(() => !stale && (infoLoading = false));
    return () => (stale = true);
  });

  const detail = $derived(info?.commodity && info.commodity.id === current ? info.commodity : null);
  const age = $derived(uexAgeText(dir?.market.fetched));
  const ageDays = $derived(uexAgeDays(dir?.market.fetched));

  /** Keep row `id` in view. */
  function reveal(id: string) {
    const i = rows.findIndex((r) => r.kind === 'item' && r.item.id === id);
    if (i < 0 || !scroller) return;
    const top = i * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + scroller.clientHeight) scroller.scrollTop = top + ROW - scroller.clientHeight;
    scrollTop = scroller.scrollTop;
  }

  /** A form named in the detail: clear a search or group that hides it, then show it. */
  function pickRef(id: string) {
    if (!shown.some((r) => r.id === id)) {
      query = '';
      group = '';
    }
    picked = id;
    queueMicrotask(() => reveal(id));
  }

  /** Switch tabs; the sidebar follows (its Commodities and Mining entries are this screen). */
  function setTab(t: CommoditiesTab) {
    app.retab(t);
  }

  /** From the Mining tab: the resource on the Commodities tab. */
  function showCommodity(id: string) {
    setTab('commodities');
    pickRef(id);
  }

  /** Keep mineral `id` in view. */
  function mReveal(id: string) {
    const i = minerals.findIndex((r) => r.id === id);
    if (i < 0 || !mScroller) return;
    const top = i * ROW;
    if (top < mScroller.scrollTop) mScroller.scrollTop = top;
    else if (top + ROW > mScroller.scrollTop + mScroller.clientHeight) mScroller.scrollTop = top + ROW - mScroller.clientHeight;
    mScrollTop = mScroller.scrollTop;
  }

  /**
   * From the Commodities tab: the same resource on the Mining tab. Waits for the Mining list
   * (it loads when the tab first opens); a search or kind that hides it is cleared, and an id
   * the list does not have says so instead of showing another mineral.
   */
  function showMining(id: string) {
    mPending = id;
    mMissing = null;
    setTab('mining');
  }
  $effect(() => {
    const id = mPending;
    if (id === null || !mdir || mdir.state !== 'ready') return;
    untrack(() => {
      mPending = null;
      const low = id.toLowerCase();
      const hit = mdir!.minerals.find((r) => r.id.toLowerCase() === low);
      if (!hit) {
        mMissing = id;
        return;
      }
      if (!minerals.some((r) => r.id === hit.id)) {
        query = '';
        mKind = '';
      }
      mPicked = hit.id;
      queueMicrotask(() => mReveal(hit.id));
    });
  });

  function pickMineral(id: string) {
    mMissing = null;
    mPicked = id;
  }

  function onMiningKey(e: KeyboardEvent) {
    const by = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!by) return;
    e.preventDefault();
    const next = stepId(minerals, mCurrent, by);
    if (next) {
      pickMineral(next);
      mReveal(next);
    }
  }

  function onListKey(e: KeyboardEvent) {
    const by = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!by) return;
    e.preventDefault();
    const next = stepId(shown, current, by);
    if (next) {
      picked = next;
      reveal(next);
    }
  }

  function networkSettings() {
    app.settingsSection = 'network';
    app.go('settings');
  }
  async function fetchNow() {
    fetching = true;
    const s = await fetchUexNow();
    fetching = false;
    if (s) tick++;
  }

  // "Read game data" (a JobKind::Extract job): the commodities, mining and loot datasets come
  // with the Where to get data, under the memory limit; then the list reloads.
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

  const CHIP = 'focus-visible:ring-ring/60 inline-flex h-6 items-center gap-1.5 rounded-full border px-2 text-[12px] outline-none focus-visible:ring-2';
</script>

{#if error}
  <ErrorState title="Could not read the commodities" {error} onretry={() => tick++} />
{:else if !dir}
  <div class="grid gap-3 lg:grid-cols-[minmax(15rem,21rem)_minmax(0,1fr)]" aria-busy="true">
    <div class="grid gap-1.5">
      {#each Array(10) as _, i (i)}<Skeleton class="h-6 w-full" />{/each}
    </div>
    <Skeleton class="h-80 w-full" />
  </div>
{:else if dir.state === 'no-channel'}
  <FullEmpty icon={Boxes} title="No game channel">Commodities come from a channel's game data. Set up an install on Home first.</FullEmpty>
{:else if dir.state === 'not-read'}
  <FullEmpty icon={Boxes} title="Game data not read yet">
    The commodities, where they are mined and their quality come from the game data of {app.target?.channel ?? 'the channel'}. Nothing is written to the game; the first read of a build takes a minute or two.
    {#snippet actions()}
      <Button disabled={reading || !app.channel?.set_up} onclick={readGameData}>
        <DatabaseZap class={cn(reading && 'animate-pulse motion-reduce:animate-none')} aria-hidden="true" />
        {reading ? 'Reading game data…' : 'Read game data'}
      </Button>
    {/snippet}
  </FullEmpty>
{:else}
  <div class="grid gap-3">
    {#if tab === 'commodities'}
      <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px]" role="note">
        <Info class="size-3.5 shrink-0" aria-hidden="true" />
        <span>{int(dir.commodities.length)} commodities{dir.build ? ` · ${dir.build}` : ''}</span>
        <span aria-hidden="true">·</span>
        {#if dir.market.loaded}
          {#if age}<span class={cn(ageDays !== null && ageDays > 14 && 'text-warning')} title={dir.market.fetched ?? undefined}>{age}{ageDays !== null && ageDays > 14 ? ` (${ageDays} days old)` : ''}</span>{/if}
          <span>Kiosk prices are player-reported</span>
        {:else}
          <span>No UEX kiosk prices{uexOn ? '' : ' (Settings -> Network)'}</span>
        {/if}
        {#if uexOn}
          <Button variant="ghost" size="sm" class="ml-auto h-6" onclick={fetchNow} disabled={fetching}>
            {#if fetching}<LoaderCircle class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Download aria-hidden="true" />{/if}
            Fetch now
          </Button>
        {/if}
      </p>
    {:else}
      <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px]" role="note">
        <Info class="size-3.5 shrink-0" aria-hidden="true" />
        {#if mdir?.state === 'ready'}
          <span>{int(mdir.minerals.length)} mineable resources{mdir.build ? ` · ${mdir.build}` : ''}</span>
          <span aria-hidden="true">·</span>
          <span>Shares of rolls at 700 or more, difficulty and the ship comparison are estimates, with their inputs</span>
          {#if notRead.length}
            <span aria-hidden="true">·</span>
            <span class="text-warning">Not read yet: {notRead.join(', ')}</span>
            <Button variant="ghost" size="sm" class="ml-auto h-6" disabled={reading || !app.channel?.set_up} onclick={readGameData}>
              <DatabaseZap class={cn(reading && 'animate-pulse motion-reduce:animate-none')} aria-hidden="true" />
              {reading ? 'Reading game data…' : 'Read game data'}
            </Button>
          {/if}
        {:else}
          <span>Mining data</span>
        {/if}
      </p>
    {/if}

    <div class="grid items-start gap-3 lg:grid-cols-[minmax(15rem,21rem)_minmax(0,1fr)]">
      <section class="bg-card flex min-h-0 flex-col rounded-lg border" aria-labelledby="commodities-list-title">
        <h2 id="commodities-list-title" class="sr-only">{tab === 'mining' ? 'Mineable resources' : 'Commodities'}</h2>
        <div class="grid gap-2 border-b p-2.5">
          <Segmented size="sm" ariaLabel="Commodities or mining" class="justify-self-start" options={TABS} value={tab} onchange={setTab} />
          <div class="relative">
            <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
            <Input
              type="search"
              aria-label={tab === 'mining' ? 'Search minerals' : 'Search commodities'}
              placeholder={tab === 'mining' ? 'Search minerals or refined forms' : 'Search commodities or groups'}
              class="h-8 pl-8"
              value={query}
              oninput={(e) => (query = (e.currentTarget as HTMLInputElement).value)}
              onkeydown={(e) => e.key === 'Escape' && query && (e.preventDefault(), (query = ''))}
            />
          </div>
          {#if tab === 'commodities'}
            <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Group">
              <button
                type="button"
                aria-pressed={activeGroup === ''}
                class={cn(CHIP, activeGroup === '' ? 'border-primary bg-primary/15 text-foreground' : 'border-border hover:bg-muted text-muted-foreground hover:text-foreground')}
                onclick={() => (group = '')}><span class={cn(activeGroup === '' && 'font-medium')}>All</span></button
              >
              {#each chips as c (c.group)}
                {@const on = activeGroup === c.group}
                <button
                  type="button"
                  aria-pressed={on}
                  class={cn(CHIP, on ? 'border-primary bg-primary/15 text-foreground' : 'border-border hover:bg-muted text-muted-foreground hover:text-foreground')}
                  onclick={() => (group = on ? '' : c.group)}><span class={cn(on && 'font-medium')}>{c.group}</span><span class="mono text-muted-foreground text-[11px]">{c.count}</span></button
                >
              {/each}
              <span class="text-muted-foreground ml-auto text-[11.5px]" aria-live="polite">{shown.length} of {dir.commodities.length}</span>
            </div>
          {:else}
            <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Mining kind">
              {#each KINDS as k (k.id)}
                {@const on = mKind === k.id}
                <button
                  type="button"
                  aria-pressed={on}
                  class={cn(CHIP, on ? 'border-primary bg-primary/15 text-foreground' : 'border-border hover:bg-muted text-muted-foreground hover:text-foreground')}
                  onclick={() => (mKind = k.id)}><span class={cn(on && 'font-medium')}>{k.label}</span></button
                >
              {/each}
              <span class="text-muted-foreground ml-auto text-[11.5px]" aria-live="polite">{minerals.length} of {mdir?.minerals.length ?? 0}</span>
            </div>
            <Segmented size="sm" ariaLabel="Sort minerals" class="justify-self-start" options={SORTS} bind:value={mSort} />
          {/if}
        </div>
        {#if tab === 'commodities'}
          {#if !shown.length}
            <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">No commodity matches.</p>
          {:else}
            <!-- Windowed: only the rows in view (plus a few) are in the DOM. -->
            <div
              bind:this={scroller}
              bind:clientHeight={viewH}
              class="relative max-h-[calc(100vh-17rem)] min-h-48 overflow-y-auto outline-none"
              role="listbox"
              tabindex="0"
              aria-label="Commodities by group"
              aria-activedescendant={current ? `commodity-${current}` : undefined}
              onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
              onkeydown={onListKey}
            >
              <div class="relative" style:height="{rows.length * ROW}px">
                {#each rows.slice(win.first, win.last + 1) as r, k (r.key)}
                  {@const top = (win.first + k) * ROW}
                  {#if r.kind === 'header'}
                    <div class="eyebrow text-muted-foreground absolute inset-x-0 flex items-end px-3 pb-1" style:top="{top}px" style:height="{ROW}px" role="presentation">
                      <span class="truncate">{r.label}</span><span class="mono ml-1.5 font-normal">{r.count}</span>
                    </div>
                  {:else}
                    {@const on = r.item.id === current}
                    <div
                      id="commodity-{r.item.id}"
                      role="option"
                      aria-selected={on}
                      tabindex="-1"
                      class={cn('absolute inset-x-0 flex cursor-pointer items-center gap-2 px-3 text-left', on ? 'bg-primary/15 text-foreground' : 'hover:bg-muted')}
                      style:top="{top}px"
                      style:height="{ROW}px"
                      onclick={() => (picked = r.item.id)}
                      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), (picked = r.item.id))}
                    >
                      <span class={cn('min-w-0 flex-1 truncate text-[13px]', on && 'font-medium')} title={r.item.name}>{r.item.name}</span>
                      {#if r.item.volatile}<span class="text-warning shrink-0 text-[10.5px]" title="Volatile cargo">volatile</span>{/if}
                      {#if r.item.raw}<span class="text-muted-foreground shrink-0 text-[10.5px]" title="Refines into something">raw</span>{/if}
                    </div>
                  {/if}
                {/each}
              </div>
            </div>
          {/if}
        {:else if mError}
          <div class="p-3"><ErrorState title="Could not read the mining data" error={mError} onretry={() => tick++} /></div>
        {:else if !mdir}
          <div class="grid gap-1.5 p-3" aria-busy="true">
            {#each Array(8) as _, i (i)}<Skeleton class="h-6 w-full" />{/each}
          </div>
        {:else if !minerals.length}
          <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">{mdir.minerals.length ? 'No mineral matches.' : 'Not known: no mineable resource in this build\'s data.'}</p>
        {:else}
          <!-- Windowed: only the rows in view (plus a few) are in the DOM. -->
          <div
            bind:this={mScroller}
            bind:clientHeight={mViewH}
            class="relative max-h-[calc(100vh-19rem)] min-h-48 overflow-y-auto outline-none"
            role="listbox"
            tabindex="0"
            aria-label="Mineable resources"
            aria-activedescendant={mCurrent ? `mineral-${mCurrent}` : undefined}
            onscroll={() => (mScrollTop = mScroller?.scrollTop ?? 0)}
            onkeydown={onMiningKey}
          >
            <div class="relative" style:height="{minerals.length * ROW}px">
              {#each minerals.slice(mWin.first, mWin.last + 1) as r, k (r.id)}
                {@const top = (mWin.first + k) * ROW}
                {@const on = r.id === mCurrent}
                <div
                  id="mineral-{r.id}"
                  role="option"
                  aria-selected={on}
                  tabindex="-1"
                  class={cn('absolute inset-x-0 flex cursor-pointer items-center gap-2 px-3 text-left', on ? 'bg-primary/15 text-foreground' : 'hover:bg-muted')}
                  style:top="{top}px"
                  style:height="{ROW}px"
                  onclick={() => pickMineral(r.id)}
                  onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), pickMineral(r.id))}
                >
                  <span class={cn('min-w-0 flex-1 truncate text-[13px]', on && 'font-medium')} title={r.name}>{r.name}</span>
                  {#if r.difficulty}<span class={cn('shrink-0 text-[10.5px]', gradeTone(r.difficulty))} title="Hardest place (estimate)">{GRADE_LABEL[r.difficulty].toLowerCase()}</span>{/if}
                  <span class="text-muted-foreground shrink-0 text-[10.5px]" title="Mining kinds">{r.kinds.map(kindLabel).join(' ') || '–'}</span>
                  {#if mSort === 'quality'}<span class={cn('shrink-0 text-[10.5px] tabular-nums', r.best_high_uncertain && 'text-muted-foreground')} title={r.best_high_uncertain ? 'Best place: estimated share of rolls at 700 or more; only from places with a quality scale below 1 or an unknown curve, so it may be lower' : 'Best place: estimated share of rolls at 700 or more'}>{pctText(r.best_high)}{r.best_high_uncertain ? '*' : ''}</span>{/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </section>

      <section class="bg-card grid min-w-0 gap-3 rounded-lg border p-3" aria-busy={tab === 'mining' ? mInfoLoading : infoLoading} aria-live="polite">
        {#if tab === 'commodities'}
          {#if infoError}
            <ErrorState title="Could not read this commodity" error={infoError} onretry={() => tick++} />
          {:else if detail}
            <div class={cn('transition-opacity', infoLoading && 'opacity-60')}>
              <CommodityDetailView c={detail} {uexOn} {fetching} onpick={pickRef} onfetch={fetchNow} onnetwork={networkSettings} onmining={showMining} />
            </div>
          {:else if infoLoading || (current && !info)}
            <div class="grid gap-3" aria-label="Loading">
              <Skeleton class="h-6 w-1/2" />
              <Skeleton class="h-4 w-1/3" />
              {#each Array(5) as _, i (i)}<Skeleton class="h-14 w-full" />{/each}
            </div>
          {:else if current}
            <p class="text-muted-foreground py-6 text-center text-[12px]">Not known: this build has no such commodity.</p>
          {:else}
            <p class="text-muted-foreground py-6 text-center">Pick a commodity.</p>
          {/if}
        {:else if mMissing !== null}
          <p class="text-muted-foreground py-6 text-center text-[12px]">Not found: <span class="mono">{mMissing}</span> is not a mineable resource of this build. Pick one from the list.</p>
        {:else if mInfoError}
          <ErrorState title="Could not read this mineral" error={mInfoError} onretry={() => tick++} />
        {:else if mineral && mdir}
          <div class={cn('transition-opacity', mInfoLoading && 'opacity-60')}>
            <MiningDetailView m={mineral} lasers={mdir.lasers} physics={mdir.physics} lasersRead={mdir.lasers_read} spawnsRead={mdir.spawns_read} spotsRead={mdir.spots_read} oncommodity={showCommodity} />
          </div>
        {:else if mInfoLoading || (mCurrent && !mInfo) || !mdir}
          <div class="grid gap-3" aria-label="Loading">
            <Skeleton class="h-6 w-1/2" />
            <Skeleton class="h-4 w-1/3" />
            {#each Array(5) as _, i (i)}<Skeleton class="h-14 w-full" />{/each}
          </div>
        {:else if mCurrent}
          <p class="text-muted-foreground py-6 text-center text-[12px]">Not known: this build has no such mineral.</p>
        {:else}
          <p class="text-muted-foreground py-6 text-center">Pick a mineral.</p>
        {/if}
      </section>
    </div>
  </div>
{/if}
