<script lang="ts">
  /**
   * Re-scan the game logs and re-import from Smart Citizen (fix for stores where a reset
   * character showed twice or nothing was marked owned). Each action runs a dry run first,
   * shows what changes per character, then confirms. Nothing from StarOrg Tools or another
   * source is removed unless "replace" is ticked. Self-contained: the host passes the
   * channel, its CharactersView and the character shown, and refreshes on `ondone`.
   */
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import FileInput from '@lucide/svelte/icons/file-input';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import * as Card from '@/components/ui/card';
  import * as Dialog from '@/components/ui/dialog';
  import * as Alert from '@/components/ui/alert';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Label } from '@/components/ui/label';
  import SelectField from '@/components/common/SelectField.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, CharactersView, ImportReport, RescanReport } from '@/lib/contract/types';
  import { toast } from '@/lib/stores/toast.svelte';
  import { characterLabel, newestId } from '@/lib/characters';

  let { target, view, shown, ondone }: { target: ChannelRef; view: CharactersView; shown: string | null; ondone: () => void } = $props();

  type Mode = 'rescan' | 'reimport';
  let mode = $state<Mode | null>(null);
  let replace = $state(false);
  let character = $state<string | null>(null);
  let rescan = $state<RescanReport | null>(null);
  let reimport = $state<ImportReport | null>(null);
  let busy = $state(false);
  let failed = $state<string | null>(null);
  let open = $state(false);

  const chars = $derived(Array.isArray(view.characters) ? view.characters : []);
  const name = (id: string | null) => chars.find((c) => c.id === id)?.name ?? '—';
  const rescanChanges = $derived(rescan ? rescan.characters.filter((c) => c.added.length || c.removed.length) : []);

  function start(m: Mode) {
    mode = m;
    replace = false;
    // With the same-name notice, the notice says: re-import into the current (newest) one.
    character = (view.notice ? newestId(chars) : null) ?? shown ?? view.active.resolved ?? chars[0]?.id ?? null;
    rescan = null;
    reimport = null;
    failed = null;
    open = true;
  }

  async function preview() {
    busy = true;
    failed = null;
    try {
      if (mode === 'rescan') rescan = await api.rescanLogs({ target, dry_run: true, replace });
      else if (character) reimport = await api.importSmartCitizen({ target, character, dry_run: true, reuse_last: true, replace });
    } catch (e) {
      failed = toContractError(e).message;
    } finally {
      busy = false;
    }
  }

  async function confirm() {
    busy = true;
    failed = null;
    try {
      if (mode === 'rescan') {
        const r = await api.rescanLogs({ target, dry_run: false, replace });
        const n = r.characters.reduce((s, c) => s + c.added.length + c.removed.length, 0);
        toast('success', `Game logs re-read: ${n} change${n === 1 ? '' : 's'}`, 'Re-apply to update the [Owned] tags in game.');
      } else if (character) {
        const r = await api.importSmartCitizen({ target, character, dry_run: false, reuse_last: true, replace });
        toast('success', `Re-imported for ${name(character)}: ${r.imported.length} added, ${r.removed.length} removed`, 'Re-apply to update the [Owned] tags in game.');
      }
      open = false;
      ondone();
    } catch (e) {
      failed = toContractError(e).message;
    } finally {
      busy = false;
    }
  }

  const hasReport = $derived(mode === 'rescan' ? !!rescan : !!reimport);
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Fix owned blueprints</Card.Title>
    <Card.Description>Read the game logs again from the start, or import the Smart Citizen export again. You see what changes before anything is written.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    {#if view.notice}
      <Alert.Root variant="warning">
        <TriangleAlert aria-hidden="true" />
        <Alert.Title>Two characters, one name</Alert.Title>
        <Alert.Description>{view.notice}</Alert.Description>
      </Alert.Root>
    {/if}
    <div class="flex flex-wrap gap-2">
      <Button variant="outline" size="sm" onclick={() => start('rescan')}><RefreshCw aria-hidden="true" /> Re-scan game logs</Button>
      <Button variant="outline" size="sm" disabled={!chars.length} onclick={() => start('reimport')}><FileInput aria-hidden="true" /> Re-import from Smart Citizen</Button>
    </div>
    {#if view.last_import}<p class="text-muted-foreground text-[12px]">Last import: <span class="mono">{view.last_import}</span></p>{/if}
  </Card.Content>
</Card.Root>

<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[min(85dvh,40rem)] flex-col overflow-hidden sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>{mode === 'rescan' ? 'Re-scan game logs' : 'Re-import from Smart Citizen'}</Dialog.Title>
      <Dialog.Description>
        {#if mode === 'rescan'}
          Reads Game.log and every log backup of {target.channel} from the start and rebuilds the blueprints found there. Entries from Smart Citizen or StarOrg Tools stay unless you tick Replace.
        {:else}
          {view.last_import ? `Reads ${view.last_import} again (the picker opens if it moved)` : 'Pick the owned-blueprints export file you saved (you pick it again to confirm)'} and adds it to one character. Blueprints from the game logs or StarOrg Tools are never removed.
        {/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if !hasReport}
      <div class="grid gap-3">
        {#if mode === 'reimport'}
          <div class="grid gap-1">
            <span class="text-muted-foreground text-[12px]">Character</span>
            <SelectField ariaLabel="Re-import into character" options={chars.map((c) => ({ id: c.id, label: `${characterLabel(c, chars)} (${c.owned} owned)` }))} value={character} onchange={(v) => (character = v)} class="max-w-72" />
          </div>
        {/if}
        <div class="flex items-start gap-2">
          <Checkbox id="reimport-replace" checked={replace} disabled={busy} onCheckedChange={(v) => (replace = v === true)} class="mt-0.5" />
          <Label for="reimport-replace" class="grid gap-0.5 font-normal">
            <span>Replace</span>
            <span class="text-muted-foreground text-[12px]">
              {mode === 'rescan' ? 'Start every character empty: only what the logs show is kept (imported and StarOrg entries go).' : 'Remove what an earlier Smart Citizen import added and this export no longer has.'}
            </span>
          </Label>
        </div>
      </div>
    {:else if mode === 'rescan' && rescan}
      <section aria-label="Dry-run report" class="-mr-2 grid min-h-0 flex-1 content-start gap-2 overflow-y-auto pr-2 text-[12px]">
        {#if rescan.warnings.length}
          <Alert.Root variant="warning">
            <TriangleAlert aria-hidden="true" />
            <Alert.Title>Not every log was read</Alert.Title>
            <Alert.Description><ul class="list-inside list-disc">{#each rescan.warnings as w (w)}<li class="mono">{w}</li>{/each}</ul></Alert.Description>
          </Alert.Root>
        {/if}
        {#if rescan.replace_refused}<p role="alert" class="text-destructive">{rescan.replace_refused}</p>{/if}
        <p>{rescan.files_read} log files read{rescan.merged ? `; ${rescan.merged} reset character${rescan.merged === 1 ? '' : 's'} merged into the newest one` : ''}.</p>
        {#each rescan.characters as c (c.id ?? '')}
          <div class="rounded-md border p-2">
            <div class="font-medium">{c.id ? c.name : 'Before any login'} <span class="text-muted-foreground mono">{c.before} → {c.after}</span></div>
            {#if c.added.length}<div class="text-muted-foreground">Added: {c.added.join(', ')}</div>{/if}
            {#if c.removed.length}<div class="text-destructive">Removed: {c.removed.join(', ')}</div>{/if}
          </div>
        {/each}
        {#if !rescanChanges.length && !rescan.merged}<p class="text-muted-foreground">Nothing changes.</p>{/if}
      </section>
    {:else if reimport}
      <section aria-label="Dry-run report" class="-mr-2 grid min-h-0 flex-1 content-start gap-2 overflow-y-auto pr-2 text-[12px]">
        <p><span class="mono">{reimport.file}</span> into {name(character)}: {reimport.imported.length} added, {reimport.known.length} already known, {reimport.removed.length} removed, {reimport.unmatched.length} unmatched.</p>
        {#if reimport.imported.length}<div class="text-muted-foreground">Added: {reimport.imported.join(', ')}</div>{/if}
        {#if reimport.removed.length}<div class="text-destructive">Removed: {reimport.removed.join(', ')}</div>{/if}
        {#if reimport.empty_catalog}<p class="text-destructive">Game data has not been read for this channel yet; apply once, then re-import.</p>{/if}
      </section>
    {/if}
    {#if failed}<p role="alert" class="text-destructive">{failed}</p>{/if}
    <Dialog.Footer class="shrink-0">
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      {#if !hasReport}
        <Button disabled={busy || (mode === 'reimport' && !character)} onclick={preview}>Check what changes</Button>
      {:else}
        <Button variant="outline" disabled={busy} onclick={() => ((rescan = null), (reimport = null))}>Back</Button>
        <Button disabled={busy || (mode === 'reimport' && !!reimport?.empty_catalog) || (mode === 'rescan' && !!rescan?.replace_refused)} onclick={confirm}>Confirm</Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
