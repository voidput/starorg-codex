<script lang="ts">
  /**
   * Loot: what drops where, from the channel's cached loot data. Two views: by area (caves,
   * derelicts, ...: what can drop there) and by item (every area it drops in), each likeliest
   * first with rough odds per spawned box. Lazy: nothing loads before the screen opens; the
   * list call returns summaries only and a row's drops load when it is picked (stale answers
   * are ignored). Both lists are windowed. The backend builds the index once per build.
   */
  import { untrack } from 'svelte';
  import PackageOpen from '@lucide/svelte/icons/package-open';
  import Search from '@lucide/svelte/icons/search';
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
  import type { LootDetail, LootList, LootSummary, LootView } from '@/lib/contract/types';
  import { dropWhere, filterSummaries, groupsOf, listEntries, listed, oddsText, type ListEntry } from '@/lib/loot';
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

  let view = $state<LootView>(loadView());
  let tick = $state(0);
  // Each view's list with the stamp (channel, refresh, retry) it was read under: switching views reuses it.
  let lists = $state.raw<Record<LootView, { stamp: string; list: LootList } | null>>({ areas: null, items: null });
  let listError = $state<ContractError | null>(null);
  let query = $state('');
  let group = $state('');
  let picked = $state<Record<LootView, string | null>>({ areas: null, items: null });
  let detail = $state.raw<LootDetail | null>(null);
  let detailError = $state<ContractError | null>(null);
  let detailLoading = $state(false);

  const devOn = $derived(!!app.settings?.advanced.developer_mode);
  const stamp = $derived(`${app.targetKey}|${app.rev}|${tick}`);
  const list = $derived(lists[view]?.stamp === stamp ? lists[view]!.list : null);
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

  const rows = $derived(filterSummaries(list?.rows, query, group));
  const entries = $derived<ListEntry[]>(listEntries(rows, view));
  const groups = $derived([{ id: '', label: view === 'areas' ? 'All places' : 'All categories' }, ...groupsOf(list?.rows).map((g) => ({ id: g.group, label: `${g.group} (${g.count})` }))]);
  // The picked row while it is listed (a filter can hide it).
  const current = $derived(listed(Array.isArray(list?.rows) ? list.rows : [], picked[view]));
  const currentRow = $derived<LootSummary | null>(current === null ? null : (list?.rows.find((r) => r.id === current) ?? null));

  // Keyed on primitives only: the view, the picked id, readiness and the stamp. A newer pick
  // marks the older request stale, so a slow answer never replaces a newer one.
  $effect(() => {
    const v = view;
    const id = current;
    void stamp;
    if (id === null || !ready) {
      detail = null;
      detailLoading = false;
      return;
    }
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    detailLoading = true;
    detailError = null;
    api
      .lootDetail(target, v, id)
      .then((d) => {
        if (stale) return;
        detail = d;
      })
      .catch((e) => !stale && (detailError = toContractError(e)))
      .finally(() => !stale && (detailLoading = false));
    return () => (stale = true);
  });

  // The detail of the current pick only (never an older answer).
  const shownDetail = $derived(detail && detail.view === view && detail.entry?.id === current ? detail : null);

  function setView(v: LootView) {
    view = v;
    query = '';
    group = '';
    saveView(v);
  }
  function pick(id: string) {
    picked = { ...picked, [view]: id };
  }
  /** Jump to the other view with that row picked (its name in the search, so it is in view). */
  function jump(id: string, name: string) {
    const other: LootView = view === 'areas' ? 'items' : 'areas';
    picked = { ...picked, [other]: id };
    view = other;
    group = '';
    query = name;
    saveView(other);
  }
  function openRecord(guid: string) {
    dev.openRecord(guid, () => app.go('explorer'));
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

  const LROW = 44;
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
            {#if list}<span class="text-muted-foreground shrink-0" aria-live="polite">{int(rows.length)} of {int(list.rows.length)}</span>{/if}
          </div>
        </div>
        {#if !list}
          <div class="grid gap-1.5 p-2.5" aria-busy="true" aria-label="Loading">
            {#each Array.from({ length: 10 }, (_, i) => i) as i (i)}<Skeleton class="h-8 w-full" />{/each}
          </div>
        {:else if !rows.length}
          <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">{list.rows.length ? `Nothing matches${query ? ` "${query}"` : ''}.` : view === 'areas' ? 'No loot area in this build.' : 'No item drops in a loot area in this build.'}</p>
        {:else}
          <VirtualList
            items={entries}
            rowHeight={LROW}
            label={view === 'areas' ? 'Loot areas by kind of place' : 'Items that drop in loot'}
            resetKey={`${view}|${query}|${group}`}
            key={(e) => (e.kind === 'group' ? `g:${e.group}` : e.row.id)}
            class="h-[calc(100vh-19rem)] min-h-64 py-1"
          >
            {#snippet row(e)}
              {#if e.kind === 'group'}
                <h3 class="eyebrow text-muted-foreground flex h-full items-end px-3 pb-1.5">{e.group} <span class="mono ml-1.5 font-normal">{e.count}</span></h3>
              {:else}
                {@const r = e.row}
                <button
                  type="button"
                  aria-current={r.id === current ? 'true' : undefined}
                  class={cn(
                    'focus-visible:ring-ring/60 grid h-full w-full grid-cols-[minmax(0,1fr)_auto] content-center gap-x-2 px-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset',
                    r.id === current ? 'bg-primary/15 text-foreground' : 'hover:bg-muted',
                  )}
                  onclick={() => pick(r.id)}
                >
                  <span class={cn('truncate text-[13px]', r.id === current && 'font-medium')} title={r.name}>{r.name}</span>
                  <span class="mono text-muted-foreground text-[11px]" title="Best rough odds per box">{oddsText(r.best)}</span>
                  <span class="text-muted-foreground col-span-2 truncate text-[11px]">
                    {view === 'areas' ? `${int(r.count)} item${r.count === 1 ? '' : 's'}` : `${r.group} · ${int(r.count)} area${r.count === 1 ? '' : 's'}`}
                  </span>
                </button>
              {/if}
            {/snippet}
          </VirtualList>
        {/if}
      </section>

      <section class="bg-card grid min-w-0 gap-3 rounded-lg border p-3" aria-labelledby="loot-title" aria-busy={detailLoading}>
        {#if !currentRow}
          <p class="text-muted-foreground py-6 text-center">{view === 'areas' ? 'Pick an area to see what can drop there.' : 'Pick an item to see where it drops.'}</p>
        {:else}
          <header class="flex flex-wrap items-start gap-x-3 gap-y-1">
            <div class="grid min-w-0 flex-1 gap-0.5">
              <h2 id="loot-title" class="text-[16px] font-semibold">{currentRow.name}</h2>
              <p class="text-muted-foreground text-[12px]">
                {currentRow.group} · {view === 'areas' ? `${int(currentRow.count)} item${currentRow.count === 1 ? '' : 's'} can drop here` : `drops in ${int(currentRow.count)} area${currentRow.count === 1 ? '' : 's'}`} · best {oddsText(currentRow.best)} per box
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
          {:else}
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
              {:else if !shownDetail.rows.length}
                <p class="text-muted-foreground py-6 text-center text-[12px]">No drops listed.</p>
              {:else}
                <VirtualList items={shownDetail.rows} rowHeight={DROW} role="rowgroup" label="Drops, likeliest first" resetKey={`${view}|${current}`} key={(d, i) => `${d.id}/${i}`} class="max-h-[calc(100vh-21rem)] min-h-48">
                  {#snippet row(d)}
                    <div role="row" class={cn(DGRID, 'border-border/50 h-full border-b px-1')}>
                      <span role="cell" class="grid min-w-0">
                        <button
                          type="button"
                          class="text-primary focus-visible:ring-ring/60 truncate rounded-sm text-left underline-offset-2 outline-none hover:underline focus-visible:ring-2"
                          title={view === 'areas' ? `Where else ${d.name} drops` : `What else drops in ${d.name}`}
                          onclick={() => jump(d.id, d.name)}>{d.name}</button
                        >
                        <span class="text-muted-foreground truncate text-[11px]">{d.group}</span>
                      </span>
                      <span role="cell" class="text-muted-foreground truncate" title={dropWhere(d)}>{dropWhere(d)}</span>
                      <span role="cell" class="text-right font-medium tabular-nums">{oddsText(d.odds)}</span>
                    </div>
                  {/snippet}
                </VirtualList>
                {#if shownDetail.total > shownDetail.rows.length}
                  <p class="text-muted-foreground pt-2 text-[11.5px]">The likeliest {int(shownDetail.rows.length)} of {int(shownDetail.total)} shown.</p>
                {/if}
              {/if}
            </div>
          {/if}
        {/if}
      </section>
    </div>
  </div>
{/if}
