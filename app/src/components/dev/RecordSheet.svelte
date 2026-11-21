<script lang="ts">
  /**
   * The record as a player reads it (the Explorer's "Readable" view): sections of key/value
   * rows built by `lib/recordsheet`, collapsible, with the first sections open and closed ones
   * not rendered at all. References and GUIDs are links to the record they name, `@keys` show
   * their text and open in Strings, a cut part loads with its button. Empty values hide until
   * "Show empty"; "Show IDs" puts the raw key and value beside each row. Ctrl+F (or `/`) finds
   * rows by label, key or value; right-click copies a row's raw value. Values are text only.
   */
  import { untrack } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right';
  import { Input } from '@/components/ui/input';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { splice, type Json, type RefInfo } from '@/lib/jsontree';
  import { buildSheet, rowCount, viewSheet, type SheetGroup, type SheetRow } from '@/lib/recordsheet';
  import type { LocText, Readable } from '@/lib/contract/dev';
  import { dev } from '@/lib/stores/dev.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  let {
    json,
    label,
    loc = [],
    labels = {},
    onfollow = null,
    onload = null,
    onloc = null,
    id = 'rs',
    class: className = '',
  }: {
    /** Compact JSON text from the backend. */
    json: string;
    label: string;
    loc?: LocText[];
    /** Raw value -> what it reads as (7.14). */
    labels?: Record<string, Readable>;
    onfollow?: ((r: RefInfo) => void) | null;
    /** Load the node at a pointer that the backend cut. */
    onload?: ((pointer: string) => Promise<Json>) | null;
    /** A loc key's text was clicked: open it in Strings. */
    onloc?: ((key: string) => void) | null;
    id?: string;
    class?: string;
  } = $props();

  /** Top-level sections open at first. */
  const OPEN_SECTIONS = 6;
  /** Nested groups open at first down to this depth. */
  const OPEN_DEPTH = 2;
  /** Groups listed under one parent before "Show more". */
  const PAGE = 100;
  /** Indentation stops here; deeper groups show where they sit instead. */
  const MAX_INDENT = 4;

  let root = $state.raw<Json>(null);
  let opened = $state.raw(new Map<string, boolean>());
  let pages = $state.raw(new Map<string, number>());
  let scroller = $state<HTMLElement | null>(null);
  $effect(() => {
    const text = json;
    untrack(() => {
      try {
        root = JSON.parse(text) as Json;
      } catch {
        root = text;
      }
      opened = new Map();
      pages = new Map();
      if (scroller) scroller.scrollTop = 0;
    });
  });
  const sheet = $derived(buildSheet(root, { labels, loc }));

  let findEl = $state<HTMLInputElement | null>(null);
  let find = $state('');
  let query = $state('');
  $effect(() => {
    const q = find;
    const t = setTimeout(() => (query = q.trim()), 150);
    return () => clearTimeout(t);
  });
  let showEmpty = $state(false);
  const view = $derived(viewSheet(sheet, query, showEmpty));

  function isOpen(g: SheetGroup, index: number): boolean {
    if (g.cut) return false;
    if (query) return true;
    return opened.get(g.id) ?? (g.depth === 0 ? index < OPEN_SECTIONS : g.depth <= OPEN_DEPTH);
  }
  function toggle(g: SheetGroup, index: number) {
    opened = new Map(opened).set(g.id, !isOpen(g, index));
  }
  const domId = (g: SheetGroup) => `${id}-g${g.id.replace(/[^\w-]/g, '_')}`;

  let loadingAt = $state<string | null>(null);
  async function load(p: string) {
    if (!onload || loadingAt) return;
    loadingAt = p;
    try {
      const v = await onload(p);
      root = splice(root, p, v);
      opened = new Map(opened).set(p, true);
    } catch (e) {
      toast('error', 'Could not load this part', e instanceof Error ? e.message : String(e));
    } finally {
      loadingAt = null;
    }
  }
  async function copy(r: SheetRow, e: MouseEvent) {
    e.preventDefault();
    try {
      await navigator.clipboard.writeText(r.raw);
      toast('success', `Copied ${r.label}`);
    } catch {
      toast('error', 'The clipboard is not available here');
    }
  }
  function onKey(e: KeyboardEvent) {
    const inField = (e.target as HTMLElement | null)?.closest('input, textarea, select');
    if (inField) return;
    if (((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') || (e.key === '/' && !e.ctrlKey && !e.altKey && !e.metaKey)) {
      e.preventDefault();
      findEl?.focus();
      findEl?.select();
    }
  }
  const indent = (depth: number) => Math.min(depth, MAX_INDENT) * 14;
</script>

{#snippet value(r: SheetRow)}
  {@const v = r.value}
  {#if v.kind === 'ref' && v.follow && onfollow}
    {@const f = v.follow}
    <button
      type="button"
      class="text-primary inline-flex max-w-full min-w-0 items-center gap-0.5 text-left underline-offset-2 hover:underline"
      title={`Open ${v.text}${v.detail ? ` (${v.detail})` : ''}\n${r.raw}`}
      onclick={() => onfollow?.(f)}
    >
      <ArrowUpRight class="size-3 shrink-0" aria-hidden="true" /><span class="truncate">{v.text}</span>
    </button>
    {#if v.detail && v.detail !== v.text}<span class="text-muted-foreground mono truncate text-[11px]">{v.detail}</span>{/if}
  {:else if v.kind === 'ref'}
    <span class="truncate" title={v.detail ?? r.raw}>{v.text}</span>
  {:else if v.kind === 'loc' && !v.missing}
    <button
      type="button"
      class="hover:text-primary min-w-0 text-left break-words underline-offset-2 hover:underline disabled:no-underline"
      title={`${v.key}${v.enhanced ? `\nEnhanced: ${v.enhanced}` : ''}${onloc ? '\nOpen in Strings' : ''}`}
      disabled={!onloc}
      onclick={() => onloc?.(v.key)}>{v.text}</button
    >
    {#if v.enhanced}<span class="text-primary text-[10px] font-semibold uppercase" title="Enhanced: {v.enhanced}">enh</span>{/if}
  {:else if v.kind === 'loc'}
    <span class="mono text-muted-foreground truncate" title="{v.key} (no text)">{v.text}</span>
  {:else if v.kind === 'list'}
    <span class="min-w-0 break-words" title={v.text}>{v.items.join(', ')}{#if v.more}<span class="text-muted-foreground"> +{v.more} more</span>{/if}</span>
  {:else if v.kind === 'empty'}
    <span class="text-muted-foreground italic">{v.text}</span>
  {:else if v.kind === 'number'}
    <span class="tabular-nums">{v.text}</span>
  {:else}
    <span class="min-w-0 break-words">{v.text}</span>
  {/if}
  {#if dev.showIds && r.raw !== v.text && v.kind !== 'number' && v.kind !== 'bool'}
    <span class="text-muted-foreground mono min-w-0 truncate text-[11px]" title={r.raw}>{r.raw}</span>
  {/if}
{/snippet}

{#snippet groups(list: SheetGroup[], parent: string)}
  {@const cap = pages.get(parent) ?? PAGE}
  {#each list.slice(0, cap) as g, i (g.id + '|' + g.title)}
    {@const isOpenNow = isOpen(g, i)}
    <section class={cn(g.depth === 0 ? 'border-b last:border-b-0' : 'mt-1')} aria-labelledby="{domId(g)}-h">
      {#if g.cut}
        <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 py-1.5 pr-3" style="padding-left: {8 + indent(g.depth)}px">
          <span class="w-3.5 shrink-0"></span>
          <h3 id="{domId(g)}-h" class={cn('min-w-0 truncate', g.depth === 0 ? 'text-[12.5px] font-semibold' : 'text-[12px] font-medium')}>{#if g.depth > MAX_INDENT}<span class="text-muted-foreground font-normal">{g.trail.slice(MAX_INDENT).join(' › ')} › </span>{/if}{g.title}</h3>
          {#if g.subtitle}<span class="text-muted-foreground truncate text-[11px]">{g.subtitle}</span>{/if}
          <Button variant="outline" size="xs" disabled={!onload || loadingAt !== null} onclick={() => void load(g.id)}>{loadingAt === g.id ? 'Loading…' : `Load ${g.cut.len} ${g.cut.kind === 'array' ? 'items' : 'fields'}`}</Button>
        </div>
      {:else}
        <h3 id="{domId(g)}-h" class="m-0">
          <button
            type="button"
            aria-expanded={isOpenNow}
            aria-controls={domId(g)}
            class={cn('hover:bg-accent/50 focus-visible:ring-ring/60 flex w-full min-w-0 items-center gap-2 py-1.5 pr-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset', g.depth === 0 && 'bg-muted/30')}
            style="padding-left: {8 + indent(g.depth)}px"
            title={[...g.trail, g.title].join(' › ') + (g.type ? `\n${g.type}` : '')}
            onclick={() => toggle(g, i)}
          >
            <ChevronRight class={cn('text-muted-foreground size-3.5 shrink-0 transition-transform motion-reduce:transition-none', isOpenNow && 'rotate-90')} aria-hidden="true" />
            <span class={cn('min-w-0 truncate', g.depth === 0 ? 'text-[12.5px] font-semibold' : 'text-[12px] font-medium')}>{#if g.depth > MAX_INDENT}<span class="text-muted-foreground font-normal">{g.trail.slice(MAX_INDENT).join(' › ')} › </span>{/if}{g.title}</span>
            {#if g.subtitle}<span class="text-muted-foreground min-w-0 truncate text-[11px]">{g.subtitle}</span>{/if}
            <span class="text-muted-foreground mono ml-auto shrink-0 text-[10.5px]">{rowCount(g)}</span>
          </button>
        </h3>
        {#if isOpenNow}
          <div id={domId(g)} class="pb-1">
            {#if g.rows.length}
              <dl class="grid grid-cols-[minmax(6rem,14rem)_minmax(0,1fr)] gap-x-3 pr-3 text-[12px]" style="padding-left: {30 + indent(g.depth)}px">
                {#each g.rows as r (r.pointer)}
                  <dt class="text-muted-foreground min-w-0 truncate py-0.5" title={r.key}>
                    {r.label}{#if dev.showIds && r.key !== r.label}<span class="mono ml-1 text-[10.5px] opacity-80">{r.key}</span>{/if}
                  </dt>
                  <dd class="flex min-w-0 flex-wrap items-baseline gap-x-1.5 py-0.5" oncontextmenu={(e) => void copy(r, e)}>{@render value(r)}</dd>
                {/each}
              </dl>
            {/if}
            {#if g.groups.length}{@render groups(g.groups, g.id)}{/if}
          </div>
        {/if}
      {/if}
    </section>
  {/each}
  {#if list.length > cap}
    <div class="py-1.5" style="padding-left: {30 + indent(list[0]!.depth)}px">
      <Button variant="link" size="xs" class="h-auto p-0" onclick={() => (pages = new Map(pages).set(parent, cap + PAGE))}>Show {Math.min(PAGE, list.length - cap)} more of {list.length - cap}</Button>
    </div>
  {/if}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class={cn('flex min-h-0 flex-col', className)} onkeydown={onKey}>
  <div class="flex flex-wrap items-center gap-2 border-b px-2 py-1.5" role="search" aria-label="Search in {label}">
    <div class="relative min-w-0 flex-1 basis-48">
      <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2" aria-hidden="true" />
      <Input
        bind:ref={findEl}
        type="search"
        aria-label="Find in record"
        placeholder="Find in record (Ctrl+F)"
        class="h-7 pl-7 text-[12px]"
        bind:value={find}
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            scroller?.focus();
          }
        }}
      />
    </div>
    <label class="text-muted-foreground flex shrink-0 items-center gap-1.5 text-[11.5px]">
      <Checkbox checked={showEmpty} onCheckedChange={(v) => (showEmpty = v === true)} />
      Show empty{#if !showEmpty && view.hidden}<span class="mono">({view.hidden} hidden)</span>{/if}
    </label>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div bind:this={scroller} data-pane-stop tabindex="0" class="focus-visible:ring-ring/60 min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-2 focus-visible:ring-inset" aria-label={label} role="region">
    {#if view.groups.length}
      {@render groups(view.groups, '')}
    {:else}
      <p class="text-muted-foreground p-4 text-[12.5px]" aria-live="polite">
        {query ? 'No field matches.' : view.hidden ? `Only empty values (${view.hidden}). Turn on Show empty to see them.` : 'This record has no values.'}
      </p>
    {/if}
  </div>
</div>
