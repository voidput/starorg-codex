<script lang="ts">
  /**
   * Import Smart Citizen's blueprint export (addendum §4): choose the file and the character, see
   * the dry-run report (new, already known, unmatched, ambiguous), then confirm. Nothing is written
   * before Confirm. The real backend opens the native file picker inside the dry run.
   */
  import FileJson from '@lucide/svelte/icons/file-json';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import * as Dialog from '@/components/ui/dialog';
  import * as Alert from '@/components/ui/alert';
  import { Button } from '@/components/ui/button';
  import SelectField from '@/components/common/SelectField.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, CharacterView, ImportReport } from '@/lib/contract/types';
  import { fmtDateTime, int } from '@/lib/format';
  import { toast } from '@/lib/stores/toast.svelte';
  import { onWindows, smartCitizenDir } from '@/lib/platform';
  import { characterLabel } from '@/lib/characters';

  const exportDir = smartCitizenDir(onWindows);

  let {
    open = $bindable(false),
    target,
    characters,
    initial,
    ondone,
  }: { open: boolean; target: ChannelRef; characters: CharacterView[]; initial: string | null; ondone: (character: string) => void } = $props();

  let character = $state<string | null>(null);
  let report = $state<ImportReport | null>(null);
  let busy = $state(false);
  let failed = $state<string | null>(null);

  $effect(() => {
    if (open) {
      character = initial ?? characters[0]?.id ?? null;
      report = null;
      failed = null;
    }
  });

  const name = (id: string | null) => characters.find((c) => c.id === id)?.name ?? id ?? '—';

  async function check() {
    if (!character) return;
    busy = true;
    failed = null;
    try {
      report = await api.importSmartCitizen({ target, character, dry_run: true });
    } catch (e) {
      failed = toContractError(e).message;
    } finally {
      busy = false;
    }
  }

  async function confirm() {
    if (!character || !report) return;
    busy = true;
    try {
      // The picker opens again: a dry run remembers nothing (pick the same file).
      const r = await api.importSmartCitizen({ target, character, dry_run: false });
      toast('success', `Imported ${r.imported.length} blueprints for ${name(character)}`, `${r.known.length} were already known. Re-apply to update [Owned] tags.`);
      open = false;
      ondone(character);
    } catch (e) {
      failed = toContractError(e).message;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[min(85dvh,44rem)] flex-col overflow-hidden sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>Import from Smart Citizen</Dialog.Title>
      <Dialog.Description>
        Reads Smart Citizen's blueprint tracker export (owned-blueprints.json). Ticked blueprints are added to one character on {target.channel}; nothing is removed.
      </Dialog.Description>
    </Dialog.Header>
    {#if !report}
      <div class="grid gap-3">
        <div class="grid gap-1">
          <span class="text-muted-foreground text-[12px]" id="imp-char">Add to character</span>
          <SelectField ariaLabel="Add to character" options={characters.map((c) => ({ id: c.id, label: `${characterLabel(c, characters)} (${c.owned} owned)` }))} value={character} onchange={(v) => (character = v)} class="max-w-72" />
        </div>
        <p class="text-muted-foreground text-[12px]">Next you pick the file, then again to confirm. Smart Citizen saves it as owned-blueprints.json in <span class="mono">{exportDir}</span>.</p>
      </div>
    {:else}
      {@const r = report}
      <section aria-label="Dry-run report" class="-mr-2 grid min-h-0 flex-1 content-start gap-3 overflow-y-auto pr-2">
        <div class="flex items-start gap-2">
          <FileJson class="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <div class="min-w-0">
            <div class="mono truncate text-[12px]" title={r.file}>{r.file}</div>
            <div class="text-muted-foreground text-[12px]">Exported {fmtDateTime(r.exported_at)} · {int(r.total)} entries · {r.not_completed} only tracked (not imported) · {r.duplicates} duplicates</div>
          </div>
        </div>
        {#if r.empty_catalog}
          <Alert.Root variant="warning">
            <TriangleAlert aria-hidden="true" />
            <Alert.Title>Nothing could be matched yet</Alert.Title>
            <Alert.Description>Game data has not been read for this channel, so no blueprint names are known. Apply once, then import again.</Alert.Description>
          </Alert.Root>
        {/if}
        <dl class="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <div class="rounded-md border px-3 py-2"><dt class="text-muted-foreground text-[12px]">New</dt><dd class="mono text-[16px] font-semibold">{r.imported.length}</dd></div>
          <div class="rounded-md border px-3 py-2"><dt class="text-muted-foreground text-[12px]">Already known</dt><dd class="mono text-[16px] font-semibold">{r.known.length}</dd></div>
          <div class="rounded-md border px-3 py-2"><dt class="text-muted-foreground text-[12px]">Unmatched</dt><dd class="mono text-[16px] font-semibold">{r.unmatched.length}</dd></div>
          <div class="rounded-md border px-3 py-2"><dt class="text-muted-foreground text-[12px]">Ambiguous</dt><dd class="mono text-[16px] font-semibold">{r.ambiguous.length}</dd></div>
        </dl>
        {#if r.imported.length}
          <div>
            <h3 class="mb-1 font-semibold">Will be added to {name(character)}</h3>
            <ul class="grid max-h-48 gap-x-3 gap-y-0.5 overflow-y-auto rounded-md border p-2 text-[12px] sm:grid-cols-2">{#each r.imported as n (n)}<li class="truncate" title={n}>{n}</li>{/each}</ul>
          </div>
        {/if}
        {#if r.unmatched.length || r.ambiguous.length}
          <Alert.Root variant="warning">
            <TriangleAlert aria-hidden="true" />
            <Alert.Title>Not imported</Alert.Title>
            <Alert.Description>
              {#if r.unmatched.length}
                <p>No blueprint with these names in this build (shown as in the file; tag prefixes such as [H] are ignored when matching):</p>
                <ul class="mono max-h-32 list-inside list-disc overflow-y-auto text-[12px]">{#each r.unmatched as n (n)}<li>{n}</li>{/each}</ul>
              {/if}
              {#if r.ambiguous.length}
                <p class="mt-1">These names match more than one blueprint, so none was guessed:</p>
                <ul class="max-h-32 list-inside list-disc overflow-y-auto text-[12px]">{#each r.ambiguous as a (a.name)}<li><span class="font-medium">{a.name}</span>: {a.candidates.join(', ')}</li>{/each}</ul>
              {/if}
            </Alert.Description>
          </Alert.Root>
        {/if}
      </section>
    {/if}
    {#if failed}<p role="alert" class="text-destructive">{failed}</p>{/if}
    <Dialog.Footer class="shrink-0">
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      {#if !report}
        <Button disabled={!character || busy} onclick={check}><FileJson aria-hidden="true" /> Choose file and check</Button>
      {:else}
        <Button variant="outline" disabled={busy} onclick={() => (report = null)}>Back</Button>
        <Button disabled={busy || !report.imported.length} onclick={confirm}>Import {report.imported.length} blueprint{report.imported.length === 1 ? '' : 's'}</Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
