<script lang="ts">
  /**
   * Blueprint finder (addendum §7.19): every blueprint of the channel's build (or of all channels)
   * with who owns it, searchable, filterable by facets with counts (category, type, maker, size,
   * source, character, missing for, channel, missions, reputation), sortable and groupable. Filters, sort
   * and scope are remembered per channel (localStorage, validated on read). Ownership stays per
   * channel and per character: the "All channels" view only puts the rows side by side, each owner
   * with its channel badge. "Earn it" opens under a row: the missions that award the blueprint
   * (computed once per build with the catalogue, never per render).
   */
  import { characterLabel } from '@/lib/characters';
  import Search from '@lucide/svelte/icons/search';
  import Star from '@lucide/svelte/icons/star';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import X from '@lucide/svelte/icons/x';
  import Info from '@lucide/svelte/icons/info';
  import ListFilter from '@lucide/svelte/icons/list-filter';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ChevronsDownUp from '@lucide/svelte/icons/chevrons-down-up';
  import ChevronsUpDown from '@lucide/svelte/icons/chevrons-up-down';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import { tick, untrack } from 'svelte';
  import { getCardCollapse } from '@/components/ui/card/collapse';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import Segmented from '@/components/common/Segmented.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import ChannelBadge from '@/components/common/ChannelBadge.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { BlueprintEarn, BlueprintFinder, BlueprintOwner, BlueprintRow, ChannelRef } from '@/lib/contract/types';
  import {
    FACETS,
    GROUPS,
    PREFS_KEY,
    SORTS,
    acquired,
    activeFilters,
    clearFilters,
    earnFor,
    missionOf,
    missionTitle,
    kindLabel,
    hiddenMatches,
    makerLabel,
    pageRows,
    parsePrefs,
    query,
    sourceText,
    toggle,
    UNATTRIBUTED,
    type FacetId,
    type FinderPrefs,
    type OwnedFilter,
    type SortKey,
  } from '@/lib/bpfinder';
  import { GROUPS_KEY, arrowToggle, readCollapsed, withCollapsed, writeCollapsed } from '@/lib/collapse';
  import { fmtDate } from '@/lib/format';
  import { cn } from '@/lib/utils';
  import { toast } from '@/lib/stores/toast.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { dev } from '@/lib/stores/dev.svelte';

  let {
    target,
    channels,
    rev,
    picked = $bindable(null),
  }: {
    target: ChannelRef;
    /** Channel names of the install, in its order. */
    channels: string[];
    /** Changes after an import, pull or rescan (not on every app refresh): the finder reloads. */
    rev: string;
    /** The first picked character (the import dialog starts there). */
    picked?: string | null;
  } = $props();

  const tkey = $derived(PREFS_KEY(target.install, target.channel));
  let prefs = $state<FinderPrefs>(parsePrefs(null));
  let prefsFor = $state('');
  $effect(() => {
    const k = tkey;
    if (k === prefsFor) return;
    untrack(() => {
      try {
        prefs = parsePrefs(localStorage.getItem(k));
      } catch {
        prefs = parsePrefs(null);
      }
      prefsFor = k;
      limit = PAGE;
    });
  });
  $effect(() => {
    const v = JSON.stringify(prefs);
    if (!prefsFor) return;
    try {
      localStorage.setItem(prefsFor, v);
    } catch {
      /* storage blocked: prefs last for this session only */
    }
  });

  // Raw: the payload is replaced whole on every load, never changed in place (no deep proxy).
  let data = $state.raw<BlueprintFinder | null>(null);
  let failed = $state<string | null>(null);
  const scopeAll = $derived(prefs.scope === 'all' && channels.length > 1);
  const fetchKey = $derived(prefsFor === tkey ? `${tkey}/${scopeAll ? '*' : target.channel}/${rev}` : '');
  $effect(() => {
    if (!fetchKey) return;
    // Keyed on `fetchKey` only: `target` is a new object after every app refresh.
    const req = untrack(() => ({ install: target.install, channels: scopeAll ? [] : [target.channel] }));
    let stale = false;
    failed = null;
    api
      .blueprintFinder(req)
      .then((d) => !stale && (data = d))
      .catch((e) => {
        if (stale) return;
        failed = toContractError(e).message;
        toast('error', 'Could not read the blueprints', failed);
      });
    return () => (stale = true);
  });

  const inView = $derived(scopeAll ? channels : [target.channel]);
  // Two characters with one name (a reset the logs cannot tie together) show newest / last seen.
  const charName = (id: string) => {
    const c = data?.characters.find((x) => x.id === id);
    return c ? characterLabel(c, data?.characters ?? []) : '—';
  };
  const v = $derived(data ? query(data, prefs, { channels: inView, characterName: charName }) : null);
  const notRead = $derived((data?.channels ?? []).filter((c) => c.catalog === null && inView.includes(c.channel)).map((c) => c.channel));
  const catalogSize = $derived((data?.channels ?? []).find((c) => c.channel === target.channel)?.catalog ?? null);
  const nActive = $derived(activeFilters(prefs, scopeAll));
  // Only a character of this channel can take an import (not a PTU-only one, not "unattributed").
  $effect(() => {
    picked = prefs.character.find((c) => data?.characters.some((x) => x.id === c && x.id !== UNATTRIBUTED && x.channels.includes(target.channel))) ?? null;
  });
  const skipped = $derived((data?.channels ?? []).filter((c) => inView.includes(c.channel)).reduce((n, c) => n + c.skipped, 0));
  const nameOfKey = (k: string) => data?.rows.find((x) => x.key === k);
  const candText = (k: string) => {
    const c = nameOfKey(k);
    return c ? [c.name, c.type_label, c.class ?? (c.size !== null ? `S${c.size}` : null), c.record].filter(Boolean).join(' · ') : k;
  };

  const PAGE = 200;
  let limit = $state(PAGE);
  let showFacets = $state(true);
  let more = $state<Record<string, boolean>>({});
  let searchEl = $state<HTMLInputElement | null>(null);

  const set = (patch: Partial<FinderPrefs>) => {
    prefs = { ...prefs, ...patch };
    limit = PAGE;
  };
  const flip = (f: FacetId, id: string) => {
    prefs = toggle(prefs, f, id);
    limit = PAGE;
  };
  function sortBy(s: SortKey) {
    if (prefs.sort === s) set({ desc: !prefs.desc });
    else set({ sort: s, desc: s === 'acquired' || s === 'owners' });
  }

  const FACET_LABEL: Record<FacetId, string> = {
    kind: 'Category',
    type: 'Type',
    manufacturer: 'Manufacturer',
    size: 'Size',
    source: 'Source',
    character: 'Character',
    missing: 'Missing for',
    channel: 'Channel',
    earn: 'Missions',
    faction: 'Reputation',
  };
  const FACET_MAX = 8;
  /** Values worth a chip: with matches, or picked. */
  const chips = (f: FacetId) => (v?.facets[f] ?? []).filter((x) => x.count > 0 || x.on);
  const facetShown = (f: FacetId) =>
    f === 'channel' && !scopeAll
      ? false
      : // A stored pick always shows (it filters), else only with two characters to compare.
        f === 'missing'
        ? prefs.missing.length > 0 || ((data?.characters.length ?? 0) > 1 && chips(f).length > 0)
        : f === 'type'
          ? chips('type').length > 1
          : chips(f).length > 0;

  // "Earn it": the rows whose mission list is open (not remembered), and the Explorer hand-off.
  let earnOpen = $state<Record<string, boolean>>({});
  const devOn = $derived(!!app.settings?.advanced.developer_mode);
  // Missions of the channels in view (or picked), like the owners.
  const earnChans = $derived(new Set(prefs.channel.filter((c) => inView.includes(c)).length ? prefs.channel.filter((c) => inView.includes(c)) : inView));
  const earnList = (r: BlueprintRow) => earnFor(r, earnChans) ?? [];
  const earnSummary = (r: BlueprintRow) => {
    const list = earnList(r);
    const sure = list.filter((e) => e.award === 'guaranteed').length;
    const pools = list.filter((e) => e.award === 'guaranteed-pool').length;
    return [`${list.length} mission${list.length === 1 ? '' : 's'}`, sure ? `${sure} guaranteed` : null, pools ? `${pools} from a guaranteed pool` : null].filter(Boolean).join(', ');
  };
  const AWARD: Record<BlueprintEarn['award'], { tone: 'success' | 'info' | 'warning'; hint: string }> = {
    guaranteed: { tone: 'success', hint: 'Every contract with this title awards a pool that holds only this blueprint' },
    'guaranteed-pool': { tone: 'info', hint: 'Every contract awards a pool for sure, but this blueprint is one draw of several' },
    chance: { tone: 'warning', hint: 'Not on every completion (or not every variant): see the odds' },
  };
  const awardLabel = (e: BlueprintEarn) => (e.award === 'guaranteed' ? 'Guaranteed' : e.award === 'guaranteed-pool' ? `Guaranteed pool, 1 of ${e.pool}` : 'Chance');
  /** The odds line: MISSION DETAILS wording, then the pool. */
  const oddsText = (e: BlueprintEarn, contracts: number) =>
    [
      `Blueprint reward: ${e.odds}`,
      e.variants < contracts ? `${e.variants} of ${contracts} variants` : null,
      `pool of ${e.pool}${e.pick ? `, ${e.pick} of the draws` : ''}`,
    ]
      .filter(Boolean)
      .join(' · ');
  function findMission(generator: string) {
    dev.findRecord(generator, () => app.go('explorer'));
  }
  /** Catalogue rows in view whose build's contracts have not been read: their missions are not known. */
  const missionsUnread = $derived(v ? v.rows.filter((r) => r.record !== null && earnFor(r, earnChans) === null).length : 0);

  /** Owners in view (or "maybe" owners), per character, with whether they are picked. */
  function owners(r: BlueprintRow, list: BlueprintOwner[] = r.owners) {
    const pickedCh = new Set(prefs.channel.filter((c) => inView.includes(c)));
    const chs = pickedCh.size ? pickedCh : new Set(inView);
    const m = new Map<string, BlueprintOwner[]>();
    for (const o of list) if (chs.has(o.channel)) m.set(o.character, [...(m.get(o.character) ?? []), o]);
    return [...m].map(([id, list]) => ({
      id,
      name: charName(id),
      channels: [...new Set(list.map((o) => o.channel))],
      sources: [...new Set(list.flatMap((o) => o.sources))],
      acquired: acquired(list),
      favorite: list.some((o) => o.favorite),
      ambiguous: list.every((o) => o.ambiguous),
      picked: !prefs.character.length || prefs.character.includes(id),
    }));
  }
  const sizeText = (r: BlueprintRow) => [r.size !== null ? `S${r.size}` : r.class, r.grade].filter(Boolean).join(' · ') || '—';
  const readChannels = $derived((data?.channels ?? []).filter((c) => c.catalog !== null && inView.includes(c.channel)).map((c) => c.channel));
  /** In the All view: the builds a blueprint is in, when that is not all of them. */
  const onlyIn = (r: BlueprintRow) => (scopeAll && r.channels.length && r.channels.length < readChannels.length ? r.channels : []);

  // The finder's card (CollapsibleCard): `/` opens it before focusing the search.
  const card = getCardCollapse();
  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (e.key === '/' && !(t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable))) {
      e.preventDefault();
      if (card && !card.open) {
        card.set(true);
        void tick().then(() => searchEl?.focus());
      } else searchEl?.focus();
    }
  }
  const ariaSort = (s: SortKey) => (prefs.sort === s ? (prefs.desc ? 'descending' : 'ascending') : 'none');
  // No hints on these options: a hint's tooltip mounts on focus and takes the focus away from
  // arrow-key users (LazyTip); the meaning is in the facet help line instead.
  const ownedOpts = $derived<{ id: OwnedFilter; label: string }[]>([
    { id: 'all', label: `All ${v?.owned.all ?? ''}`.trim() },
    { id: 'owned', label: `Owned ${v?.owned.owned ?? ''}`.trim() },
    { id: 'missing', label: `Missing ${v?.owned.missing ?? ''}`.trim() },
  ]);
  const cols: { id: SortKey; label: string; cls?: string }[] = [
    { id: 'name', label: 'Blueprint' },
    { id: 'kind', label: 'Category' },
    { id: 'manufacturer', label: 'Maker', cls: 'max-lg:hidden' },
    { id: 'size', label: 'Size' },
    { id: 'owners', label: 'Owned by' },
    { id: 'source', label: 'Source', cls: 'max-xl:hidden' },
    { id: 'acquired', label: 'Since' },
  ];
  // Collapsed groups, remembered per channel, scope and group-by (a group id is a category,
  // maker or character id). A collapsed group keeps its header; its rows leave the paging.
  const groupKey = $derived(prefs.group === 'none' ? '' : GROUPS_KEY(target.install, target.channel, scopeAll ? 'all' : 'channel', prefs.group));
  // Read while deriving (not in an effect), so the first render already has the stored state.
  let changed = $state<{ key: string; list: string[] } | null>(null);
  const collapsed = $derived(changed && changed.key === groupKey ? changed.list : groupKey ? readCollapsed(groupKey) : []);
  const shut = $derived(new Set(collapsed));
  function setCollapsed(list: string[]) {
    changed = { key: groupKey, list };
    if (groupKey) writeCollapsed(groupKey, list);
  }
  const toggleGroup = (id: string, open: boolean) => setCollapsed(withCollapsed(collapsed, id, !open));
  const allIds = $derived(v?.groups?.map((g) => g.id) ?? []);
  // Collapse all merges: groups collapsed outside the current filter stay collapsed.
  const collapseAll = () => setCollapsed([...collapsed.filter((id) => !allIds.includes(id)), ...allIds]);
  const expandAll = () => setCollapsed(collapsed.filter((id) => !allIds.includes(id)));
  const hidden = $derived(v ? hiddenMatches(v, shut) : { count: 0, groups: [] });
  const narrowed = $derived(!!prefs.search.trim() || nActive > 0);
  const allShut = $derived(allIds.length > 0 && allIds.every((id) => shut.has(id)));
  const nShut = $derived(allIds.filter((id) => shut.has(id)).length);
  const noneShut = $derived(nShut === 0);
  function groupKeydown(e: KeyboardEvent, id: string, open: boolean) {
    const to = arrowToggle(e.key, open);
    if (to !== null) {
      e.preventDefault();
      toggleGroup(id, to);
      return;
    }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Home' && e.key !== 'End') return;
    // Up/Down/Home/End move between the group headers.
    const all = [...((e.currentTarget as HTMLElement).closest('tbody')?.querySelectorAll<HTMLElement>('[data-group-toggle]') ?? [])];
    const i = all.indexOf(e.currentTarget as HTMLElement);
    const j = e.key === 'Home' ? 0 : e.key === 'End' ? all.length - 1 : e.key === 'ArrowDown' ? i + 1 : i - 1;
    if (j < 0 || j >= all.length || j === i) return;
    e.preventDefault();
    all[j]!.focus();
  }
  const paged = $derived(v ? pageRows(v, limit, shut) : { lines: [], total: 0 });
  const shownRows = $derived(paged.lines);
  const totalShown = $derived(paged.total);
</script>

<svelte:window onkeydown={onKey} />

<div class="grid gap-3">
  <div class="grid gap-3" role="search" aria-label="Find blueprints">
  <div class="flex flex-wrap items-center gap-2">
    {#if channels.length > 1}
      <Segmented
        ariaLabel="Channels in view"
        size="sm"
        options={[
          { id: 'channel', label: target.channel },
          { id: 'all', label: 'All channels' },
        ]}
        value={scopeAll ? 'all' : 'channel'}
        onchange={(s) => set({ scope: s as FinderPrefs['scope'] })}
      />
    {/if}
    <Segmented ariaLabel="Owned or missing" size="sm" options={ownedOpts} value={prefs.owned} onchange={(o) => set({ owned: o as OwnedFilter })} />
    <div class="relative min-w-44 flex-1">
      <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
      <Input
        bind:ref={searchEl}
        type="search"
        aria-label="Search blueprints"
        aria-keyshortcuts="/"
        placeholder="Search blueprints  ( / )"
        class="h-8 pl-8"
        value={prefs.search}
        oninput={(e) => set({ search: (e.currentTarget as HTMLInputElement).value })}
        onkeydown={(e) => e.key === 'Escape' && prefs.search && (e.preventDefault(), set({ search: '' }))}
      />
    </div>
  </div>
  <div class="flex flex-wrap items-center gap-2 text-[12px]">
    <span class="text-muted-foreground" id="bp-sort">Sort</span>
    <SelectField ariaLabel="Sort by" options={SORTS} value={prefs.sort} onchange={(s) => set({ sort: s })} class="w-36" />
    <Button variant="outline" size="icon-sm" aria-label={prefs.desc ? 'Descending, switch to ascending' : 'Ascending, switch to descending'} title={prefs.desc ? 'Descending' : 'Ascending'} onclick={() => set({ desc: !prefs.desc })}>
      {#if prefs.desc}<ArrowDown aria-hidden="true" />{:else}<ArrowUp aria-hidden="true" />{/if}
    </Button>
    <span class="text-muted-foreground ml-2">Group</span>
    <SelectField ariaLabel="Group by" options={GROUPS} value={prefs.group} onchange={(g) => set({ group: g })} class="w-36" />
    <div class="ml-auto flex items-center gap-2">
      <Button variant="ghost" size="sm" aria-expanded={showFacets} aria-controls="bp-facets" onclick={() => (showFacets = !showFacets)}>
        <ListFilter aria-hidden="true" /> Filters{nActive ? ` (${nActive})` : ''}
      </Button>
      {#if nActive}
        <Button variant="ghost" size="sm" onclick={() => set(clearFilters(prefs))}><X aria-hidden="true" /> Clear filters</Button>
      {/if}
    </div>
  </div>

  {#if showFacets && v}
    <div id="bp-facets" class="bg-muted/30 grid gap-2 rounded-md border p-2.5">
      <p class="text-muted-foreground text-[11.5px]">Owned and Missing count the picked characters (all of them when none is picked){scopeAll ? ', on every channel; each channel keeps its own sets' : ` on ${target.channel}`}. A character's number is how many blueprints it owns (lacks, with Missing); other numbers show what a filter would give. Missing for keeps only what none of its characters owns (any source): pick Owned, a character (an alt, or a guild list pulled into one) and yourself under Missing for to see what they have and you lack. Missions and Reputation come from the builds' contracts; Guaranteed means a sure pool that holds only this blueprint, From a guaranteed pool a sure pool it is one draw of.</p>
      {#each FACETS.filter(facetShown) as f (f)}
        {@const list = chips(f)}
        {@const open = more[f] || list.length <= FACET_MAX + 1}
        <div class="grid grid-cols-[5.5rem_minmax(0,1fr)] items-start gap-2" role="group" aria-labelledby={`bp-f-${f}`}>
          <span id={`bp-f-${f}`} class="text-muted-foreground pt-1 text-[11.5px] font-medium">{FACET_LABEL[f]}</span>
          <div class="flex flex-wrap items-center gap-1.5">
          {#each open ? list : list.slice(0, FACET_MAX) as x (x.id)}
            <button
              type="button"
              aria-pressed={x.on}
              class={cn(
                'focus-visible:ring-ring/60 inline-flex h-6 items-center gap-1.5 rounded-full border px-2 text-[12px] outline-none focus-visible:ring-2',
                x.on ? 'border-primary bg-primary/15 text-foreground' : 'border-border hover:bg-muted text-muted-foreground hover:text-foreground',
                !x.count && !x.on && 'opacity-60',
              )}
              onclick={() => flip(f, x.id)}
            >
              {#if f === 'channel'}<ChannelBadge channel={x.id} variant="swatch" />{/if}
              <span class={cn(x.on && 'font-medium')}>{x.label}</span>
              {#if f === 'character' && scopeAll}
                {#each data?.characters.find((c) => c.id === x.id)?.channels ?? [] as c (c)}<ChannelBadge channel={c} variant="dot" title={`Known on ${c}`} />{/each}
              {/if}
              <span class="mono text-muted-foreground text-[11px]">{x.count}</span>
            </button>
          {/each}
          {#if !open}
            <button type="button" class="text-primary text-[12px] underline-offset-2 hover:underline" onclick={() => (more = { ...more, [f]: true })}>+{list.length - FACET_MAX} more</button>
          {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  </div>

  {#if skipped}
    <p class="text-muted-foreground flex items-start gap-1.5 text-[12px]" role="note">
      <Info class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <span>{skipped} blueprint file{skipped === 1 ? '' : 's'} could not be read and {skipped === 1 ? 'is' : 'are'} not shown; see Activity.</span>
    </p>
  {/if}
  {#if missionsUnread && !notRead.length}
    <p class="text-muted-foreground flex items-start gap-1.5 text-[12px]" role="note">
      <Info class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <span>The missions of {missionsUnread} blueprint{missionsUnread === 1 ? '' : 's'} are not known: that build's contracts have not been read (only a local read of the game data has them). Apply (or a dry run) reads them.</span>
    </p>
  {/if}
  {#if notRead.length}
    <p class="text-muted-foreground flex items-start gap-1.5 text-[12px]" role="note">
      <Info class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <span>The game data of {notRead.join(', ')} has not been read for this build yet, so only blueprints owned there are listed. Apply (or a dry run) reads it.</span>
    </p>
  {/if}

  {#if !data && !failed}
    <Skeleton class="h-40 w-full" />
  {:else if failed && !data}
    <p class="text-muted-foreground py-6 text-center">{failed}</p>
  {:else if v}
    <div class="flex flex-wrap items-baseline gap-x-3 text-[12px]" aria-live="polite">
      <span><span id="bp-count" class="text-foreground font-medium">{v.rows.length}</span> <span class="text-muted-foreground">shown of {v.total}</span></span>
      {#if catalogSize !== null && !scopeAll}<span class="text-muted-foreground">{target.channel}'s build has {catalogSize} blueprints</span>{/if}
      {#if prefs.owned === 'missing' && v.maybe}<span class="text-muted-foreground">{v.maybe} maybe owned (an owned name several blueprints have)</span>{/if}
      {#if v.groups && v.groups.length}
        <span class="ml-auto flex items-center gap-1">
          <span class="text-muted-foreground">{v.groups.length} group{v.groups.length === 1 ? '' : 's'}{nShut ? `, ${nShut} collapsed` : ''}</span>
          <Button variant="ghost" size="sm" disabled={allShut} onclick={collapseAll}><ChevronsDownUp aria-hidden="true" /> Collapse all</Button>
          <Button variant="ghost" size="sm" disabled={noneShut} onclick={expandAll}><ChevronsUpDown aria-hidden="true" /> Expand all</Button>
        </span>
      {/if}
    </div>
    {#if narrowed && hidden.count}
      <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 text-[12px]" role="status">
        <Info class="size-3.5 shrink-0" aria-hidden="true" />
        <span>{hidden.count} match{hidden.count === 1 ? '' : 'es'} in collapsed groups.</span>
        <button type="button" class="text-primary underline-offset-2 hover:underline" onclick={() => setCollapsed(collapsed.filter((id) => !hidden.groups.includes(id)))}>Expand {hidden.groups.length === 1 ? 'it' : 'them'}</button>
      </p>
    {/if}
    {#if !v.rows.length}
      <div class="text-muted-foreground grid justify-items-center gap-2 py-6 text-center">
        {#if v.total === 0}
          <p>No blueprints known for {scopeAll ? 'any channel' : target.channel} yet. Play, then the postexit hook or a rescan picks them up from Game.log, or import them.</p>
        {:else}
          <p>No blueprint matches these filters.</p>
          {#if nActive}<Button variant="outline" size="sm" onclick={() => set(clearFilters(prefs))}>Clear filters</Button>{/if}
        {/if}
      </div>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full text-left">
          <caption class="sr-only">Blueprints, {SORTS.find((s) => s.id === prefs.sort)?.label} {prefs.desc ? 'descending' : 'ascending'}</caption>
          <thead class="text-muted-foreground border-b text-[12px]">
            <tr>
              {#each cols as c (c.id)}
                <th scope="col" class={cn('py-1.5 pr-3 font-medium', c.cls)} aria-sort={ariaSort(c.id)}>
                  <button type="button" class="hover:text-foreground focus-visible:ring-ring/60 inline-flex items-center gap-1 rounded-sm outline-none focus-visible:ring-2" onclick={() => sortBy(c.id)}>
                    {c.label}
                    {#if prefs.sort === c.id}{#if prefs.desc}<ArrowDown class="size-3" aria-hidden="true" />{:else}<ArrowUp class="size-3" aria-hidden="true" />{/if}{/if}
                  </button>
                </th>
              {/each}
            </tr>
          </thead>
          <tbody class="divide-y">
            {#each shownRows as s, i (`${s.group ?? ''}/${s.row?.key ?? `#${s.label}`}/${i}`)}
              {#if s.row === null}
                {@const open = !s.collapsed}
                <tr class="bg-muted/40">
                  <th scope="rowgroup" colspan={cols.length} class="p-0 text-[12px] font-semibold">
                    <button
                      type="button"
                      data-group-toggle
                      class="hover:bg-muted focus-visible:ring-ring/60 flex w-full items-center gap-1.5 px-1 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset"
                      aria-expanded={open}
                      onclick={() => toggleGroup(s.group ?? '', !open)}
                      onkeydown={(e) => groupKeydown(e, s.group ?? '', open)}
                    >
                      <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', open && 'rotate-90')} aria-hidden="true" />
                      <span>{s.label}</span>
                      <span class="text-muted-foreground mono font-normal">{s.count}</span>
                      {#if !open}<span class="sr-only">(collapsed)</span>{/if}
                    </button>
                  </th>
                </tr>
              {:else}
                {@const r = s.row}
                {@const own = owners(r)}
                {@const mine = own.filter((o) => o.picked)}
                {@const maybe = owners(r, r.maybe).filter((o) => o.picked && !own.some((x) => x.id === o.id))}
                {@const maybeLacking = prefs.missing.length ? owners(r, r.maybe).filter((o) => prefs.missing.includes(o.id)) : []}
                <tr>
                  <td class="py-1.5 pr-3">
                    <div class="flex flex-wrap items-center gap-1.5">
                      {#if own.some((o) => o.favorite)}<Star class="text-accent-2 size-3.5" aria-label="Favourite" />{/if}
                      <span>{r.name}</span>
                      {#if r.record === null && r.candidates.length}<ToneBadge tone="warning" label="Ambiguous" hint={`This name could be ${r.candidates.length} blueprints; none is marked owned`} />{/if}
                      {#if r.record === null && !r.candidates.length}<ToneBadge tone="muted" label="Not in build" hint="No blueprint of the read builds has this name" />{/if}
                      {#if maybe.length}<ToneBadge tone="info" label="Maybe owned" hint="An owned name could be this blueprint or another one" />{/if}
                      {#each maybeLacking as o (o.id)}<ToneBadge tone="info" label={`Maybe owned by ${o.name}`} hint={`Missing for ${o.name}, but an owned name of theirs could be this blueprint or another one`} />{/each}
                      {#each onlyIn(r) as c (c)}<ChannelBadge channel={c} variant="badge" title={`Only in ${c}'s build`} />{/each}
                    </div>
                    {#if r.record}<div class="mono text-muted-foreground truncate text-[10.5px]" title={r.record}>{r.record}</div>{/if}
                    {#if earnList(r).length}
                      <button
                        type="button"
                        class="text-primary focus-visible:ring-ring/60 mt-0.5 inline-flex items-center gap-1 rounded-sm text-[11.5px] underline-offset-2 outline-none hover:underline focus-visible:ring-2"
                        aria-expanded={!!earnOpen[r.key]}
                        aria-controls={`bp-earn-${i}`}
                        onclick={() => (earnOpen = { ...earnOpen, [r.key]: !earnOpen[r.key] })}
                      >
                        <ChevronRight class={cn('size-3 shrink-0 transition-transform motion-reduce:transition-none', earnOpen[r.key] && 'rotate-90')} aria-hidden="true" />
                        Earn it: {earnSummary(r)}
                      </button>
                    {/if}
                    {#if r.record === null && r.candidates.length}
                      <ul class="text-muted-foreground mt-0.5 grid text-[11px]" aria-label="Could be">
                        {#each r.candidates as k (k)}<li>Could be: {candText(k)}</li>{/each}
                      </ul>
                    {/if}
                  </td>
                  <td class="py-1.5 pr-3 text-[12px]">
                    <div>{kindLabel(r.kind)}</div>
                    {#if r.type_label}<div class="text-muted-foreground">{r.type_label}</div>{/if}
                  </td>
                  <td class="text-muted-foreground py-1.5 pr-3 text-[12px] max-lg:hidden">{r.manufacturer ? makerLabel(r) : '—'}</td>
                  <td class="mono py-1.5 pr-3 text-[12px] whitespace-nowrap">{sizeText(r)}</td>
                  <td class="py-1.5 pr-3 text-[12px]">
                    {#if own.length}
                      <ul class="grid gap-0.5" aria-label="Owned by">
                        {#each own as o (o.id)}
                          <li class={cn('flex flex-wrap items-center gap-1', !o.picked && 'text-muted-foreground')}>
                            <span>{o.name}</span>
                            {#if scopeAll}{#each o.channels as c (c)}<ChannelBadge channel={c} />{/each}{/if}
                          </li>
                        {/each}
                      </ul>
                    {:else if !maybe.length}<span class="text-muted-foreground">Not owned</span>{/if}
                    {#if maybe.length}
                      <ul class="text-muted-foreground grid gap-0.5" aria-label="Maybe owned by">
                        {#each maybe as o (o.id)}
                          <li class="flex flex-wrap items-center gap-1"><span>Maybe {o.name}</span>{#if scopeAll}{#each o.channels as c (c)}<ChannelBadge channel={c} />{/each}{/if}</li>
                        {/each}
                      </ul>
                    {/if}
                  </td>
                  <td class="text-muted-foreground py-1.5 pr-3 text-[12px] max-xl:hidden">{[...new Set(mine.flatMap((o) => o.sources).map(sourceText))].join(', ') || '—'}</td>
                  <td class="mono text-muted-foreground py-1.5 text-[12px] whitespace-nowrap">{fmtDate(acquired(r.owners.filter((o) => mine.some((x) => x.id === o.character && x.channels.includes(o.channel)))))}</td>
                </tr>
                {#if earnOpen[r.key] && earnList(r).length}
                  <tr id={`bp-earn-${i}`} class="bg-muted/20">
                    <td colspan={cols.length} class="py-1.5 pr-3 pl-5 text-[12px]">
                      <ul class="grid gap-1.5" aria-label={`Missions that award ${r.name}`}>
                        {#each earnList(r) as e, k (`${e.mission}/${k}`)}
                          {@const m = data ? missionOf(data, e) : null}
                          <li class="grid gap-0.5">
                            {#if m}
                              <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                                <span class={cn('font-medium', m.title === null && 'mono text-[11.5px]')} title={m.title ? m.record : 'No readable title in this build'}>{missionTitle(m)}</span>
                                <ToneBadge tone={AWARD[e.award].tone} label={awardLabel(e)} hint={AWARD[e.award].hint} />
                                {#if scopeAll && e.channels.length < readChannels.length}{#each e.channels as c (c)}<ChannelBadge channel={c} variant="badge" title={`In ${c}'s build`} />{/each}{/if}
                                {#if devOn}
                                  <button
                                    type="button"
                                    class="text-primary focus-visible:ring-ring/60 inline-flex items-center gap-1 rounded-sm text-[11.5px] underline-offset-2 outline-none hover:underline focus-visible:ring-2"
                                    title="Search the Explorer for {m.generator}"
                                    onclick={() => findMission(m.generator)}>Find in Explorer<ExternalLink class="size-3 shrink-0" aria-hidden="true" /></button
                                  >
                                {/if}
                              </div>
                              <div class="text-muted-foreground">
                                {[m.reputation.length ? `Reputation: ${m.reputation.join(', ')}` : null, m.systems.length ? `System: ${m.systems.join(', ')}` : 'System: not known', m.kinds.length ? `Type: ${m.kinds.join(', ')}` : null, m.requires ? `Requires: ${m.requires}` : null].filter(Boolean).join(' · ')}
                              </div>
                              <div class="text-muted-foreground">{oddsText(e, m.contracts)}</div>
                            {:else}
                              <span class="text-muted-foreground">A mission this list does not have (reload the finder).</span>
                            {/if}
                          </li>
                        {/each}
                      </ul>
                    </td>
                  </tr>
                {/if}
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
      {#if totalShown > limit}
        <div class="flex justify-center"><Button variant="outline" size="sm" onclick={() => (limit += PAGE)}>Show {Math.min(PAGE, totalShown - limit)} more ({totalShown - limit} left)</Button></div>
      {/if}
    {/if}
  {/if}
</div>
