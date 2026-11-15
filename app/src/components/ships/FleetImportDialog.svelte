<script lang="ts">
  /**
   * Fleet import (addendum §6.3, 6.6): choose a file (RSI+ / FleetYards / FleetView / hangar.link
   * export, HangarXPLOR JSON or CSV) or paste a list, see the dry-run report (hulls with counts,
   * unmatched, ambiguous with a pick per row, rename suggestions), then Import. Replace is the
   * default; "Add to my current fleet" merges. Nothing is written before Import. The real backend
   * opens the file picker inside the dry run and keeps the file for the Import step.
   */
  import FileJson from '@lucide/svelte/icons/file-json';
  import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import * as Dialog from '@/components/ui/dialog';
  import * as Alert from '@/components/ui/alert';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Label } from '@/components/ui/label';
  import Segmented from '@/components/common/Segmented.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, FleetHull, FleetImportReport } from '@/lib/contract/types';
  import { int } from '@/lib/format';
  import { toast } from '@/lib/stores/toast.svelte';

  let {
    open = $bindable(false),
    target,
    catalog,
    initialMode = 'file',
    ondone,
  }: { open: boolean; target: ChannelRef; catalog: FleetHull[]; initialMode?: 'file' | 'paste'; ondone: () => void } = $props();

  const uid = $props.id();
  let mode = $state<'file' | 'paste'>('file');
  let text = $state('');
  let merge = $state(false);
  let report = $state<FleetImportReport | null>(null);
  let busy = $state(false);
  let failed = $state<string | null>(null);
  /** Picks for ambiguous rows: row name -> hull. Applied right after the import. */
  let picks = $state<Record<string, string>>({});

  $effect(() => {
    if (open) {
      mode = initialMode;
      report = null;
      failed = null;
      picks = {};
    }
  });

  const hullName = (id: string) => catalog.find((h) => h.hull === id)?.name.trim() ?? id;
  const source = () => (mode === 'paste' ? { kind: 'text' as const, text } : { kind: 'file' as const });

  async function check() {
    busy = true;
    failed = null;
    try {
      report = await api.fleetImport({ target, source: source(), dry_run: true, merge });
    } catch (e) {
      const c = toContractError(e);
      if (c.code !== 'cancelled') failed = c.message;
    } finally {
      busy = false;
    }
  }

  async function confirm() {
    if (!report) return;
    busy = true;
    failed = null;
    try {
      const src = mode === 'paste' ? source() : { kind: 'last' as const };
      const r = await api.fleetImport({ target, source: src, dry_run: false, merge });
      for (const [name, hull] of Object.entries(picks)) await api.fleetMatch({ target, name, hull });
      const ships = r.hulls.reduce((n, h) => n + h.count, 0);
      const left = r.unmatched.length + r.ambiguous.length - Object.keys(picks).length;
      toast('success', `Imported ${ships} ships (${r.hulls.length} models)`, `${r.added_to_order} added to the end of My ships.${left > 0 ? ` ${left} still to match.` : ''} Nothing is written to the game until ship names are on and you apply.`);
      open = false;
      ondone();
    } catch (e) {
      failed = toContractError(e).message;
    } finally {
      busy = false;
    }
  }

  const suggestions = $derived(report ? report.hulls.filter((h) => h.names.length) : []);
</script>

<Dialog.Root bind:open={() => open, (v) => { if (!busy) open = v; }}>
  <Dialog.Content class="flex max-h-[min(88dvh,46rem)] flex-col overflow-hidden sm:max-w-2xl" showCloseButton={!busy}>
    <Dialog.Header>
      <Dialog.Title>Import your fleet</Dialog.Title>
      <Dialog.Description>
        Reads a fleet export or a list of ships and matches each one to a ship of {target.channel}. Your fleet replaces the one imported before; the order you set stays, new ships are added at the end.
      </Dialog.Description>
    </Dialog.Header>
    {#if !report}
      <div class="grid min-h-0 gap-3 overflow-y-auto">
        <Segmented
          ariaLabel="Import from"
          options={[{ id: 'file', label: 'A file' }, { id: 'paste', label: 'Paste a list' }]}
          value={mode}
          onchange={(v) => (mode = v)}
        />
        {#if mode === 'file'}
          <div class="text-muted-foreground grid gap-1.5 text-[12.5px]">
            <p><span class="text-foreground font-medium">RSI+:</span> My Hangar, then Export fleet, then HangarXPLOR JSON. FleetYards, FleetView (myfleet.gg) and hangar.link exports in the same format work too.</p>
            <p>CSV works, but a ship you named is listed under its name only, so it has to be matched by hand. Prefer JSON.</p>
            <p>Only ship names, counts and the names you gave your ships are kept. Pledge names, prices, dates and insurance are not stored.</p>
          </div>
        {:else}
          <div class="grid gap-1">
            <label for="{uid}-paste" class="text-[12.5px]">One ship per line; "2x Gladius" for two</label>
            <textarea
              id="{uid}-paste"
              bind:value={text}
              rows="8"
              spellcheck="false"
              placeholder={'Aegis Gladius\n2x Drake Cutlass Black\nF7A Hornet Mk II'}
              class="dark:bg-input/30 border-input focus-visible:border-ring focus-visible:ring-ring/50 mono min-h-36 w-full rounded-lg border bg-transparent px-2.5 py-2 text-[12.5px] outline-none focus-visible:ring-3"
            ></textarea>
          </div>
        {/if}
        <div class="flex items-start gap-2">
          <Checkbox id="{uid}-merge" checked={merge} onCheckedChange={(v) => (merge = v === true)} class="mt-0.5" />
          <Label for="{uid}-merge" class="leading-snug font-normal">Add to my current fleet instead of replacing it</Label>
        </div>
      </div>
    {:else}
      {@const r = report}
      <section aria-label="Dry-run report" class="-mr-2 grid min-h-0 flex-1 content-start gap-3 overflow-y-auto pr-2">
        <div class="flex items-start gap-2">
          {#if r.file}<FileJson class="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden="true" />{:else}<ClipboardPaste class="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden="true" />{/if}
          <div class="min-w-0">
            <div class="mono truncate text-[12px]" title={r.file ?? 'Pasted list'}>{r.file ?? 'Pasted list'}</div>
            <div class="text-muted-foreground text-[12px]">{r.format === 'xplor-json' ? 'HangarXPLOR JSON' : r.format === 'xplor-csv' ? 'HangarXPLOR CSV' : 'Plain list'} · {int(r.total)} rows · {r.skipped} not ships (paints, items) skipped{r.not_in_game ? ` · ${r.not_in_game} you marked "not in the game"` : ''}</div>
          </div>
        </div>
        {#if r.empty_catalog}
          <Alert.Root variant="warning">
            <TriangleAlert aria-hidden="true" />
            <Alert.Title>Nothing could be matched yet</Alert.Title>
            <Alert.Description>The game data of {target.channel} has not been read, so no ship names are known. Read it from the Ships screen, then import again.</Alert.Description>
          </Alert.Root>
        {/if}
        <dl class="grid grid-cols-3 gap-2">
          <div class="rounded-md border px-3 py-2"><dt class="text-muted-foreground text-[12px]">Matched ships</dt><dd class="mono text-[16px] font-semibold">{r.hulls.reduce((n, h) => n + h.count, 0)} <span class="text-muted-foreground text-[12px] font-normal">({r.hulls.length} models)</span></dd></div>
          <div class="rounded-md border px-3 py-2"><dt class="text-muted-foreground text-[12px]">Not matched</dt><dd class="mono text-[16px] font-semibold">{r.unmatched.length}</dd></div>
          <div class="rounded-md border px-3 py-2"><dt class="text-muted-foreground text-[12px]">More than one fits</dt><dd class="mono text-[16px] font-semibold">{r.ambiguous.length}</dd></div>
        </dl>
        {#if r.hulls.length}
          <div>
            <h3 class="mb-1 font-semibold">Your ships</h3>
            <ul class="grid max-h-56 gap-x-4 gap-y-0.5 overflow-y-auto rounded-md border p-2 text-[12.5px] sm:grid-cols-2">
              {#each r.hulls as h (h.hull)}
                <li class="flex min-w-0 items-baseline gap-2">
                  <span class="truncate" title={h.name}>{h.name}</span>
                  {#if h.count > 1}<span class="mono text-muted-foreground shrink-0 text-[11.5px]">×{h.count}</span>{/if}
                  {#if !h.new}<span class="text-muted-foreground shrink-0 text-[11.5px]">already listed</span>{/if}
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if suggestions.length}
          <div>
            <h3 class="mb-1 font-semibold">Names you gave your ships</h3>
            <p class="text-muted-foreground mb-1 text-[12px]">Kept as suggestions: nothing is renamed until you choose “Use” on the Ships screen.</p>
            <ul class="grid gap-0.5 text-[12.5px]">
              {#each suggestions as h (h.hull)}<li><span class="text-muted-foreground">{h.name}:</span> {h.names.map((n) => `“${n}”`).join(', ')}</li>{/each}
            </ul>
          </div>
        {/if}
        {#if r.ambiguous.length}
          <div class="grid gap-1.5">
            <h3 class="font-semibold">Pick the right ship</h3>
            <p class="text-muted-foreground text-[12px]">These names fit more than one ship (for example Mk I and Mk II), so none was guessed. Leave one open to decide later.</p>
            {#each r.ambiguous as a (a.name)}
              <div class="flex flex-wrap items-center gap-2">
                <span class="min-w-40 font-medium">{a.name}{a.count > 1 ? ` ×${a.count}` : ''}</span>
                <SelectField ariaLabel={`Ship for ${a.name}`} options={a.candidates.map((c) => ({ id: c, label: hullName(c) }))} value={picks[a.name] ?? null} placeholder="Decide later" onchange={(v) => (picks = { ...picks, [a.name]: v })} class="min-w-56" />
              </div>
            {/each}
          </div>
        {/if}
        {#if r.unmatched.length}
          <Alert.Root variant="warning">
            <TriangleAlert aria-hidden="true" />
            <Alert.Title>Not matched</Alert.Title>
            <Alert.Description>
              <p>No ship of {target.channel} has these names: concept ships, store names the game spells differently, or typos. They stay in a list to match by hand later.{#if r.format === 'xplor-csv'} A HangarXPLOR CSV gives a ship you named only by that name, so it lands here; the JSON export keeps both.{/if}</p>
              <ul class="max-h-32 list-inside list-disc overflow-y-auto text-[12px]">{#each r.unmatched as u (u.name)}<li>{u.name}{u.count > 1 ? ` ×${u.count}` : ''}</li>{/each}</ul>
            </Alert.Description>
          </Alert.Root>
        {/if}
      </section>
    {/if}
    {#if failed}<p role="alert" class="text-destructive">{failed}</p>{/if}
    <Dialog.Footer class="shrink-0">
      <Button variant="ghost" disabled={busy} onclick={() => (open = false)}>Cancel</Button>
      {#if !report}
        <Button disabled={busy || (mode === 'paste' && !text.trim())} aria-busy={busy} onclick={check}>
          {#if busy}<LoaderCircle class="animate-spin" aria-hidden="true" />Checking…{:else if mode === 'file'}<FileJson aria-hidden="true" /> Choose file and check{:else}Check list{/if}
        </Button>
      {:else}
        <Button variant="outline" disabled={busy} onclick={() => (report = null)}>Back</Button>
        <Button disabled={busy || report.empty_catalog || !report.hulls.length} aria-busy={busy} onclick={confirm}>
          {#if busy}<LoaderCircle class="animate-spin" aria-hidden="true" />Importing…{:else}{merge ? 'Add' : 'Import'} {report.hulls.length} model{report.hulls.length === 1 ? '' : 's'}{/if}
        </Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
