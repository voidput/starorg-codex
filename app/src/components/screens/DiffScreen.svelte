<script lang="ts">
  /**
   * Diff (addendum 7.6, 7.8): pick A and B among the dataset sources (two builds, two folders,
   * or a folder against this build), then a summary per table (added / removed / changed /
   * same, largest first; tables on one side marked), the records of the picked table (changed
   * first; filter by state and by changed field; identity column override) and the field diff
   * of the picked record (old ► new, numbers with delta and percent). F6 cycles the panes.
   * Records are titled with the item's display name and fields read as names, text and numbers
   * with units on both sides, grouped by item (7.14); "Show IDs" switches back to raw ids, and
   * the record filter matches names and ids.
   */
  import { untrack } from 'svelte';
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import SelectField from '@/components/common/SelectField.svelte';
  import Segmented from '@/components/common/Segmented.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import JobProgress from '@/components/common/JobProgress.svelte';
  import Copy from '@lucide/svelte/icons/copy';
  import Search from '@lucide/svelte/icons/search';
  import { Input } from '@/components/ui/input';
  import ShowIdsToggle from '@/components/dev/ShowIdsToggle.svelte';
  import { changeTexts, groupChanges, titled } from '@/lib/readable';
  import { dev } from '@/lib/stores/dev.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { devApi, type DevDiffRecords, type DevDiffSummary, type DevFieldDiff, type DevSource, type DiffTable, type RecordState } from '@/lib/contract/dev';
  import { toContractError, type ContractError } from '@/lib/contract/client';
  import { app } from '@/lib/stores/app.svelte';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';

  const target = $derived(app.target);
  const tkey = $derived(app.targetKey);
  let sources = $state<DevSource[]>([]);
  let a = $state<string | null>(null);
  let b = $state<string | null>(null);
  let summary = $state<DevDiffSummary | null>(null);
  let error = $state<ContractError | null>(null);
  let running = $state(false);

  $effect(() => {
    void tkey;
    untrack(async () => {
      if (!target) return;
      try {
        sources = (await devApi.sources($state.snapshot(target))).filter((s) => s.state === 'ready');
        a ??= sources[0]?.id ?? null;
        b ??= sources.find((s) => s.id !== a)?.id ?? null;
        summary = await devApi.diffSummary($state.snapshot(target)).catch(() => null);
        if (summary) {
          a = summary.a;
          b = summary.b;
        }
      } catch (e) {
        error = toContractError(e);
      }
    });
  });
  const options = $derived(sources.map((s) => ({ id: s.id, label: s.label })));
  async function compare() {
    if (!target || !a || !b) return;
    running = true;
    picked = null;
    records = null;
    field = null;
    try {
      summary = await devApi.diff({ target: $state.snapshot(target), a, b });
      error = null;
      const firstChanged = summary.tables.find((t) => t.added + t.removed + t.changed > 0);
      if (firstChanged) void pickTable(firstChanged);
    } catch (e) {
      error = toContractError(e);
    } finally {
      running = false;
    }
  }

  // --- Records of the picked table --------------------------------------------------------
  let picked = $state<DiffTable | null>(null);
  let records = $state<DevDiffRecords | null>(null);
  let stateFilter = $state<'all' | RecordState>('all');
  let fieldFilter = $state<string>('');
  let identity = $state<string | null>(null);
  let recQuery = $state('');
  let recQueryDebounced = $state('');
  $effect(() => {
    const q = recQuery;
    const t = setTimeout(() => (recQueryDebounced = q.trim()), 200);
    return () => clearTimeout(t);
  });
  let recError = $state<ContractError | null>(null);
  let loadingMore = $state(false);
  // Each new query bumps this, so a page that lands after a filter change is dropped.
  let recSeq = 0;
  const PAGE = 500;
  async function loadRecords(more = false) {
    if (!target || !picked) return;
    const seq = more ? recSeq : ++recSeq;
    const offset = more && records ? records.records.length : 0;
    if (more) loadingMore = true;
    try {
      const page = await devApi.diffRecords({
        target: $state.snapshot(target),
        dataset: picked.dataset,
        table: picked.table,
        state: stateFilter === 'all' ? null : stateFilter,
        field: fieldFilter || null,
        identity,
        query: recQueryDebounced || null,
        offset,
        limit: PAGE,
      });
      if (seq !== recSeq) return;
      records = more && records ? { ...page, records: [...records.records, ...page.records] } : page;
      recError = null;
    } catch (e) {
      if (seq === recSeq) recError = toContractError(e);
    } finally {
      if (more) loadingMore = false;
    }
  }
  async function pickTable(t: DiffTable) {
    picked = t;
    stateFilter = 'all';
    fieldFilter = '';
    identity = null;
    recQuery = '';
    recQueryDebounced = '';
    field = null;
    await loadRecords();
  }
  $effect(() => {
    void stateFilter;
    void fieldFilter;
    void identity;
    void recQueryDebounced;
    untrack(() => void loadRecords());
  });

  // --- Field diff --------------------------------------------------------------------------
  let field = $state<DevFieldDiff | null>(null);
  async function pickRecord(id: string) {
    if (!target || !picked) return;
    try {
      field = await devApi.diffRecord({ target: $state.snapshot(target), dataset: picked.dataset, table: picked.table, id, identity });
    } catch (e) {
      recError = toContractError(e);
    }
  }
  const show = (v: unknown) => (v === null || v === undefined ? '—' : typeof v === 'string' ? JSON.stringify(v) : JSON.stringify(v));
  const pct = (p: number | null) => (p === null ? '' : ` (${p > 0 ? '+' : ''}${p.toFixed(1)} %)`);
  const delta = (d: number | null) => (d === null ? '' : `${d > 0 ? '+' : ''}${Number.isInteger(d) ? d : d.toFixed(3)}`);
  const label = (id: string) => sources.find((s) => s.id === id)?.label ?? id;
  const deltaUnit = (d: number | null, unit: string | null) => (d === null ? '' : `${delta(d)}${unit && !dev.showIds ? ` ${unit}` : ''}`);
  async function copyId(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast('success', 'Copied the id');
    } catch {
      toast('error', 'The clipboard is not available here');
    }
  }

  // Lists: arrows move, Enter picks.
  let tableActive = $state(0);
  let recActive = $state(0);
  function listKey(e: KeyboardEvent, n: number, cur: number, set: (i: number) => void, enter: (i: number) => void) {
    const map: Record<string, number> = { ArrowDown: cur + 1, ArrowUp: cur - 1, Home: 0, End: n - 1 };
    if (e.key in map) {
      e.preventDefault();
      const i = Math.max(0, Math.min(n - 1, map[e.key]!));
      set(i);
      (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-i="${i}"]`)?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter') {
      e.preventDefault();
      enter(cur);
    }
  }
  let rootEl = $state<HTMLElement | null>(null);
  function onF6(e: KeyboardEvent) {
    if (e.key !== 'F6' || !rootEl) return;
    const stops = [...rootEl.querySelectorAll<HTMLElement>('[data-stop]')].filter((x) => x.offsetParent !== null);
    if (!stops.length) return;
    e.preventDefault();
    const at = stops.findIndex((x) => x.contains(document.activeElement));
    stops[at < 0 ? 0 : (at + (e.shiftKey ? stops.length - 1 : 1)) % stops.length]!.focus();
  }
</script>

{#if !target}
  <FullEmpty icon={GitCompare} title="No channel">Pick a game channel first.</FullEmpty>
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div bind:this={rootEl} class="flex h-full min-h-[420px] flex-col gap-3" onkeydown={onF6}>
    <div class="flex flex-wrap items-center gap-2" data-stop tabindex="-1">
      <span class="text-[12.5px] font-medium">A</span>
      <SelectField ariaLabel="Source A" {options} value={a} placeholder="Source…" onchange={(v) => (a = v)} class="min-w-44" />
      <ArrowRight class="text-muted-foreground size-4" aria-hidden="true" />
      <span class="text-[12.5px] font-medium">B</span>
      <SelectField ariaLabel="Source B" {options} value={b} placeholder="Source…" onchange={(v) => (b = v)} class="min-w-44" />
      <Button size="sm" disabled={!a || !b || a === b || running} onclick={() => void compare()}><GitCompare aria-hidden="true" /> {running ? 'Comparing…' : 'Compare'}</Button>
      {#if app.job?.kind === 'dev'}<JobProgress job={app.job} compact />{/if}
      <span class="flex-1"></span>
      <ShowIdsToggle />
      {#if summary}<span class="text-muted-foreground text-[12px]">{label(summary.a)} → {label(summary.b)} · {int(summary.tables.length)} tables · {summary.ms} ms</span>{/if}
    </div>
    {#if sources.length < 2}
      <p class="text-muted-foreground text-[12.5px]">Two sources are needed: read another channel's game data (apply or a dry run), or add a dataset folder on the Datasets screen.</p>
    {/if}
    {#if error}
      <ErrorState title="Could not compare" {error} />
    {:else if !summary}
      <FullEmpty icon={GitCompare} title="Pick A and B">Two builds (LIVE against PTU), two folders, or a folder against this build. Envelope fields like _extracted never count as a change.</FullEmpty>
    {:else}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        role="listbox"
        aria-label="Tables"
        tabindex="0"
        data-stop
        aria-activedescendant="dt-row-{tableActive}"
        class="bg-card focus-visible:ring-ring/60 max-h-[38%] min-h-24 shrink-0 overflow-auto rounded-lg border outline-none focus-visible:ring-2"
        onkeydown={(e) => listKey(e, summary!.tables.length, tableActive, (i) => (tableActive = i), (i) => void pickTable(summary!.tables[i]!))}
      >
        <div class="text-muted-foreground bg-card sticky top-0 grid grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)_repeat(4,72px)_minmax(0,0.8fr)] gap-3 border-b px-3 py-1.5 text-[11.5px] font-medium" aria-hidden="true">
          <span>Dataset</span><span>Table</span><span class="text-right">Added</span><span class="text-right">Removed</span><span class="text-right">Changed</span><span class="text-right">Same</span><span>Matched by</span>
        </div>
        {#each summary.tables as t, i (t.dataset + '/' + t.table)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id="dt-row-{i}"
            data-i={i}
            role="option"
            aria-selected={picked?.dataset === t.dataset && picked?.table === t.table}
            tabindex="-1"
            class={cn('hover:bg-accent/60 grid cursor-pointer grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)_repeat(4,72px)_minmax(0,0.8fr)] items-center gap-3 border-b border-border/60 px-3 py-1 text-[12px]', i === tableActive && 'bg-accent/70', picked?.table === t.table && picked?.dataset === t.dataset && 'text-primary')}
            onclick={() => {
              tableActive = i;
              void pickTable(t);
            }}
          >
            <span class="mono truncate">{t.dataset}</span>
            <span class="mono truncate">{t.table}{t.presence !== 'both' ? (t.presence === 'only-a' ? ' (only in A)' : ' (only in B)') : ''}</span>
            <span class="mono text-right">{int(t.added)}</span>
            <span class="mono text-right">{int(t.removed)}</span>
            <span class="mono text-right">{int(t.changed)}</span>
            <span class="text-muted-foreground mono text-right">{int(t.same)}</span>
            <span class="text-muted-foreground mono truncate">{t.identity ?? 'position'}</span>
          </div>
        {/each}
      </div>

      {#if picked}
        <div class="flex min-h-0 flex-1 gap-3">
          <section class="bg-card flex w-[min(420px,45%)] shrink-0 flex-col overflow-hidden rounded-lg border" aria-label="Records">
            <div class="flex flex-wrap items-center gap-2 border-b px-2 py-1.5">
              <Segmented size="sm" ariaLabel="Records" options={[{ id: 'all', label: 'All' }, { id: 'changed', label: 'Changed' }, { id: 'added', label: 'Added' }, { id: 'removed', label: 'Removed' }] as { id: 'all' | RecordState; label: string }[]} bind:value={stateFilter} />
              {#if records?.fields.length}
                <SelectField ariaLabel="Changed field" size="sm" options={[{ id: '', label: 'Any field' }, ...records.fields.map((f) => ({ id: f.value, label: `${f.value} (${int(f.count)})` }))]} value={fieldFilter} onchange={(v) => (fieldFilter = v)} class="min-w-32" />
              {/if}
              <div class="relative min-w-36 flex-1">
                <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2" aria-hidden="true" />
                <Input type="search" aria-label="Filter records by name or id" placeholder="Name or id" class="h-7 pl-7 text-[12px]" bind:value={recQuery} />
              </div>
              {#if records?.identity_options.length}
                <SelectField ariaLabel="Match records by" size="sm" options={[{ id: '', label: `Match: ${records.summary.identity ?? 'position'}` }, ...records.identity_options.map((c) => ({ id: c, label: `Match by ${c}` }))]} value={identity ?? ''} onchange={(v) => (identity = v || null)} class="min-w-36" />
              {/if}
            </div>
            {#if recError}
              <div class="p-2"><ErrorState title="Could not list records" error={recError} /></div>
            {:else if !records}
              <div class="grid gap-1 p-2">{#each { length: 6 } as _, i (i)}<Skeleton class="h-6 w-full" />{/each}</div>
            {:else if !records.records.length}
              <p class="text-muted-foreground p-3 text-[12.5px]">No record differs here.</p>
            {:else}
              <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
              <ul
                role="listbox"
                aria-label="Changed records"
                tabindex="0"
                data-stop
                aria-activedescendant="dr-{recActive}"
                class="focus-visible:ring-ring/60 min-h-0 flex-1 overflow-y-auto outline-none focus-visible:ring-2 focus-visible:ring-inset"
                onkeydown={(e) => listKey(e, records!.records.length, recActive, (i) => (recActive = i), (i) => void pickRecord(records!.records[i]!.id))}
              >
                {#each records.records as r, i (r.id + r.state)}
                  {@const t = titled(r.id, r.label, dev.showIds)}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <li
                    id="dr-{i}"
                    data-i={i}
                    role="option"
                    aria-selected={field?.id === r.id}
                    tabindex="-1"
                    class={cn('hover:bg-accent/60 grid cursor-pointer gap-0.5 border-b border-border/60 px-3 py-1.5', i === recActive && 'bg-accent/70')}
                    onclick={() => {
                      recActive = i;
                      void pickRecord(r.id);
                    }}
                  >
                    <span class="flex min-w-0 items-baseline gap-2" title={r.label ? `${r.label}\n${r.id}` : r.id}>
                      <span class={cn('shrink-0 rounded px-1 text-[10.5px] font-semibold uppercase', r.state === 'added' ? 'bg-success/15 text-success' : r.state === 'removed' ? 'bg-destructive/15 text-destructive' : 'bg-warning/15 text-warning')}>{r.state}</span>
                      <span class={cn('min-w-0 truncate text-[12px]', !t.secondary && 'mono', t.secondary && 'font-medium')}>{t.primary}</span>
                      {#if t.secondary}<span class="text-muted-foreground mono min-w-0 shrink-[2] truncate text-[10.5px]">{t.secondary}</span>{/if}
                    </span>
                    {#if r.fields.length}<span class="text-muted-foreground mono truncate text-[11px]">{r.fields.join(', ')}</span>{/if}
                  </li>
                {/each}
              </ul>
              <div class="text-muted-foreground flex items-center justify-between gap-2 border-t px-3 py-1 text-[11.5px]">
                <span>{int(records.records.length)} of {int(records.total)}</span>
                {#if records.records.length < records.total}
                  <Button size="sm" variant="ghost" class="h-6 px-2 text-[11.5px]" disabled={loadingMore} onclick={() => void loadRecords(true)}>{loadingMore ? 'Loading…' : `Load ${int(Math.min(PAGE, records.total - records.records.length))} more`}</Button>
                {/if}
              </div>
            {/if}
          </section>
          <section class="bg-card flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border" aria-label="Field changes">
            {#if field}
              {@const t = titled(field.id, field.label, dev.showIds)}
              <div class="flex items-center gap-2 border-b px-3 py-2">
                <h2 class="flex min-w-0 flex-1 items-baseline gap-2 truncate" title={field.label ? `${field.label}\n${field.id}` : field.id}>
                  <span class={cn('truncate text-[13px] font-semibold', !t.secondary && 'mono')}>{t.primary}</span>
                  {#if t.secondary}<span class="text-muted-foreground mono truncate text-[11.5px]">({t.secondary})</span>{/if}
                </h2>
                <Button variant="ghost" size="icon-xs" aria-label="Copy id {field.id}" title="Copy the id" onclick={() => void copyId(field!.id)}><Copy aria-hidden="true" /></Button>
                <span class="text-muted-foreground text-[12px]">{int(field.changes.length)} {field.changes.length === 1 ? 'field' : 'fields'}</span>
              </div>
              <div class="min-h-0 flex-1 overflow-auto" data-stop tabindex="-1">
                <table class="w-full text-[12px]">
                  <thead class="text-muted-foreground bg-card sticky top-0 text-left text-[11.5px]">
                    <tr><th class="px-3 py-1.5 font-medium">Field</th><th class="px-3 py-1.5 font-medium">{label(summary.a)}</th><th class="px-3 py-1.5 font-medium">{label(summary.b)}</th><th class="px-3 py-1.5 font-medium">Δ</th></tr>
                  </thead>
                  {#each groupChanges(field.changes) as g, gi (gi)}
                    <tbody>
                      {#if g.group && !dev.showIds}
                        <tr class="bg-muted/40 border-t border-border/60"><th colspan="4" scope="rowgroup" class="px-3 py-1 text-left text-[11.5px] font-semibold">{g.group}</th></tr>
                      {/if}
                      {#each g.items as c (c.path)}
                        {@const [o, n] = changeTexts(c, dev.showIds)}
                        {@const path = !dev.showIds && c.path_label ? c.path_label : c.path}
                        <tr class="border-t border-border/60 align-top">
                          <td class="px-3 py-1 break-all" title={c.path}>
                            <span class={cn(path === c.path && 'mono')}>{path}</span>
                            {#if path !== c.path}<span class="text-muted-foreground mono block text-[10.5px]">{c.path}</span>{/if}
                          </td>
                          <td class="text-destructive px-3 py-1 break-all" title={o.title}>
                            <span class={cn(o.raw && 'mono')}>{o.primary}</span>{#if o.enhanced}<span class="text-primary ml-1 text-[10px] font-semibold uppercase">enh</span>{/if}
                            {#if o.secondary}<span class="text-muted-foreground mono block text-[10.5px]">{o.secondary}</span>{/if}
                          </td>
                          <td class="text-success px-3 py-1 break-all" title={n.title}>
                            <span class={cn(n.raw && 'mono')}>{n.primary}</span>{#if n.enhanced}<span class="text-primary ml-1 text-[10px] font-semibold uppercase">enh</span>{/if}
                            {#if n.secondary}<span class="text-muted-foreground mono block text-[10.5px]">{n.secondary}</span>{/if}
                          </td>
                          <td class="mono text-muted-foreground px-3 py-1 whitespace-nowrap">{deltaUnit(c.delta, c.unit)}{pct(c.percent)}</td>
                        </tr>
                      {/each}
                    </tbody>
                  {/each}
                </table>
              </div>
            {:else}
              <p class="text-muted-foreground p-4 text-[12.5px]">Pick a record to see what changed.</p>
            {/if}
          </section>
        </div>
      {/if}
    {/if}
  </div>
{/if}
