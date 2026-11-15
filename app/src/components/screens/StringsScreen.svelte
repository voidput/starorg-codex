<script lang="ts">
  /**
   * Strings (spec 9.4): every key of the channel (about 91,000) in a virtualised grid. Rows are
   * fetched in pages as they scroll into view, so the DOM holds about 40 rows at any time.
   * Search, source facets, category and "changed only" filter on the backend. Keyboard: arrows,
   * Page Up/Down, Home/End move the row; Enter opens the detail sheet.
   */
  import Search from '@lucide/svelte/icons/search';
  import { tick, untrack } from 'svelte';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import StringDetailSheet from '@/components/strings/StringDetailSheet.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { FacetCount, StringRow, StringSort, StringSource } from '@/lib/contract/types';
  import { CATEGORY_LABEL, int, SOURCE_LABEL, SOURCE_TONE } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  const ROW = 34;
  const PAGE = 200;
  const OVERSCAN = 8;

  let search = $state('');
  let debounced = $state('');
  let sources = $state<StringSource[]>([]);
  let category = $state<string>('all');
  let changedOnly = $state(false);
  let sort = $state<StringSort>('key');

  let total = $state<number | null>(null);
  let all = $state(0);
  let sourceFacets = $state<FacetCount[]>([]);
  let categoryFacets = $state<FacetCount[]>([]);
  let pages = $state<Record<number, StringRow[]>>({});
  let error = $state<ContractError | null>(null);
  let gen = 0;
  const loading = new Set<number>();

  let scroller = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(600);
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
  const qkey = $derived(JSON.stringify([tkey, debounced, sources, category, changedOnly, sort, app.rev]));

  function query(offset: number) {
    return {
      target: $state.snapshot(target!),
      search: debounced,
      sources: [...sources],
      categories: category === 'all' ? [] : [category],
      changed_only: changedOnly,
      sort,
      offset,
      limit: PAGE,
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
      categoryFacets = r.categories;
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
    if (!tkey) return;
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
  const catOptions = $derived([
    { id: 'all', label: 'All categories' },
    ...(categoryFacets.length ? categoryFacets : Object.keys(CATEGORY_LABEL).map((id) => ({ id, count: 0 }))).map((f) => ({
      id: f.id,
      label: `${CATEGORY_LABEL[f.id] ?? f.id}${f.count ? ` (${int(f.count)})` : ''}`,
    })),
  ]);
  const sorts: { id: StringSort; label: string }[] = [
    { id: 'key', label: 'Key A–Z' },
    { id: 'key-desc', label: 'Key Z–A' },
    { id: 'category', label: 'Category' },
    { id: 'source', label: 'Source' },
  ];
  const SRC: StringSource[] = ['unmodified', 'enhanced', 'patched', 'edited'];
  const cols = 'grid grid-cols-[minmax(180px,1.2fr)_minmax(0,1.4fr)_92px] xl:grid-cols-[minmax(220px,1.1fr)_minmax(0,1fr)_minmax(0,1fr)_96px_104px]';
</script>

<div class="flex h-full min-h-[420px] flex-col gap-3">
  <div class="flex flex-wrap items-center gap-x-3 gap-y-2" role="search" aria-label="Filter strings">
    <div class="relative w-full max-w-sm min-w-56 flex-1">
      <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
      <Input type="search" aria-label="Search keys and text" placeholder="Search keys and text" class="h-8 pl-8" bind:value={search} />
    </div>
    <SelectField ariaLabel="Category" options={catOptions} value={category} onchange={(v) => (category = v)} class="min-w-40" />
    <SelectField ariaLabel="Sort" options={sorts} value={sort} onchange={(v) => (sort = v)} />
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
          {SOURCE_LABEL[s]} <span class="mono text-[11px]">{int(facetCount(s))}</span>
        </button>
      {/each}
    </div>
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
      aria-colcount={5}
      tabindex="-1"
      class="bg-card flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border"
      onkeydown={onKey}
    >
      <div role="rowgroup" class="border-b">
        <div role="row" aria-rowindex={1} class={cn(cols, 'text-muted-foreground h-8 items-center gap-3 px-3 text-[12px] font-medium')}>
          <span role="columnheader">Key</span>
          <span role="columnheader" class="hidden xl:block">Original</span>
          <span role="columnheader">Result</span>
          <span role="columnheader">Source</span>
          <span role="columnheader" class="hidden xl:block">Category</span>
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
                tabindex={active === v.i ? 0 : -1}
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
                  <span role="gridcell" class="text-muted-foreground hidden truncate xl:block">{r.original}</span>
                  <span role="gridcell" class="truncate">{r.result}</span>
                  <span role="gridcell"><ToneBadge tone={SOURCE_TONE[r.source]} label={SOURCE_LABEL[r.source]} icon={null} /></span>
                  <span role="gridcell" class="text-muted-foreground hidden truncate text-[12px] xl:block">{CATEGORY_LABEL[r.category] ?? r.category}</span>
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

<StringDetailSheet bind:open={sheet} target={target} rowKey={openKey} onchanged={patchRow} />
