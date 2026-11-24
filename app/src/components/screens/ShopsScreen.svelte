<script lang="ts">
  /**
   * Shops: what each shop sells. Left, every UEX item terminal by system and place, with a
   * search and a system filter; right, the picked shop's items by category with a chip row, a
   * text filter, buy and sell prices, and a link to the item's Explorer record when the
   * channel's item data knows it. The "Shops | Items" switch turns the left column into an
   * item search: the items a name matches, and the picked one's shops cheapest first (or, to
   * sell, the best payers first); a shop there opens it in the shop list with the item filter
   * set. The mode and the search last for the session (`shopsState`). Shop stock is in no game
   * file: every row is UEX data (Settings -> Network -> Download UEX market data, Fetch now).
   */
  import { untrack } from 'svelte';
  import Store from '@lucide/svelte/icons/store';
  import Search from '@lucide/svelte/icons/search';
  import Settings from '@lucide/svelte/icons/settings';
  import Download from '@lucide/svelte/icons/download';
  import DatabaseZap from '@lucide/svelte/icons/database-zap';
  import Info from '@lucide/svelte/icons/info';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import SelectField from '@/components/common/SelectField.svelte';
  import Segmented from '@/components/common/Segmented.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { ShopCategory, ShopDirectory, ShopItemSearch, ShopItems } from '@/lib/contract/types';
  import { priceText } from '@/lib/sources';
  import { int } from '@/lib/format';
  import {
    CATEGORY_LABEL,
    categoryChips,
    dealOffers,
    filterShops,
    firstShop,
    groupItems,
    hasShop,
    itemMatchLine,
    textFiltered,
    topCategories,
    uexAgeDays,
    uexAgeText,
    type ItemDeal,
    type ShopsMode,
  } from '@/lib/shops';
  import { shopsState } from '@/lib/stores/shops.svelte';
  import { fetchUexNow } from '@/lib/uex';
  import { toast } from '@/lib/stores/toast.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { dev } from '@/lib/stores/dev.svelte';
  import { cn } from '@/lib/utils';

  let dir = $state<ShopDirectory | null>(null);
  let error = $state<ContractError | null>(null);
  let tick = $state(0);
  let query = $state('');
  let system = $state('');
  let selected = $state<number | null>(null);
  let items = $state<ShopItems | null>(null);
  let itemsError = $state<ContractError | null>(null);
  let itemsLoading = $state(false);
  let category = $state<ShopCategory | null>(null);
  let itemQuery = $state('');
  let fetching = $state(false);

  const devOn = $derived(!!app.settings?.advanced.developer_mode);
  const uexOn = $derived(!!app.settings?.network.uex);

  // Keyed on primitives only: the refresh counter, the channel key and Reload.
  $effect(() => {
    void app.rev;
    void tick;
    const key = app.targetKey;
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    api
      .shopInventory(key ? target : null)
      .then((d) => {
        if (stale) return;
        dir = d;
        error = null;
      })
      .catch((e) => !stale && (error = toContractError(e)));
    return () => (stale = true);
  });

  const ready = $derived(dir?.state === 'ready');
  const groups = $derived(filterShops(dir, query, system));
  const systems = $derived([{ id: '', label: 'All systems' }, ...(Array.isArray(dir?.systems) ? dir.systems : []).map((s) => ({ id: s.name, label: s.name }))]);
  const shown = $derived(groups.reduce((n, g) => n + g.count, 0));
  // The picked shop, else the first one listed.
  const current = $derived(hasShop(groups, selected) ? selected : (firstShop(groups)?.id ?? null));

  $effect(() => {
    void app.rev;
    void tick;
    const id = current;
    const key = app.targetKey;
    if (id === null || !ready || shopsState.mode !== 'shops') {
      items = null;
      return;
    }
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    itemsLoading = true;
    api
      .shopItems(key ? target : null, id)
      .then((r) => {
        if (stale) return;
        items = r;
        itemsError = null;
      })
      .catch((e) => !stale && (itemsError = toContractError(e)))
      .finally(() => !stale && (itemsLoading = false));
    return () => (stale = true);
  });

  const textRows = $derived(textFiltered(items?.items, itemQuery));
  const chips = $derived(categoryChips(textRows));
  // A category the shop does not sell (another shop picked) shows everything.
  const activeCategory = $derived(category && chips.some((c) => c.category === category) ? category : null);
  const itemGroups = $derived(groupItems(textRows, activeCategory));
  const shownItems = $derived(itemGroups.reduce((n, g) => n + g.items.length, 0));
  const age = $derived(uexAgeText(dir?.fetched));
  const ageDays = $derived(uexAgeDays(dir?.fetched));

  function pick(id: number) {
    selected = id;
    itemQuery = '';
  }

  // The item search. The typed text is sent after a short pause; the request is keyed on
  // primitives only (the sent text, the picked item, the channel, Reload), and a newer one
  // makes the older answer stale.
  const MODES: { id: ShopsMode; label: string; hint: string }[] = [
    { id: 'shops', label: 'Shops', hint: 'Every shop by system and place' },
    { id: 'items', label: 'Items', hint: 'Every shop that sells or buys an item' },
  ];
  let search = $state<ShopItemSearch | null>(null);
  let searchError = $state<ContractError | null>(null);
  let searchLoading = $state(false);
  let sentQuery = $state(shopsState.itemQuery.trim());
  $effect(() => {
    const q = shopsState.itemQuery.trim();
    const t = setTimeout(() => (sentQuery = q), 200);
    return () => clearTimeout(t);
  });
  $effect(() => {
    void app.rev;
    void tick;
    const q = sentQuery;
    const k = shopsState.itemKey;
    const key = app.targetKey;
    if (shopsState.mode !== 'items' || !ready || !q) {
      search = null;
      searchError = null;
      searchLoading = false;
      return;
    }
    let stale = false;
    const target = untrack(() => (app.target ? $state.snapshot(app.target) : null));
    searchLoading = true;
    api
      .shopItemSearch(key ? target : null, q, k)
      .then((r) => {
        if (stale) return;
        search = r;
        searchError = null;
      })
      .catch((e) => !stale && (searchError = toContractError(e)))
      .finally(() => !stale && (searchLoading = false));
    return () => (stale = true);
  });
  const typing = $derived(shopsState.itemQuery.trim() !== sentQuery);
  const found = $derived(Array.isArray(search?.items) ? search.items : []);
  const picked = $derived(search?.item ?? null);
  const offers = $derived(dealOffers(search?.offers, shopsState.deal, system));
  const deals = $derived<{ id: ItemDeal; label: string; hint: string }[]>([
    { id: 'buy', label: `Buy${picked ? ` · ${picked.sold_at}` : ''}`, hint: 'Shops that sell it, cheapest first' },
    { id: 'sell', label: `Sell${picked ? ` · ${picked.bought_at}` : ''}`, hint: 'Shops that pay for it, best price first' },
  ]);

  function setItemQuery(q: string) {
    shopsState.itemQuery = q;
    shopsState.itemKey = null;
  }
  // A shop of the item search: the shop list with that shop picked and the item filter set.
  function openShop(terminal: number) {
    const name = picked?.name ?? '';
    shopsState.mode = 'shops';
    query = '';
    category = null;
    selected = terminal;
    itemQuery = name;
  }
  function openRecord(guid: string) {
    dev.openRecord(guid, () => app.go('explorer'));
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

  // "Read game data" (a JobKind::Extract job): GameData, the Where to get datasets and the
  // ship item data the links need, under the memory limit; then the lists reload.
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

  const ROW = 'grid grid-cols-[minmax(0,1fr)_7.5rem_7.5rem_6rem] items-baseline gap-3';
  const OFFER_ROW = 'grid grid-cols-[minmax(0,1fr)_7.5rem_6rem] items-baseline gap-3';
</script>

{#if error}
  <ErrorState title="Could not read the shop data" {error} onretry={() => tick++} />
{:else if !dir}
  <Skeleton class="h-96 w-full" />
{:else if dir.state === 'missing'}
  <FullEmpty icon={Store} title="No shop data yet">
    Shop stock is in no game file: this screen lists the UEX market data (player-reported). Turn on "Download UEX market data" in
    Settings -> Network, then press Fetch now.
    {#if dir.folder}<span class="mt-2 block text-[12px]">Or put <span class="mono">uex-item-prices.json</span> and <span class="mono">uex-terminals.json</span> in <span class="mono select-all break-all">{dir.folder}</span>.</span>{/if}
    {#snippet actions()}
      {#if uexOn}
        <Button onclick={fetchNow} disabled={fetching}>
          {#if fetching}<LoaderCircle class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Download aria-hidden="true" />{/if}
          Fetch now
        </Button>
      {/if}
      <Button variant={uexOn ? 'outline' : 'default'} onclick={networkSettings}><Settings aria-hidden="true" /> Network settings</Button>
    {/snippet}
  </FullEmpty>
{:else}
  <div class="grid gap-3">
    <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px]" role="note">
      <Info class="size-3.5 shrink-0" aria-hidden="true" />
      {#if age}<span class={cn(ageDays !== null && ageDays > 14 && 'text-warning')} title={dir.fetched ?? undefined}>{age}{ageDays !== null && ageDays > 14 ? ` (${ageDays} days old)` : ''}</span><span aria-hidden="true">·</span>{/if}
      <span>{int(dir.shop_count)} shops, {int(dir.item_count)} items</span>
      <span aria-hidden="true">·</span>
      <span>Prices are player-reported; a date is the day UEX last confirmed the row</span>
      {#if uexOn}
        <Button variant="ghost" size="sm" class="ml-auto h-6" onclick={fetchNow} disabled={fetching}>
          {#if fetching}<LoaderCircle class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Download aria-hidden="true" />{/if}
          Fetch now
        </Button>
      {/if}
    </p>
    {#if !dir.ship_items && app.target}
      <div class="bg-muted/30 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-md border px-3 py-2 text-[12px]" role="note">
        <DatabaseZap class="text-muted-foreground size-4 shrink-0" aria-hidden="true" />
        <span class="min-w-0 flex-1">
          {#if dir.linked}
            Ship weapons, missiles and components keep UEX's names and do not link to their records until the ship item data of {app.target.channel} is read.
          {:else}
            Item names are UEX's and nothing links to its record until the game data of {app.target.channel} is read.
          {/if}
          {#if !app.channel?.set_up}Set up {app.target.channel} first.{:else}Nothing is written to the game; the first read of a build takes a minute or two.{/if}
        </span>
        <Button size="sm" variant="outline" disabled={reading || !app.channel?.set_up} onclick={readGameData}>
          <DatabaseZap class={cn(reading && 'animate-pulse motion-reduce:animate-none')} aria-hidden="true" />
          {reading ? 'Reading game data…' : 'Read game data'}
        </Button>
      </div>
    {/if}

    <div class="grid items-start gap-3 lg:grid-cols-[minmax(15rem,21rem)_minmax(0,1fr)]">
      <section class="bg-card flex min-h-0 flex-col rounded-lg border" aria-labelledby="shops-list-title">
        <h2 id="shops-list-title" class="sr-only">{shopsState.mode === 'items' ? 'Item search' : 'Shops'}</h2>
        <div class="grid gap-2 border-b p-2.5">
          <Segmented size="sm" ariaLabel="Search shops or items" class="justify-self-start" options={MODES} bind:value={shopsState.mode} />
          {#if shopsState.mode === 'items'}
            <div class="relative">
              <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
              <Input
                type="search"
                aria-label="Search items"
                placeholder="Search items"
                class="h-8 pl-8"
                value={shopsState.itemQuery}
                oninput={(e) => setItemQuery((e.currentTarget as HTMLInputElement).value)}
                onkeydown={(e) => e.key === 'Escape' && shopsState.itemQuery && (e.preventDefault(), setItemQuery(''))}
              />
            </div>
            <div class="flex items-center gap-2 text-[12px]">
              <SelectField ariaLabel="System" options={systems} value={system} onchange={(v) => (system = v)} class="min-w-36 flex-1" />
              <span class="text-muted-foreground shrink-0" aria-live="polite">{search ? `${search.total} item${search.total === 1 ? '' : 's'}` : ''}</span>
            </div>
          {:else}
          <div class="relative">
            <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
            <Input
              type="search"
              aria-label="Search shops"
              placeholder="Search shops or places"
              class="h-8 pl-8"
              value={query}
              oninput={(e) => (query = (e.currentTarget as HTMLInputElement).value)}
              onkeydown={(e) => e.key === 'Escape' && query && (e.preventDefault(), (query = ''))}
            />
          </div>
          <div class="flex items-center gap-2 text-[12px]">
            <SelectField ariaLabel="System" options={systems} value={system} onchange={(v) => (system = v)} class="min-w-36 flex-1" />
            <span class="text-muted-foreground shrink-0" aria-live="polite">{shown} of {dir.shop_count}</span>
          </div>
          {/if}
        </div>
        {#if shopsState.mode === 'items'}
          {#if !shopsState.itemQuery.trim()}
            <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">Type an item name to see every shop that sells or buys it.</p>
          {:else if searchError && !search}
            <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">The search failed; see the right side.</p>
          {:else if !search || (typing && !found.length)}
            <div class="grid gap-2 p-3" aria-busy="true"><Skeleton class="h-8 w-full" /><Skeleton class="h-8 w-full" /><Skeleton class="h-8 w-full" /></div>
          {:else if !found.length}
            <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">No item matches “{search.query}”.</p>
          {:else}
            <ul class="m-0 max-h-[calc(100vh-15rem)] list-none overflow-y-auto p-0 py-1" aria-label="Items">
              {#each found as m (m.key)}
                <li>
                  <button
                    type="button"
                    aria-current={m.key === picked?.key ? 'true' : undefined}
                    class={cn(
                      'focus-visible:ring-ring/60 grid w-full gap-0.5 px-3 py-1 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset',
                      m.key === picked?.key ? 'bg-primary/15 text-foreground' : 'hover:bg-muted',
                    )}
                    onclick={() => (shopsState.itemKey = m.key)}
                  >
                    <span class={cn('truncate text-[13px]', m.key === picked?.key && 'font-medium')} title={m.uex_name ? `${m.name} (UEX: ${m.uex_name})` : m.name}>{m.name}</span>
                    <span class="text-muted-foreground truncate text-[11px]">{CATEGORY_LABEL[m.category]} · {itemMatchLine(m, priceText)}</span>
                  </button>
                </li>
              {/each}
            </ul>
            {#if search.total > found.length}
              <p class="text-muted-foreground border-t px-3 py-2 text-[11.5px]">{found.length} of {search.total} shown; type more of the name.</p>
            {/if}
          {/if}
        {:else if !groups.length}
          <p class="text-muted-foreground px-3 py-6 text-center text-[12px]">No shop matches.</p>
        {:else}
          <nav class="max-h-[calc(100vh-15rem)] overflow-y-auto py-1" aria-label="Shops by system and place">
            {#each groups as g (g.system)}
              <h3 class="eyebrow text-muted-foreground px-3 pt-2.5 pb-1">{g.system} <span class="mono font-normal">{g.count}</span></h3>
              {#each g.places as p (p.name)}
                <p class="text-muted-foreground truncate px-3 pt-1.5 text-[11px]" title={p.name}>{p.name}</p>
                <ul class="m-0 list-none p-0">
                  {#each p.shops as s (s.id)}
                    <li>
                      <button
                        type="button"
                        aria-current={s.id === current ? 'true' : undefined}
                        class={cn(
                          'focus-visible:ring-ring/60 grid w-full grid-cols-[minmax(0,1fr)_auto] gap-x-2 px-3 py-1 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset',
                          s.id === current ? 'bg-primary/15 text-foreground' : 'hover:bg-muted',
                        )}
                        onclick={() => pick(s.id)}
                      >
                        <span class={cn('truncate text-[13px]', s.id === current && 'font-medium')} title={s.name}>{s.name}</span>
                        <span class="mono text-muted-foreground text-[11px]">{s.items}</span>
                        <span class="text-muted-foreground col-span-2 truncate text-[11px]">{topCategories(s)}{s.not_on_live ? ' · not on LIVE' : ''}</span>
                      </button>
                    </li>
                  {/each}
                </ul>
              {/each}
            {/each}
          </nav>
        {/if}
      </section>

      {#if shopsState.mode === 'items'}
        <section class="bg-card grid min-w-0 gap-3 rounded-lg border p-3" aria-labelledby="item-title" aria-busy={searchLoading}>
          {#if searchError}
            <ErrorState title="Could not search the shop data" error={searchError} onretry={() => tick++} />
          {:else if !picked}
            {#if searchLoading}<Skeleton class="h-64 w-full" />{:else}<p class="text-muted-foreground py-6 text-center">Search for an item, then pick it.</p>{/if}
          {:else}
            <header class="flex flex-wrap items-start gap-x-3 gap-y-2">
              <div class="grid min-w-0 flex-1 gap-0.5">
                <h2 id="item-title" class="text-[16px] font-semibold">
                  {#if picked.record && devOn}
                    <button
                      type="button"
                      class="text-primary focus-visible:ring-ring/60 inline-flex max-w-full items-center gap-1 rounded-sm text-left underline-offset-2 outline-none hover:underline focus-visible:ring-2"
                      title="Open {picked.record.class_name} in the Explorer"
                      onclick={() => openRecord(picked.record!.guid)}><span class="truncate">{picked.name}</span><ExternalLink class="size-3.5 shrink-0" aria-hidden="true" /></button
                    >
                  {:else}{picked.name}{/if}
                </h2>
                <p class="text-muted-foreground text-[12px]">
                  {CATEGORY_LABEL[picked.category]}{picked.uex_name ? ` · UEX: ${picked.uex_name}` : ''}{picked.record ? '' : ' · not linked to a game record'}
                </p>
              </div>
              <Segmented size="sm" ariaLabel="Buy or sell" options={deals} bind:value={shopsState.deal} />
            </header>
            {#if !offers.length}
              <p class="text-muted-foreground py-6 text-center text-[12px]">
                {shopsState.deal === 'buy' ? 'No shop sells it' : 'No shop pays for it'}{system ? ` in ${system}` : ''} in the UEX data.
              </p>
            {:else}
              <div role="table" aria-label="{shopsState.deal === 'buy' ? 'Shops that sell' : 'Shops that buy'} {picked.name}" class="text-[12.5px]">
                <div role="rowgroup">
                  <div role="row" class={cn(OFFER_ROW, 'text-muted-foreground border-b py-1 text-[11px]')}>
                    <span role="columnheader" class="eyebrow text-foreground">Shop <span class="mono text-muted-foreground font-normal">{offers.length}</span></span>
                    <span role="columnheader" class="text-right" title={shopsState.deal === 'buy' ? 'What you pay' : 'What the shop pays'}>{shopsState.deal === 'buy' ? 'Buy' : 'Sell'}</span>
                    <span role="columnheader" class="text-right" title="Day UEX last confirmed the row">Updated</span>
                  </div>
                </div>
                <div role="rowgroup" class="divide-y divide-border/50">
                  {#each offers as o (o.terminal)}
                    {@const price = shopsState.deal === 'buy' ? o.price_buy : o.price_sell}
                    <div role="row" class={cn(OFFER_ROW, 'py-1')}>
                      <span role="cell" class="min-w-0">
                        <button
                          type="button"
                          class="text-primary focus-visible:ring-ring/60 inline-flex max-w-full items-center rounded-sm text-left underline-offset-2 outline-none hover:underline focus-visible:ring-2"
                          title="Show everything {o.shop} sells"
                          onclick={() => openShop(o.terminal)}><span class="truncate">{o.shop}</span></button
                        >
                        <span class="text-muted-foreground block truncate text-[11px]"
                          >{o.location}{o.system ? ` · ${o.system}` : ''}{#if o.not_on_live}<span class="text-warning"> · not on LIVE</span>{/if}</span
                        >
                      </span>
                      <span role="cell" class="text-right font-medium tabular-nums">{price !== null ? priceText(price) : '–'}</span>
                      <span role="cell" class={cn('text-right tabular-nums', o.stale ? 'text-muted-foreground/70' : 'text-muted-foreground')} title={o.stale ? 'Not confirmed for over 30 days' : undefined}
                        >{o.updated ?? '–'}{o.stale ? ' ⚠' : ''}</span
                      >
                    </div>
                  {/each}
                </div>
              </div>
              {#if search && search.offer_count > search.offers.length}
                <p class="text-muted-foreground text-[11.5px]">The first {search.offers.length} of {search.offer_count} shops, cheapest first.</p>
              {/if}
            {/if}
          {/if}
        </section>
      {:else}
      <section class="bg-card grid min-w-0 gap-3 rounded-lg border p-3" aria-labelledby="shop-title" aria-busy={itemsLoading}>
        {#if itemsError}
          <ErrorState title="Could not read this shop" error={itemsError} onretry={() => tick++} />
        {:else if !items?.shop}
          {#if itemsLoading}<Skeleton class="h-64 w-full" />{:else}<p class="text-muted-foreground py-6 text-center">Pick a shop.</p>{/if}
        {:else}
          {@const s = items.shop}
          <header class="grid gap-0.5">
            <h2 id="shop-title" class="text-[16px] font-semibold">{s.name}</h2>
            <p class="text-muted-foreground text-[12px]">
              {s.location}{s.system ? ` · ${s.system}` : ''} · {s.items} item{s.items === 1 ? '' : 's'}{s.updated ? ` · last confirmed ${s.updated}` : ''}
              {#if s.not_on_live}<span class="text-warning"> · UEX marks it not on LIVE</span>{/if}
            </p>
          </header>
          <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Category">
            <button
              type="button"
              aria-pressed={activeCategory === null}
              class={cn(
                'focus-visible:ring-ring/60 inline-flex h-6 items-center gap-1.5 rounded-full border px-2 text-[12px] outline-none focus-visible:ring-2',
                activeCategory === null ? 'border-primary bg-primary/15 text-foreground' : 'border-border hover:bg-muted text-muted-foreground hover:text-foreground',
              )}
              onclick={() => (category = null)}><span class={cn(activeCategory === null && 'font-medium')}>All</span><span class="mono text-muted-foreground text-[11px]">{textRows.length}</span></button
            >
            {#each chips as c (c.category)}
              {@const on = activeCategory === c.category}
              <button
                type="button"
                aria-pressed={on}
                class={cn(
                  'focus-visible:ring-ring/60 inline-flex h-6 items-center gap-1.5 rounded-full border px-2 text-[12px] outline-none focus-visible:ring-2',
                  on ? 'border-primary bg-primary/15 text-foreground' : 'border-border hover:bg-muted text-muted-foreground hover:text-foreground',
                )}
                onclick={() => (category = on ? null : c.category)}><span class={cn(on && 'font-medium')}>{c.label}</span><span class="mono text-muted-foreground text-[11px]">{c.count}</span></button
              >
            {/each}
            <div class="relative ml-auto min-w-44">
              <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
              <Input
                type="search"
                aria-label="Filter items"
                placeholder="Filter items"
                class="h-7 pl-8 text-[12.5px]"
                value={itemQuery}
                oninput={(e) => (itemQuery = (e.currentTarget as HTMLInputElement).value)}
                onkeydown={(e) => e.key === 'Escape' && itemQuery && (e.preventDefault(), (itemQuery = ''))}
              />
            </div>
          </div>
          {#if !shownItems}
            <p class="text-muted-foreground py-6 text-center text-[12px]">No item matches{itemQuery ? ` "${itemQuery}"` : ''}.</p>
          {:else}
            {#each itemGroups as g (g.category)}
              <div role="table" aria-label="{g.label} at {s.name}" class="text-[12.5px]">
                <div role="rowgroup">
                  <div role="row" class={cn(ROW, 'text-muted-foreground border-b py-1 text-[11px]')}>
                    <span role="columnheader" class="eyebrow text-foreground">{g.label} <span class="mono text-muted-foreground font-normal">{g.items.length}</span></span>
                    <span role="columnheader" class="text-right">Buy</span>
                    <span role="columnheader" class="text-right" title="What the shop pays for it">Sell</span>
                    <span role="columnheader" class="text-right" title="Day UEX last confirmed the row">Updated</span>
                  </div>
                </div>
                <div role="rowgroup" class="divide-y divide-border/50">
                  {#each g.items as i, k (`${i.record?.guid ?? i.name}/${k}`)}
                    <div role="row" class={cn(ROW, 'py-1')}>
                      <span role="cell" class="min-w-0">
                        {#if i.record && devOn}
                          <button
                            type="button"
                            class="text-primary focus-visible:ring-ring/60 inline-flex max-w-full items-center gap-1 rounded-sm text-left underline-offset-2 outline-none hover:underline focus-visible:ring-2"
                            title="Open {i.record.class_name} in the Explorer"
                            onclick={() => openRecord(i.record!.guid)}
                          >
                            <span class="truncate">{i.name}</span><ExternalLink class="size-3 shrink-0" aria-hidden="true" />
                          </button>
                        {:else}
                          <span class="truncate" title={i.record ? `${i.record.class_name} (turn on Developer mode in Settings -> Advanced to open records in the Explorer)` : 'Not linked to a game record'}>{i.name}</span>
                        {/if}
                        {#if i.uex_name}<span class="text-muted-foreground block truncate text-[11px]">UEX: {i.uex_name}</span>{/if}
                      </span>
                      <span role="cell" class="text-right font-medium tabular-nums">{i.price_buy !== null ? priceText(i.price_buy) : '–'}</span>
                      <span role="cell" class="text-muted-foreground text-right tabular-nums">{i.price_sell !== null ? priceText(i.price_sell) : '–'}</span>
                      <span role="cell" class={cn('text-right tabular-nums', i.stale ? 'text-muted-foreground/70' : 'text-muted-foreground')} title={i.stale ? 'Not confirmed for over 30 days' : undefined}
                        >{i.updated ?? '–'}{i.stale ? ' ⚠' : ''}</span
                      >
                    </div>
                  {/each}
                </div>
              </div>
            {/each}
            {#if shownItems < (items.items?.length ?? 0)}
              <p class="text-muted-foreground text-[11.5px]">{shownItems} of {items.items.length} items shown{activeCategory ? ` (${CATEGORY_LABEL[activeCategory]})` : ''}.</p>
            {/if}
          {/if}
        {/if}
      </section>
      {/if}
    </div>
  </div>
{/if}
