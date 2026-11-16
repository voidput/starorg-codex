<script lang="ts">
  /**
   * Collapsible, virtualised JSON tree (addendum 7.4, 7.8): only the lines in view are in the
   * DOM, the first two levels start open, a cut node (`{"_cut": ...}`) loads on expand.
   * Keys: `role="tree"` with Up/Down, Right/Left, Home/End, `*` opens all siblings, Enter
   * follows a reference or toggles, Ctrl+C copies the value under the cursor, Shift+Ctrl+C its
   * JSON pointer, Ctrl+F (or `/`) searches inside the value and Enter/F3 steps through
   * matches. References, GUIDs and `@key`s show what they read as next to the raw value
   * (7.14; hidden with "Show IDs"); a GUID that names a record follows like a reference.
   * Values are text only: nothing is rendered as HTML.
   */
  import { tick, untrack } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right';
  import { Input } from '@/components/ui/input';
  import { Button } from '@/components/ui/button';
  import { ancestors, at, copyText, initialOpen, jsonKey, scalarText, searchJson, splice, visibleLines, type Json, type JsonLine, type RefInfo } from '@/lib/jsontree';
  import type { LocText, Readable } from '@/lib/contract/dev';
  import { dev } from '@/lib/stores/dev.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';
  import { numberCell, unitOf } from '@/lib/readable';

  let {
    json,
    label,
    loc = [],
    labels = {},
    onfollow = null,
    onload = null,
    onloc = null,
    units = false,
    id = 'jt',
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
    /** Numbers read as the Diff shows them: grouped, with the unit their key says ("25,740 HP"). */
    units?: boolean;
    id?: string;
    class?: string;
  } = $props();

  const ROW = 24;
  const OVERSCAN = 10;

  let root = $state.raw<Json>(null);
  let open = $state.raw(new Set<string>());
  let active = $state('');
  $effect(() => {
    const text = json;
    untrack(() => {
      try {
        root = JSON.parse(text) as Json;
      } catch {
        root = text;
      }
      open = initialOpen(root);
      active = '';
      matches = [];
      if (scroller) scroller.scrollTop = 0;
    });
  });
  const lines = $derived(visibleLines(root, open));
  const locMap = $derived(new Map(loc.map((l) => [l.key, l.text])));
  const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  const labelOf = (v: unknown): Readable | null => (!dev.showIds && typeof v === 'string' ? (labels[v] ?? null) : null);

  let scroller = $state<HTMLElement | null>(null);
  let treeEl = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(400);
  $effect(() => {
    if (!scroller) return;
    const ro = new ResizeObserver(() => (viewH = scroller!.clientHeight));
    ro.observe(scroller);
    return () => ro.disconnect();
  });
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const last = $derived(Math.min(lines.length - 1, Math.ceil((scrollTop + viewH) / ROW) + OVERSCAN));
  const shown = $derived(lines.slice(first, last + 1).map((l, k) => ({ l, i: first + k })));
  const activeIndex = $derived(Math.max(0, lines.findIndex((l) => l.pointer === active)));
  const lineId = (i: number) => `${id}-l${i}`;

  function reveal(i: number) {
    if (!scroller) return;
    const top = i * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + scroller.clientHeight) scroller.scrollTop = top + ROW - scroller.clientHeight;
    scrollTop = scroller.scrollTop;
  }
  async function focusPointer(p: string) {
    active = p;
    await tick();
    reveal(lines.findIndex((l) => l.pointer === p));
  }
  function setOpen(p: string, v: boolean) {
    const n = new Set(open);
    if (v) n.add(p);
    else n.delete(p);
    open = n;
  }
  let loadingAt = $state<string | null>(null);
  async function load(p: string) {
    if (!onload || loadingAt) return;
    loadingAt = p;
    try {
      const v = await onload(p);
      root = splice(root, p, v);
      setOpen(p, true);
    } catch (e) {
      toast('error', 'Could not load this part', e instanceof Error ? e.message : String(e));
    } finally {
      loadingAt = null;
    }
  }
  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast('success', `Copied ${what}`);
    } catch {
      toast('error', 'The clipboard is not available here');
    }
  }

  function onKey(e: KeyboardEvent) {
    const cur = lines[activeIndex];
    if (!cur) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
      e.preventDefault();
      if (e.shiftKey) void copy(cur.pointer || '/', 'the JSON pointer');
      else void copy(copyText(at(root, cur.pointer) ?? null), 'the value');
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      findEl?.focus();
      return;
    }
    if (e.key === '/' && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      findEl?.focus();
      return;
    }
    if (e.key === 'F3') {
      e.preventDefault();
      step(e.shiftKey ? -1 : 1);
      return;
    }
    if (e.key === 'PageDown' || e.key === 'PageUp') {
      e.preventDefault();
      const per = Math.max(1, Math.floor(viewH / ROW) - 1);
      const j = Math.max(0, Math.min(lines.length - 1, activeIndex + (e.key === 'PageDown' ? per : -per)));
      void focusPointer(lines[j]!.pointer);
      return;
    }
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key === 'Enter' && cur.kind === 'value' && typeof cur.value === 'string' && GUID.test(cur.value) && labelOf(cur.value) && onfollow) {
      e.preventDefault();
      onfollow({ guid: cur.value, name: null, type: null, path: null });
      return;
    }
    const a = jsonKey(lines, cur.pointer, e.key);
    if (!a) return;
    e.preventDefault();
    if (a.kind === 'focus') void focusPointer(a.pointer);
    else if (a.kind === 'open') setOpen(a.pointer, true);
    else if (a.kind === 'close') setOpen(a.pointer, false);
    else if (a.kind === 'open-siblings') open = new Set([...open, ...a.pointers]);
    else if (a.kind === 'follow') onfollow?.(a.ref);
    else if (a.kind === 'load') void load(a.pointer);
  }

  // Search inside the record.
  let findEl = $state<HTMLInputElement | null>(null);
  let find = $state('');
  let matches = $state<string[]>([]);
  let at_ = $state(0);
  $effect(() => {
    const q = find;
    const t = setTimeout(() => {
      matches = searchJson(root, q);
      at_ = 0;
      if (matches.length) void goMatch(0);
    }, 150);
    return () => clearTimeout(t);
  });
  async function goMatch(k: number) {
    const p = matches[k];
    if (p === undefined) return;
    open = new Set([...open, ...ancestors(p)]);
    await tick();
    await focusPointer(p);
  }
  function step(d: number) {
    if (!matches.length) return;
    at_ = (at_ + d + matches.length) % matches.length;
    void goMatch(at_);
  }
  const matchSet = $derived(new Set(matches));

  function keyLabel(l: JsonLine) {
    return l.key === null ? label : /^\d+$/.test(l.key) && l.parent !== null && Array.isArray(at(root, l.parent)) ? `[${l.key}]` : l.key;
  }
  function summary(l: JsonLine) {
    if (l.kind === 'object') return `{${l.size}}`;
    if (l.kind === 'array') return `[${l.size}]`;
    if (l.kind === 'cut') return l.cutKind === 'array' ? `[${l.size}] …` : `{${l.size}} …`;
    return '';
  }
  function a11yName(l: JsonLine) {
    const k = keyLabel(l);
    if (l.kind === 'ref') {
      const lab = labelOf(l.ref?.guid);
      return `${k}: reference to ${lab ? `${lab.text}, ` : ''}${l.ref?.name ?? l.ref?.guid}${l.ref?.type ? `, ${l.ref.type}` : ''}`;
    }
    if (l.kind === 'value') {
      const lab = labelOf(l.value);
      return `${k}: ${scalarText(l.value)}${lab ? `, reads as ${lab.text}` : ''}`;
    }
    if (l.kind === 'cut') return `${k}: ${l.size} ${l.cutKind === 'array' ? 'items' : 'fields'}, not loaded`;
    return `${k}: ${l.size} ${l.kind === 'array' ? 'items' : 'fields'}`;
  }
</script>

<div class={cn('flex min-h-0 flex-col', className)}>
  <div class="flex items-center gap-2 border-b px-2 py-1.5" role="search" aria-label="Search in {label}">
    <div class="relative min-w-0 flex-1">
      <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2" aria-hidden="true" />
      <Input
        bind:ref={findEl}
        type="search"
        aria-label="Find in record"
        placeholder="Find in record (Ctrl+F)"
        class="h-7 pl-7 text-[12px]"
        bind:value={find}
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === 'Enter' || e.key === 'F3') {
            e.preventDefault();
            step(e.shiftKey ? -1 : 1);
          } else if (e.key === 'Escape') {
            e.preventDefault();
            treeEl?.focus();
          }
        }}
      />
    </div>
    <span class="text-muted-foreground mono shrink-0 text-[11px]" aria-live="polite">{find.trim() ? `${matches.length ? at_ + 1 : 0} / ${matches.length}` : ''}</span>
  </div>
  <div bind:this={scroller} class="relative min-h-0 flex-1 overflow-auto" onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      bind:this={treeEl}
      role="tree"
      aria-label={label}
      tabindex="0"
      aria-activedescendant={lineId(activeIndex)}
      class="focus-visible:ring-ring/60 relative min-w-max outline-none focus-visible:ring-2 focus-visible:ring-inset"
      style="height: {lines.length * ROW}px"
      onkeydown={onKey}
    >
      {#each shown as { l, i } (l.pointer)}
        {@const container = l.kind === 'object' || l.kind === 'array' || l.kind === 'cut'}
        {@const lab = l.kind === 'value' ? labelOf(l.value) : null}
        {@const text = !dev.showIds && l.kind === 'value' && typeof l.value === 'string' && (!lab || lab.kind === 'loc') ? (lab?.text ?? locMap.get(l.value)) : undefined}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id={lineId(i)}
          role="treeitem"
          aria-level={l.depth + 1}
          aria-expanded={l.kind === 'object' || l.kind === 'array' ? (l.size ? l.open : undefined) : l.kind === 'cut' ? false : undefined}
          aria-selected={i === activeIndex}
          aria-label={a11yName(l)}
          tabindex="-1"
          class={cn(
            'mono absolute right-0 left-0 flex items-center gap-1 pr-3 text-[12px] whitespace-nowrap',
            i === activeIndex && 'bg-accent/70',
            matchSet.has(l.pointer) && 'outline-primary/60 outline-1 -outline-offset-1',
          )}
          style="top: {i * ROW}px; height: {ROW}px; padding-left: {8 + l.depth * 14}px"
          onclick={() => {
            active = l.pointer;
            treeEl?.focus();
          }}
        >
          {#if container}
            <button
              type="button"
              tabindex="-1"
              aria-hidden="true"
              class="text-muted-foreground hover:text-foreground grid size-4 shrink-0 place-items-center"
              onclick={(e) => {
                e.stopPropagation();
                active = l.pointer;
                if (l.kind === 'cut') void load(l.pointer);
                else setOpen(l.pointer, !l.open);
              }}
            >
              <ChevronRight class={cn('size-3.5 transition-transform motion-reduce:transition-none', l.open && 'rotate-90')} />
            </button>
          {:else}
            <span class="w-4 shrink-0"></span>
          {/if}
          <span class={cn(l.key === null ? 'text-foreground font-semibold' : 'text-sky-700 dark:text-sky-300')}>{keyLabel(l)}</span>
          {#if l.key !== null || l.kind !== 'object'}<span class="text-muted-foreground">:</span>{/if}
          {#if l.kind === 'ref' && l.ref}
            {@const r = l.ref}
            {@const rl = labelOf(r.guid)}
            <button
              type="button"
              tabindex="-1"
              class={cn('text-primary inline-flex items-center gap-0.5 underline-offset-2 hover:underline', rl && 'font-sans')}
              title={`${rl ? `${rl.text}\n` : ''}${r.name ?? ''}\n${r.path ?? r.guid}`}
              onclick={(e) => {
                e.stopPropagation();
                onfollow?.(r);
              }}
            >
              <ArrowUpRight class="size-3" aria-hidden="true" />{rl ? rl.text : (r.name ?? r.guid)}
            </button>
            {#if rl && r.name}<span class="text-muted-foreground text-[11px]">{r.name}</span>{/if}
            {#if r.type}<span class="text-muted-foreground">({r.type})</span>{/if}
          {:else if l.kind === 'value'}
            <span class={cn(typeof l.value === 'string' ? 'text-emerald-700 dark:text-emerald-300' : typeof l.value === 'number' ? 'text-amber-700 dark:text-amber-300' : 'text-violet-700 dark:text-violet-300')}>{units && !dev.showIds && typeof l.value === 'number' ? numberCell(String(l.value), unitOf(l.key ?? ''), false) : scalarText(l.value)}</span>
            {#if text !== undefined}
              <button
                type="button"
                tabindex="-1"
                class="text-muted-foreground hover:text-foreground ml-1 max-w-[32rem] truncate font-sans italic underline-offset-2 hover:underline"
                title={lab?.enhanced ? `Open in Strings\nEnhanced: ${lab.enhanced}` : 'Open in Strings'}
                onclick={(e) => {
                  e.stopPropagation();
                  onloc?.(String(l.value));
                }}>“{text}”</button
              >
              {#if lab?.enhanced}<span class="text-primary font-sans text-[10px] font-semibold uppercase" title="Enhanced: {lab.enhanced}">enh</span>{/if}
            {:else if lab && GUID.test(String(l.value)) && (lab.kind === 'record' || lab.kind === 'tag') && onfollow}
              <button
                type="button"
                tabindex="-1"
                class="text-primary ml-1 inline-flex max-w-[32rem] items-center gap-0.5 truncate font-sans underline-offset-2 hover:underline"
                title={lab.detail ? `${lab.text} (${lab.detail})` : lab.text}
                onclick={(e) => {
                  e.stopPropagation();
                  onfollow?.({ guid: String(l.value), name: lab.detail, type: null, path: null });
                }}><ArrowUpRight class="size-3" aria-hidden="true" />{lab.text}</button
              >
            {:else if lab}
              <span class="text-muted-foreground ml-1 max-w-[32rem] truncate font-sans italic" title={lab.detail ?? lab.text}>{lab.text}</span>
            {/if}
          {:else}
            <span class="text-muted-foreground">{summary(l)}</span>
            {#if l.kind === 'cut'}
              <Button
                variant="link"
                size="xs"
                tabindex={-1}
                class="h-auto p-0 font-sans"
                onclick={(e: MouseEvent) => {
                  e.stopPropagation();
                  void load(l.pointer);
                }}>{loadingAt === l.pointer ? 'Loading…' : 'Load'}</Button
              >
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>
