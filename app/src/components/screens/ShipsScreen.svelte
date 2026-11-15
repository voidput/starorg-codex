<script lang="ts">
  /**
   * Ships (addendum §6.6): your fleet as a short list you order and rename, with an optional sort
   * prefix so in-game ship lists follow your order. Left: My ships (drag or Alt+arrows; groups as
   * blocks; rename inline). Right: the in-game preview (the list as the game is believed to sort it,
   * after overrides) and All ships (the build's catalogue, to add more). Settings are saved with Save
   * / Save & apply like Enhancements; while the draft differs it is registered as `app.draft`.
   *
   * The preview is computed here (lib/shipnames.ts, same rules as the core, tested against shared
   * vectors), so a drag never waits for the back end.
   */
  import { onDestroy, tick, untrack } from 'svelte';
  import Rocket from '@lucide/svelte/icons/rocket';
  import FileUp from '@lucide/svelte/icons/file-up';
  import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste';
  import ListPlus from '@lucide/svelte/icons/list-plus';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import Pencil from '@lucide/svelte/icons/pencil';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import ArrowUpToLine from '@lucide/svelte/icons/arrow-up-to-line';
  import ArrowDownToLine from '@lucide/svelte/icons/arrow-down-to-line';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import X from '@lucide/svelte/icons/x';
  import Plus from '@lucide/svelte/icons/plus';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import Search from '@lucide/svelte/icons/search';
  import Save from '@lucide/svelte/icons/save';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Check from '@lucide/svelte/icons/check';
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Info from '@lucide/svelte/icons/info';
  import Database from '@lucide/svelte/icons/database';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import * as Card from '@/components/ui/card';
  import * as Alert from '@/components/ui/alert';
  import { Button } from '@/components/ui/button';
  import { Badge } from '@/components/ui/badge';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import Segmented from '@/components/common/Segmented.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import ActionDialog from '@/components/common/ActionDialog.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import FleetImportDialog from '@/components/ships/FleetImportDialog.svelte';
  import MatchDialog from '@/components/ships/MatchDialog.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { FleetHull, FleetView, NameStyle, ShipEntry, ShipGroup, ShipNamesScope, ShipNamesSettings } from '@/lib/contract/types';
  import * as names from '@/lib/shipnames';
  import { fold } from '@/lib/shipnames';
  import { fmtDateTime } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  interface Draft {
    scope: ShipNamesScope;
    settings: ShipNamesSettings;
  }
  type Row = { kind: 'group'; key: string; group: ShipGroup | null; count: number } | { kind: 'ship'; key: string; entry: ShipEntry; group: ShipGroup | null };

  let view = $state<FleetView | null>(null);
  let draft = $state<Draft | null>(null);
  let saved = $state('');
  let error = $state<ContractError | null>(null);
  let loadSeq = $state(0);
  let saving = $state(false);
  let reading = $state(false);
  let tab = $state<'preview' | 'all'>('preview');
  let mineOnly = $state(false);
  let search = $state('');
  let maker = $state<string>('*');
  let handPick = $state(false);
  let importOpen = $state(false);
  let importMode = $state<'file' | 'paste'>('file');
  let matchOpen = $state(false);
  let focusKey = $state<string | null>(null);
  let editing = $state<string | null>(null);
  let editValue = $state('');
  let editError = $state<string | null>(null);
  let grabbed = $state<{ hull: string; before: ShipNamesSettings } | null>(null);
  let dragHull = $state<string | null>(null);
  let dropKey = $state<string | null>(null);
  let said = $state('');
  let listEl = $state<HTMLElement | null>(null);

  const refOf = (k: string) => {
    const i = k.lastIndexOf('/');
    return { install: k.slice(0, i), channel: k.slice(i + 1) };
  };
  const dirty = $derived(draft !== null && JSON.stringify(draft) !== saved);
  const chName = $derived(app.channel?.channel ?? '');

  function absorb(v: FleetView, keepDraft = dirty) {
    view = v;
    const next: Draft = { scope: v.scope, settings: v.settings };
    if (keepDraft && draft) {
      // Unsaved edits stay; ships the back end added (import, match) join the draft at the end.
      let s = draft.settings;
      for (const e of v.settings.ships) s = names.addShip(s, e.hull);
      draft = { ...draft, settings: s };
    } else draft = structuredClone(next);
    saved = JSON.stringify(next);
  }

  $effect(() => {
    const k = app.targetKey;
    void loadSeq;
    if (!k) return;
    let stale = false;
    untrack(() => {
      error = null;
      api
        .fleetView({ target: refOf(k), read_game_data: false })
        .then((v) => !stale && absorb(v, false))
        .catch((e) => !stale && (error = toContractError(e)));
    });
    return () => (stale = true);
  });

  const handle = { label: 'Ships', save: () => save(false), discard: () => revert() };
  $effect(() => {
    app.draft = dirty ? handle : null;
  });
  onDestroy(() => {
    if (app.draft === handle) app.draft = null;
  });

  function revert() {
    if (saved) draft = JSON.parse(saved) as Draft;
    editing = null;
    grabbed = null;
  }

  async function save(andApply: boolean): Promise<boolean> {
    if (!draft || !app.target) return false;
    const target = $state.snapshot(app.target);
    saving = true;
    try {
      const d = $state.snapshot(draft) as Draft;
      absorb(await api.saveShipNames({ target, scope: d.scope, settings: d.settings }), false);
      toast('success', `Saved ship names for ${d.scope === 'shared' ? 'all channels' : target.channel}`, andApply ? 'Applying now.' : d.settings.enabled ? 'Apply to write them into the game.' : 'Ship names are off: nothing is written to the game.');
      await app.refresh();
      if (andApply) await app.apply();
      return true;
    } catch (e) {
      toast('error', 'Ship names not saved', toContractError(e).message);
      return false;
    } finally {
      saving = false;
    }
  }

  async function readGameData() {
    if (!app.target) return;
    reading = true;
    try {
      absorb(await api.fleetView({ target: $state.snapshot(app.target), read_game_data: true }));
    } catch (e) {
      toast('error', 'Could not read the game data', toContractError(e).message);
    } finally {
      reading = false;
    }
  }

  async function reload() {
    if (!app.target) return;
    try {
      absorb(await api.fleetView({ target: $state.snapshot(app.target), read_game_data: false }));
    } catch (e) {
      toast('error', 'Could not reload the fleet', toContractError(e).message);
    }
  }

  function edit(f: (s: ShipNamesSettings) => ShipNamesSettings) {
    if (!draft) return;
    draft = { ...draft, settings: f($state.snapshot(draft.settings) as ShipNamesSettings) };
  }
  const setOpt = <K extends keyof ShipNamesSettings>(k: K, v: ShipNamesSettings[K]) => edit((s) => ({ ...s, [k]: v }));

  // --- Derived views -----------------------------------------------------------------------
  const catalog = $derived(view?.catalog ?? []);
  const byHull = $derived(new Map(catalog.map((h) => [fold(h.hull), h])));
  const owned = $derived(new Map((view?.owned ?? []).map((o) => [fold(o.hull), o])));
  const overrides = $derived(new Map((view?.overrides ?? []).map((o) => [fold(o.hull), o])));
  const settings = $derived(draft?.settings ?? null);
  const plan = $derived(settings ? names.plan(settings, catalog) : null);
  const placed = $derived(settings ? names.ordered(settings) : []);
  const writing = $derived(!!draft && draft.scope !== 'off' && draft.settings.enabled);
  const rows = $derived.by((): Row[] => {
    if (!settings) return [];
    const out: Row[] = [];
    const showHeaders = settings.groups.length > 0;
    const blocks: (ShipGroup | null)[] = [null, ...settings.groups];
    for (const g of blocks) {
      const ships = placed.filter((p) => (p.group?.id ?? null) === (g?.id ?? null));
      if (showHeaders && (g || ships.length)) out.push({ kind: 'group', key: `g:${g?.id ?? ''}`, group: g, count: ships.length });
      for (const p of ships) out.push({ kind: 'ship', key: `s:${fold(p.entry.hull)}`, entry: p.entry, group: p.group });
    }
    return out;
  });
  const listed = $derived(new Set(placed.map((p) => fold(p.entry.hull))));
  const label = (hull: string) => byHull.get(fold(hull))?.name.trim() ?? hull;
  const finalName = (e: ShipEntry) => overrides.get(fold(e.hull))?.long ?? e.name ?? byHull.get(fold(e.hull))?.name.trim() ?? e.hull;
  const previewRows = $derived(settings ? names.preview(settings, catalog, view?.overrides ?? [], plan ?? undefined) : []);
  const shownPreview = $derived(mineOnly ? previewRows.filter((r) => r.listed) : previewRows);
  const makers = $derived([...new Set(catalog.map((h) => h.maker ?? '').filter(Boolean))].sort());
  const found = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return [...catalog]
      .filter((h) => (maker === '*' || h.maker === maker) && (!q || h.name.toLowerCase().includes(q) || (h.short ?? '').toLowerCase().includes(q)))
      .sort((a, b) => names.compareNames(a.name, b.name));
  });
  const emptyList = $derived(!!view && view.owned.length === 0 && (settings?.ships.length ?? 0) === 0 && !handPick);

  // --- List keyboard model ---------------------------------------------------------------
  async function focusRow(key: string | null) {
    focusKey = key;
    await tick();
    if (key) listEl?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)?.focus();
  }
  function say(hull: string) {
    if (settings) said = names.announce(settings, hull, label(hull));
  }
  async function moved(hull: string, f: (s: ShipNamesSettings) => ShipNamesSettings) {
    edit(f);
    say(hull);
    await focusRow(`s:${fold(hull)}`);
  }
  function startRename(key: string, current: string) {
    editing = key;
    editValue = current;
    editError = null;
    void tick().then(() => listEl?.querySelector<HTMLInputElement>(`[data-edit="${CSS.escape(key)}"]`)?.select());
  }
  async function commitRename(row: Row) {
    const v = editValue.trim();
    if (row.kind === 'ship') {
      const stock = byHull.get(fold(row.entry.hull))?.name.trim() ?? '';
      if (v && v !== stock && names.nameProblem(v)) {
        editError = names.nameProblem(v);
        return;
      }
      edit((s) => names.rename(s, row.entry.hull, v && v !== stock ? v : null));
      said = v && v !== stock ? `Renamed to ${v}` : `${stock}: stock name`;
    } else if (row.group) {
      if (v && names.nameProblem(v)) {
        editError = names.nameProblem(v);
        return;
      }
      const id = row.group.id;
      edit((s) => names.renameGroup(s, id, v));
    }
    editing = null;
    await focusRow(row.key);
  }
  function removeShip(hull: string) {
    if (!settings) return;
    const before = $state.snapshot(settings) as ShipNamesSettings;
    const i = rows.findIndex((r) => r.key === `s:${fold(hull)}`);
    edit((s) => names.removeShip(s, hull));
    said = `${label(hull)} removed from the list`;
    toast('info', `Removed ${label(hull)} from My ships`, 'Not saved yet.', { label: 'Undo', run: () => draft && (draft = { ...draft, settings: before }) });
    const next = rows[Math.min(i, rows.length - 1)];
    void focusRow(next?.key ?? null);
  }

  async function onRowKey(e: KeyboardEvent, row: Row) {
    // Keys typed in the rename box or on a row button are theirs.
    if (editing || e.target !== e.currentTarget) return;
    const i = rows.findIndex((r) => r.key === row.key);
    const k = e.key;
    const hull = row.kind === 'ship' ? row.entry.hull : null;
    if (grabbed) {
      e.preventDefault();
      if (k === 'ArrowUp' || k === 'ArrowDown') {
        if (hull) await moved(hull, (s) => names.moveBy(s, hull, k === 'ArrowUp' ? -1 : 1));
      } else if (k === ' ' || k === 'Enter') {
        said = `${label(grabbed.hull)} dropped. ${said}`;
        grabbed = null;
      } else if (k === 'Escape') {
        const g = grabbed;
        grabbed = null;
        edit(() => g.before);
        said = `${label(g.hull)} put back`;
        await focusRow(`s:${fold(g.hull)}`);
      }
      return;
    }
    if (e.altKey && (k === 'ArrowUp' || k === 'ArrowDown')) {
      e.preventDefault();
      const d = k === 'ArrowUp' ? -1 : 1;
      if (hull) await moved(hull, (s) => (e.shiftKey ? names.moveToBlock(s, hull, d) : names.moveBy(s, hull, d)));
      else if (row.kind === 'group' && row.group) {
        const id = row.group.id;
        edit((s) => names.moveGroup(s, id, d));
        said = `Group ${row.group.name} moved`;
        await focusRow(row.key);
      }
      return;
    }
    if (e.altKey && (k === 'Home' || k === 'End')) {
      e.preventDefault();
      if (hull) await moved(hull, (s) => names.moveToEnd(s, hull, k === 'Home' ? 'top' : 'bottom'));
      return;
    }
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    if (k === 'ArrowUp' || k === 'ArrowDown' || k === 'Home' || k === 'End') {
      e.preventDefault();
      const j = k === 'Home' ? 0 : k === 'End' ? rows.length - 1 : Math.max(0, Math.min(rows.length - 1, i + (k === 'ArrowUp' ? -1 : 1)));
      await focusRow(rows[j]?.key ?? null);
    } else if (k === 'F2' || k === 'Enter') {
      e.preventDefault();
      if (row.kind === 'ship') startRename(row.key, row.entry.name ?? label(row.entry.hull));
      else if (row.group) startRename(row.key, row.group.name);
    } else if (k === 'Delete' || k === 'Backspace') {
      e.preventDefault();
      if (hull) removeShip(hull);
      else if (row.kind === 'group' && row.group) deleteGroup(row.group);
    } else if (k === ' ' && hull) {
      e.preventDefault();
      grabbed = { hull, before: $state.snapshot(settings!) as ShipNamesSettings };
      said = `${label(hull)} picked up. Up and Down move it, Space drops it, Escape puts it back.`;
    }
  }

  function deleteGroup(g: ShipGroup) {
    if (!settings) return;
    const before = $state.snapshot(settings) as ShipNamesSettings;
    edit((s) => names.deleteGroup(s, g.id));
    said = `Group ${g.name} deleted; its ships have no group now`;
    toast('info', `Deleted the group ${g.name}`, 'Its ships have no group now. Not saved yet.', { label: 'Undo', run: () => draft && (draft = { ...draft, settings: before }) });
  }
  async function newGroup() {
    if (!settings) return;
    let id = '';
    edit((s) => {
      const [n, gid] = names.addGroup(s, `Group ${s.groups.length + 1}`);
      id = gid;
      return n;
    });
    await tick();
    const g = settings.groups.find((x) => x.id === id);
    if (g) startRename(`g:${id}`, g.name);
  }

  // --- Drag and drop ---------------------------------------------------------------------
  function onDragStart(e: DragEvent, hull: string) {
    dragHull = hull;
    e.dataTransfer?.setData('text/plain', hull);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onDragOver(e: DragEvent, row: Row) {
    if (!dragHull) return;
    e.preventDefault();
    dropKey = row.key;
  }
  async function onDrop(e: DragEvent, row: Row) {
    e.preventDefault();
    const hull = dragHull;
    dragHull = null;
    dropKey = null;
    if (!hull) return;
    if (row.kind === 'group') await moved(hull, (s) => names.placeIn(s, hull, row.group?.id ?? null, 'first'));
    else if (fold(row.entry.hull) !== fold(hull)) {
      const before = row.entry.hull;
      const gid = row.group?.id ?? null;
      await moved(hull, (s) => names.placeIn(s, hull, gid, { before }));
    }
  }

  // --- Scope -----------------------------------------------------------------------------
  // Leaving a channel's own list asks first, in the app's own dialog (native confirm() is
  // not dependable in every webview).
  let sharedAsk = $state(false);
  function setScope(next: ShipNamesScope, asked = false) {
    if (!draft || !view) return;
    if (next === 'shared' && draft.scope === 'channel' && !asked) {
      sharedAsk = true;
      return;
    }
    draft = {
      scope: next,
      settings: next === 'shared' && draft.scope !== 'shared' ? structuredClone($state.snapshot(view.shared_settings) as ShipNamesSettings) : draft.settings,
    };
  }

  const styles: { id: NameStyle; label: string; hint: string }[] = [
    { id: 'none', label: 'Rename only', hint: 'No prefix: only the names you change are written.' },
    { id: 'numeric', label: '01', hint: '01, 02, … in your order. Zero-padded so 2 sorts before 10.' },
    { id: 'group-letter', label: 'A1', hint: 'A letter per group and the place in it: A1, A2, B1.' },
    { id: 'symbol', label: 'Symbol', hint: 'The same symbol on every ship of your list: they float above the rest, in name order.' },
  ];
  const separators = [
    { id: ' ', label: 'Space' },
    { id: '-', label: 'Dash' },
    { id: '. ', label: 'Dot' },
  ];
  const symbols = names.SYMBOLS.map((s) => ({ id: s as string, label: s as string }));
</script>

{#if error}
  <ErrorState title="Could not read your fleet" {error} onretry={() => loadSeq++} />
{:else if !view || !draft || !settings}
  <div class="grid gap-3"><Skeleton class="h-24 w-full" /><Skeleton class="h-72 w-full" /></div>
{:else if !view.catalog_ready}
  <FullEmpty icon={Database} title={`Read the game data of ${chName} first`}>
    The list of ships comes from the game files of this build. Reading them takes up to a minute and writes nothing to the game.
    {#snippet actions()}
      <Button disabled={reading} aria-busy={reading} onclick={readGameData}>
        {#if reading}<LoaderCircle class="animate-spin" aria-hidden="true" /> Reading…{:else}<Database aria-hidden="true" /> Read game data{/if}
      </Button>
    {/snippet}
  </FullEmpty>
{:else if emptyList}
  <FullEmpty icon={Rocket} title="Make your own ship list">
    <p>Import your fleet to get a short list of your ships, then put them in the order you want them in game lists and give them names.</p>
    <p class="mt-2 text-[12.5px]">In RSI+: My Hangar, then Export fleet (HangarXPLOR JSON). FleetYards, FleetView and hangar.link exports work too.</p>
    {#snippet actions()}
      <Button onclick={() => ((importMode = 'file'), (importOpen = true))}><FileUp aria-hidden="true" /> Import fleet</Button>
      <Button variant="outline" onclick={() => ((importMode = 'paste'), (importOpen = true))}><ClipboardPaste aria-hidden="true" /> Paste a list</Button>
      <Button variant="outline" onclick={() => ((handPick = true), (tab = 'all'))}><ListPlus aria-hidden="true" /> Pick ships by hand</Button>
    {/snippet}
  </FullEmpty>
{:else}
  {@const d = draft}
  {@const s = settings}
  <div class="@container grid max-w-[1760px] gap-4">
    <div class="flex flex-wrap items-center gap-2" role="toolbar" aria-label="Ship names">
      <Button size="sm" variant="outline" onclick={() => ((importMode = 'file'), (importOpen = true))}><FileUp aria-hidden="true" /> Import fleet</Button>
      {#if view.imported_at}<span class="text-muted-foreground text-[12px]">Fleet imported {fmtDateTime(view.imported_at)} · {view.owned.reduce((n, o) => n + o.count, 0)} ships</span>{/if}
      <span class="flex-1"></span>
      <span class={cn('text-[12px]', dirty ? 'text-warning' : 'text-muted-foreground')} aria-live="polite">{dirty ? 'Unsaved changes' : 'Saved'}</span>
      <Button variant="ghost" size="sm" disabled={!dirty || saving} onclick={revert}><Undo2 aria-hidden="true" /> Revert</Button>
      <Button variant="outline" size="sm" disabled={!dirty || saving} onclick={() => save(false)}><Save aria-hidden="true" /> Save</Button>
      <Button size="sm" disabled={saving || !!app.blocked || !app.channel?.set_up} title={app.blocked ?? undefined} onclick={() => save(true)}><Check aria-hidden="true" /> Save &amp; apply</Button>
    </div>

    <Card.Root>
      <Card.Content class="grid gap-3">
        <SwitchRow id="sn-enabled" checked={s.enabled} disabled={d.scope === 'off'} onchange={(v) => setOpt('enabled', v)}
          label="Write ship names into the game: order and rename ships in game lists"
          hint="The prefix and new names show everywhere the game shows the ship's name, not only in lists: HUD target labels, contracts, hangar and insurance screens, and NPC ships of the same model.">
          <Badge variant="warning" title="In-game ship lists are believed to sort by the ship's name, so a prefix orders them. Not confirmed yet; renaming works either way."><FlaskConical aria-hidden="true" /> Experimental</Badge>
        </SwitchRow>
        <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
          <span class="flex items-center gap-2"><span class="text-muted-foreground text-[12.5px]">Applies to</span>
            <SelectField ariaLabel="Applies to" options={[{ id: 'shared', label: 'All channels' }, { id: 'channel', label: `Only ${chName}` }, { id: 'off', label: `Off for ${chName}` }]} value={d.scope} onchange={(v) => setScope(v as ShipNamesScope)} class="min-w-40" /></span>
          <span class="flex items-center gap-2"><span class="text-muted-foreground text-[12.5px]">Prefix</span>
            <Segmented size="sm" ariaLabel="Prefix style" options={styles} value={s.style} onchange={(v) => setOpt('style', v)} /></span>
          {#if s.style !== 'none'}
            <span class="flex items-center gap-2"><span class="text-muted-foreground text-[12.5px]">Separator</span>
              <Segmented size="sm" ariaLabel="Separator after the prefix" options={separators} value={s.separator} onchange={(v) => setOpt('separator', v)} /></span>
          {/if}
          {#if s.style === 'symbol'}
            <span class="flex items-center gap-2"><span class="text-muted-foreground text-[12.5px]">Symbol</span>
              <Segmented size="sm" ariaLabel="Symbol" options={symbols} value={s.symbol} onchange={(v) => setOpt('symbol', v)} /></span>
          {/if}
        </div>
        <div class="grid gap-x-6 gap-y-0.5 @min-[760px]:grid-cols-2">
          <SwitchRow id="sn-rest" size="sm" label="Number all other ships too" disabled={s.style !== 'numeric'} checked={s.rest === 'numbered'} onchange={(v) => setOpt('rest', v ? 'numbered' : 'stock')}
            hint="Off: only your ships get a number and float above the rest." />
          <SwitchRow id="sn-short" size="sm" label="Prefix short names too" disabled={s.style === 'none'} checked={s.prefix_short} onchange={(v) => setOpt('prefix_short', v)}
            hint="Short names show on small labels; which lists use them is not known yet." />
        </div>
        {#if s.style === 'group-letter'}
          <p class="text-muted-foreground flex items-start gap-1.5 text-[12px]"><Info class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" /> Letter prefixes sort among names that start with the same letter (A1 next to “Aegis …”); the preview shows where they land.</p>
        {/if}
      </Card.Content>
    </Card.Root>

    {#if view.invalid || view.warnings.length}
      <Alert.Root variant="warning">
        <TriangleAlert aria-hidden="true" />
        <Alert.Title>{view.invalid ? 'The saved ship-name settings cannot be used' : 'Some saved entries were left out'}</Alert.Title>
        <Alert.Description>
          <ul class="list-inside list-disc text-[12px]">{#each view.warnings as w (w)}<li>{w}</li>{/each}</ul>
          {#if view.invalid}<p class="mt-1">Nothing is written until you save valid settings here ({view.settings_file}).</p>{/if}
        </Alert.Description>
      </Alert.Root>
    {/if}
    {#if !writing}
      <div class="bg-muted/40 flex items-start gap-2 rounded-md border p-3 text-[12.5px]" role="status">
        <Info class="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden="true" />
        <span>{d.scope === 'off' ? `Ship names are off for ${chName}: nothing is written there.` : 'Ship names are off: you can arrange the list now; nothing is written to the game until you turn them on and apply.'}</span>
      </div>
    {/if}
    {#if view.pending.length}
      <div class="border-warning/40 bg-warning/10 flex flex-wrap items-center gap-2 rounded-md border p-3 text-[12.5px]" role="status">
        <TriangleAlert class="text-warning size-4 shrink-0" aria-hidden="true" />
        <span class="flex-1">{view.pending.length} {view.pending.length === 1 ? 'ship' : 'ships'} from your import {view.pending.length === 1 ? 'is' : 'are'} not matched yet.</span>
        <Button size="sm" variant="outline" onclick={() => (matchOpen = true)}>Review</Button>
      </div>
    {/if}

    <div class="grid gap-4 @min-[980px]:grid-cols-[minmax(0,1.15fr)_minmax(320px,1fr)]">
      <Card.Root class="min-w-0 self-start">
        <Card.Header>
          <Card.Title>My ships <span class="text-muted-foreground font-normal">({placed.length})</span></Card.Title>
          <Card.Description class="text-[12px]">Drag to reorder, or focus a ship: Alt+↑/↓ moves it, Alt+Shift+↑/↓ to another group, F2 renames, Delete removes.</Card.Description>
        </Card.Header>
        <Card.Content class="grid gap-2">
          {#if !rows.length}
            <p class="text-muted-foreground py-6 text-center">No ships in your list yet. Add them from All ships.</p>
          {/if}
          <ol bind:this={listEl} role="listbox" aria-label="My ships, in order" aria-describedby="sn-keys" class="-mr-2 grid max-h-[min(70dvh,52rem)] content-start gap-0.5 overflow-y-auto pr-2">
            {#each rows as row, ri (row.key)}
              {@const active = (focusKey ?? rows[0]?.key) === row.key}
              {#if row.kind === 'group'}
                <li
                  data-key={row.key}
                  role="option"
                  aria-selected={active}
                  tabindex={active ? 0 : -1}
                  aria-label={row.group ? `Group ${row.group.name}, ${row.count} ships` : `No group, ${row.count} ships`}
                  class={cn('group/row focus-visible:ring-ring/60 mt-2 flex min-h-8 items-center gap-2 rounded-md border-b px-1.5 text-[12px] font-semibold tracking-wide uppercase outline-none focus-visible:ring-2', dropKey === row.key && 'bg-primary/10')}
                  onfocus={() => (focusKey = row.key)}
                  onkeydown={(e) => onRowKey(e, row)}
                  ondragover={(e) => onDragOver(e, row)}
                  ondragleave={() => dropKey === row.key && (dropKey = null)}
                  ondrop={(e) => onDrop(e, row)}
                >
                  {#if editing === row.key}
                    <Input data-edit={row.key} class="h-7 max-w-64 normal-case" aria-label="Group name" aria-invalid={!!editError} bind:value={editValue}
                      onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void commitRename(row); } else if (e.key === 'Escape') { e.preventDefault(); editing = null; void focusRow(row.key); } }}
                      onblur={() => editing === row.key && commitRename(row)} />
                    {#if editError}<span class="text-destructive text-[12px] normal-case" role="alert">{editError}</span>{/if}
                  {:else}
                    <span class={cn('truncate', row.group ? 'text-foreground/90' : 'text-muted-foreground')}>{row.group?.name ?? 'No group'}</span>
                    <span class="text-muted-foreground font-normal">{row.count}</span>
                    {#if row.group}
                      {@const g = row.group}
                      <span class="ml-auto flex gap-0.5 opacity-0 group-focus-within/row:opacity-100 group-hover/row:opacity-100">
                        <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Move group ${g.name} up`} onclick={() => edit((x) => names.moveGroup(x, g.id, -1))}><ChevronUp aria-hidden="true" /></Button>
                        <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Move group ${g.name} down`} onclick={() => edit((x) => names.moveGroup(x, g.id, 1))}><ChevronDown aria-hidden="true" /></Button>
                        <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Rename group ${g.name}`} onclick={() => startRename(row.key, g.name)}><Pencil aria-hidden="true" /></Button>
                        <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Delete group ${g.name}`} onclick={() => deleteGroup(g)}><X aria-hidden="true" /></Button>
                      </span>
                    {/if}
                  {/if}
                </li>
              {:else}
                {@const e = row.entry}
                {@const h = byHull.get(fold(e.hull))}
                {@const lab = plan?.labels.get(fold(e.hull))}
                {@const own = owned.get(fold(e.hull))}
                {@const ov = overrides.get(fold(e.hull))}
                <li
                  data-key={row.key}
                  data-hull={e.hull}
                  role="option"
                  aria-selected={active}
                  tabindex={active ? 0 : -1}
                  draggable="true"
                  aria-label={`${finalName(e)}${lab ? `, prefix ${lab.label || 'none'}` : ''}${own ? `, you own ${own.count}` : ''}${!h ? `, not in ${chName}` : ''}`}
                  aria-roledescription="sortable ship"
                  class={cn(
                    'group/row focus-visible:ring-ring/60 hover:bg-muted/50 flex min-h-9 items-center gap-2 rounded-md px-1 py-0.5 outline-none focus-visible:ring-2',
                    grabbed && fold(grabbed.hull) === fold(e.hull) && 'bg-primary/10 ring-primary/50 ring-2',
                    dropKey === row.key && 'border-primary border-t-2',
                    dragHull && fold(dragHull) === fold(e.hull) && 'opacity-50',
                  )}
                  onfocus={() => (focusKey = row.key)}
                  onkeydown={(ev) => onRowKey(ev, row)}
                  ondragstart={(ev) => onDragStart(ev, e.hull)}
                  ondragend={() => ((dragHull = null), (dropKey = null))}
                  ondragover={(ev) => onDragOver(ev, row)}
                  ondragleave={() => dropKey === row.key && (dropKey = null)}
                  ondrop={(ev) => onDrop(ev, row)}
                >
                  <span class="text-muted-foreground cursor-grab" aria-hidden="true"><GripVertical class="size-4" /></span>
                  <span class={cn('mono w-10 shrink-0 text-right text-[12px]', writing ? 'text-primary' : 'text-muted-foreground')} title="Prefix">{lab?.label ?? ''}</span>
                  <div class="grid min-w-0 flex-1">
                    {#if editing === row.key}
                      <Input data-edit={row.key} class="h-7 max-w-md" aria-label={`New name for ${label(e.hull)} (empty = stock name)`} aria-invalid={!!editError} bind:value={editValue}
                        onkeydown={(ev) => { if (ev.key === 'Enter') { ev.preventDefault(); void commitRename(row); } else if (ev.key === 'Escape') { ev.preventDefault(); editing = null; void focusRow(row.key); } }}
                        onblur={() => editing === row.key && commitRename(row)} />
                      {#if editError}<span class="text-destructive text-[12px]" role="alert">{editError}</span>{/if}
                    {:else}
                      <span class="flex min-w-0 items-baseline gap-2">
                        <span class={cn('truncate', !h && 'text-muted-foreground')}>{finalName(e)}</span>
                        {#if (e.name || ov?.long) && h}<span class="text-muted-foreground truncate text-[12px]">{h.name.trim()}</span>{/if}
                      </span>
                      {#if own?.names.length && !e.name}
                        <span class="flex flex-wrap items-center gap-1">
                          {#each own.names as n (n)}
                            {@const bad = names.nameProblem(n)}
                            <button type="button" tabindex={-1} disabled={!!bad} title={bad ? `Cannot be used as a name: ${bad}` : undefined} class="border-primary/40 text-primary hover:bg-primary/10 rounded-full border px-1.5 text-[11.5px] disabled:pointer-events-none disabled:opacity-50" onclick={() => { if (!bad) edit((x) => names.rename(x, e.hull, n)); }}>Use “{n}”</button>
                          {/each}
                          {#if own.names.length > 1}<span class="text-muted-foreground text-[11.5px]">One model has one name in the game.</span>{/if}
                        </span>
                      {/if}
                    {/if}
                  </div>
                  {#if ov}<Badge variant="info" title={`An edit in Strings replaces this name${ov.long ? '' : ' (short name)'}; it wins over the prefix and rename.`}>Overridden in Strings</Badge>{/if}
                  {#if !h}<Badge variant="warning">Not in {chName}</Badge>{/if}
                  <span class="mono text-muted-foreground w-8 shrink-0 text-right text-[12px]" title={own ? `You own ${own.count}` : 'Added by hand'}>{own ? `×${own.count}` : ''}</span>
                  <span class="flex w-[9.5rem] shrink-0 justify-end gap-0.5 opacity-0 group-focus-within/row:opacity-100 group-hover/row:opacity-100">
                    <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Rename ${label(e.hull)}`} onclick={() => startRename(row.key, e.name ?? label(e.hull))}><Pencil aria-hidden="true" /></Button>
                    {#if e.name}<Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Reset ${label(e.hull)} to its stock name`} onclick={() => edit((x) => names.rename(x, e.hull, null))}><RotateCcw aria-hidden="true" /></Button>{/if}
                    <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Move ${label(e.hull)} to the top`} disabled={ri === 0} onclick={() => moved(e.hull, (x) => names.moveToEnd(x, e.hull, 'top'))}><ArrowUpToLine aria-hidden="true" /></Button>
                    <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Move ${label(e.hull)} to the bottom`} onclick={() => moved(e.hull, (x) => names.moveToEnd(x, e.hull, 'bottom'))}><ArrowDownToLine aria-hidden="true" /></Button>
                    <Button tabindex={-1} size="icon-xs" variant="ghost" aria-label={`Remove ${label(e.hull)} from the list`} onclick={() => removeShip(e.hull)}><X aria-hidden="true" /></Button>
                  </span>
                </li>
              {/if}
            {/each}
          </ol>
          <p id="sn-keys" class="sr-only">Up and Down move the focus. Alt+Up and Alt+Down move the ship. Alt+Home and Alt+End move it to the top or bottom. Alt+Shift+Up and Down move it to the previous or next group. F2 renames, Delete removes, Space picks it up.</p>
          <p class="sr-only" aria-live="polite">{said}</p>
          <div class="flex flex-wrap gap-2 border-t pt-2">
            <Button size="sm" variant="ghost" onclick={newGroup}><FolderPlus aria-hidden="true" /> New group</Button>
            <Button size="sm" variant="ghost" onclick={() => (tab = 'all')}><Plus aria-hidden="true" /> Add ships</Button>
            {#if plan?.notInBuild.length}<span class="text-muted-foreground self-center text-[12px]">{plan.notInBuild.length} not in {chName}: skipped there.</span>{/if}
          </div>
        </Card.Content>
      </Card.Root>

      <Card.Root class="min-w-0 self-start @min-[980px]:sticky @min-[980px]:top-0">
        <Card.Header class="gap-2">
          <div role="tablist" aria-label="Right panel" class="flex gap-1">
            {#each [{ id: 'preview', label: 'In-game preview' }, { id: 'all', label: 'All ships' }] as t (t.id)}
              <button
                type="button"
                role="tab"
                id="sn-tab-{t.id}"
                aria-selected={tab === t.id}
                aria-controls="sn-panel"
                tabindex={tab === t.id ? 0 : -1}
                class={cn('focus-visible:ring-ring/60 h-8 rounded-md px-2.5 outline-none focus-visible:ring-2', tab === t.id ? 'bg-accent text-foreground font-medium' : 'hover:bg-muted text-foreground/85')}
                onclick={() => (tab = t.id as 'preview' | 'all')}
                onkeydown={(e) => {
                  if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
                    e.preventDefault();
                    tab = tab === 'preview' ? 'all' : 'preview';
                    void tick().then(() => document.getElementById(`sn-tab-${tab}`)?.focus());
                  }
                }}>{t.label}</button
              >
            {/each}
          </div>
        </Card.Header>
        <Card.Content id="sn-panel" role="tabpanel" aria-labelledby="sn-tab-{tab}" class="grid gap-2">
          {#if tab === 'preview'}
            <div class="flex flex-wrap items-center gap-2">
              <p class="text-muted-foreground flex-1 text-[12px]">Sorted as the game is believed to sort: case-insensitive, letter by letter. Not confirmed yet. Overrides from Strings included.</p>
              <SwitchRow id="sn-mine" size="sm" label="My ships only" checked={mineOnly} onchange={(v) => (mineOnly = v)} />
            </div>
            <ol aria-label="Ship list as the game would sort it" class="-mr-2 grid max-h-[min(66dvh,48rem)] content-start overflow-y-auto pr-2">
              {#each shownPreview as r (r.hull)}
                <li class={cn('flex min-h-7 items-center gap-2 border-b border-dashed px-1 text-[12.5px] last:border-0', !r.listed && 'text-muted-foreground')}>
                  <span class="mono truncate">{r.value}</span>
                  {#if r.overridden}<Badge variant="info">Overridden</Badge>{/if}
                  {#if !r.listed && !mineOnly}<span class="ml-auto shrink-0 text-[11.5px]">not in my list</span>{/if}
                </li>
              {/each}
            </ol>
            {#if !writing}<p class="text-muted-foreground text-[12px]">Preview only: ship names are off.</p>{/if}
          {:else}
            <div class="flex flex-wrap items-center gap-2">
              <div class="relative min-w-40 flex-1">
                <Search class="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden="true" />
                <Input type="search" aria-label="Search ships" placeholder="Search ships" class="h-8 pl-8" bind:value={search} />
              </div>
              <SelectField ariaLabel="Maker" options={[{ id: '*', label: 'All makers' }, ...makers.map((m) => ({ id: m, label: m.toUpperCase() }))]} value={maker} onchange={(v) => (maker = v)} class="min-w-32" />
            </div>
            <p class="text-muted-foreground text-[12px]">{found.length} of {catalog.length} ships of {chName}. Enter adds the focused ship.</p>
            {#if !found.length}
              <div class="text-muted-foreground py-6 text-center">No ship matches. <Button size="sm" variant="link" onclick={() => ((search = ''), (maker = '*'))}>Clear</Button></div>
            {/if}
            <ul aria-label="All ships of this build" class="-mr-2 grid max-h-[min(62dvh,46rem)] content-start overflow-y-auto pr-2">
              {#each found as h (h.hull)}
                {@const isListed = listed.has(fold(h.hull))}
                <li class="flex min-h-8 items-center gap-2 border-b border-dashed px-1 text-[12.5px] last:border-0">
                  <span class="min-w-0 flex-1 truncate" title={h.name}>{h.name.trim()}</span>
                  <span class="text-muted-foreground mono shrink-0 text-[11.5px]">{(h.maker ?? '').toUpperCase()}</span>
                  <span class="text-muted-foreground w-12 shrink-0 text-[11.5px]">{h.ground ? 'ground' : 'ship'}</span>
                  {#if owned.has(fold(h.hull))}<span class="mono text-muted-foreground shrink-0 text-[11.5px]">×{owned.get(fold(h.hull))?.count}</span>{/if}
                  <Button size="xs" variant={isListed ? 'ghost' : 'outline'} disabled={isListed} aria-label={isListed ? `${h.name.trim()} is in your list` : `Add ${h.name.trim()} to My ships`}
                    onclick={() => { edit((x) => names.addShip(x, h.hull)); said = `${h.name.trim()} added at position ${placed.length + 1}`; }}>{isListed ? 'Listed' : 'Add'}</Button>
                </li>
              {/each}
            </ul>
          {/if}
        </Card.Content>
      </Card.Root>
    </div>
  </div>
{/if}

{#if view && app.target}
  <FleetImportDialog bind:open={importOpen} target={app.target} catalog={view.catalog} initialMode={importMode} ondone={() => { handPick = false; void reload(); }} />
  <ActionDialog bind:open={sharedAsk} title="Use the shared list again?" description={`${chName} goes back to the list shared by all channels. ${chName}'s own list is dropped when you save.`} confirmLabel="Use the shared list" onconfirm={async () => setScope('shared', true)} />
  <MatchDialog bind:open={matchOpen} target={app.target} {view} onchange={(v) => absorb(v)} />
{/if}
