<script lang="ts">
  /**
   * Backups (spec 7.6, 9.4): three separate actions (Remove StarOrg changes, Undo last apply,
   * Restore the file from before StarOrg Strings, shown only when one exists, with its build and
   * a mismatch warning), Back up now, and the timeline of this channel's backups.
   */
  import Eraser from '@lucide/svelte/icons/eraser';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import History from '@lucide/svelte/icons/history';
  import HardDriveDownload from '@lucide/svelte/icons/hard-drive-download';
  import Pin from '@lucide/svelte/icons/pin';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import * as Alert from '@/components/ui/alert';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import ActionDialog from '@/components/common/ActionDialog.svelte';
  import RemoveDialog from '@/components/common/RemoveDialog.svelte';
  import PathText from '@/components/common/PathText.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { BackupList, BackupView, Build } from '@/lib/contract/types';
  import { BACKUP_REASON, buildLabel, fmtBytes, fmtDateTime, sameBuild } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  let list = $state<BackupList | null>(null);
  let error = $state<ContractError | null>(null);
  let dialog = $state<'remove' | 'undo' | 'foreign' | 'restore' | null>(null);
  let picked = $state<BackupView | null>(null);
  let tick = $state(0);
  const key = $derived(app.target ? `${app.target.install}/${app.target.channel}/${app.rev}/${tick}` : '');

  $effect(() => {
    if (!key || !app.target) return;
    const t = $state.snapshot(app.target);
    let stale = false;
    api
      .listBackups(t)
      .then((l) => !stale && ((list = l), (error = null)))
      .catch((e) => !stale && (error = toContractError(e)));
    return () => (stale = true);
  });

  const ch = $derived(app.channel);
  const mismatch = (b: Build) => !!ch && !sameBuild(b, ch.build);
  const originLabel = (b: BackupView) => (b.origin === 'ours' ? 'Our file' : b.origin === 'foreign' ? `${b.foreign_tool ?? 'Another tool'}'s file` : 'No file (stock text)');

  const open = (d: typeof dialog, b: BackupView | null = null) => {
    picked = b;
    dialog = d;
  };
  const isOpen = (d: typeof dialog) => () => dialog === d;
  const setOpen = (d: typeof dialog) => (v: boolean) => {
    dialog = v ? d : null;
  };

  const running = $derived(ch?.game_running ? `Game running: close it to change the file on ${ch.channel}.` : null);
  const undoBackup = $derived(list?.offers.undo_last_apply ? (list.backups.find((b) => b.id === list!.offers.undo_last_apply) ?? null) : null);

  async function doUndo() {
    const target = app.target!;
    await app.write('undo', () => api.undoLastApply(target), { inline: true });
    tick++;
  }
  async function doRestore(id: string, allow: boolean) {
    const target = app.target!;
    await app.write('restore', () => api.restoreBackup({ target, backup: id, allow_build_mismatch: allow }), { inline: true });
    tick++;
  }
  async function backupNow() {
    if (!app.target) return;
    try {
      const b = await api.backupNow(app.target);
      toast('success', 'Backed up', `${originLabel(b)} saved as ${b.id}.`);
      tick++;
    } catch (e) {
      toast('error', 'Backup failed', toContractError(e).message);
    }
  }
  async function del(b: BackupView) {
    if (!app.target) return;
    try {
      list = await api.deleteBackup({ target: app.target, backup: b.id });
      toast('success', 'Backup deleted', b.id);
    } catch (e) {
      toast('error', 'Not deleted', toContractError(e).message);
    }
  }
</script>

{#if error}
  <ErrorState title="Could not read the backups" {error} onretry={() => tick++} />
{:else if !list || !ch}
  <Skeleton class="h-64 w-full" />
{:else}
  {@const o = list.offers}
  <div class="grid gap-4">
    {#if running}
      <Alert.Root variant="warning"><TriangleAlert aria-hidden="true" /><Alert.Description>{running}</Alert.Description></Alert.Root>
    {/if}
    <section aria-label="Restore actions" class="grid items-start gap-3 lg:grid-cols-3">
      <div class="bg-card flex flex-col gap-3 self-start rounded-lg border p-4">
        <div>
          <h2 class="font-semibold">Remove StarOrg changes</h2>
          <p class="text-muted-foreground mt-1 text-[12.5px]">Back to the game's stock text. {o.remove_foreign ? `The file in place is from ${ch.foreign_tool ?? 'another tool'}; it is backed up, then removed.` : ''}</p>
        </div>
        <div><Button variant="outline" size="sm" disabled={!o.remove || !!running} onclick={() => open('remove')}><Eraser aria-hidden="true" /> Remove changes</Button></div>
        {#if !o.remove}<p class="text-muted-foreground text-[12px]">Nothing to remove: the game shows stock text.</p>{/if}
      </div>
      <div class="bg-card flex flex-col gap-3 self-start rounded-lg border p-4">
        <div>
          <h2 class="font-semibold">Undo last apply</h2>
          <p class="text-muted-foreground mt-1 text-[12.5px]">
            {#if undoBackup}Goes back to the file from {fmtDateTime(undoBackup.created)}, before the last apply.{:else}Put back the file from before the last apply, for this build.{/if}
          </p>
        </div>
        <div><Button variant="outline" size="sm" disabled={!o.undo_last_apply || !!running} onclick={() => open('undo')}><Undo2 aria-hidden="true" /> Undo last apply</Button></div>
        {#if !o.undo_last_apply}<p class="text-muted-foreground text-[12px]">No earlier apply of this build to go back to.</p>{/if}
      </div>
      {#if o.restore_foreign}
        {@const f = o.restore_foreign}
        <div class="bg-card flex flex-col gap-3 self-start rounded-lg border p-4">
          <div>
            <h2 class="font-semibold">Restore the file from before StarOrg Strings</h2>
            <p class="text-muted-foreground mt-1 text-[12.5px]">{f.tool ?? 'Another tool'}'s file, made for <span class="mono">{buildLabel(f.made_for)}</span>.</p>
            {#if f.build_mismatch}<p class="text-warning mt-1 flex items-center gap-1 text-[12px]"><TriangleAlert class="size-3.5" aria-hidden="true" /> Made for another game build</p>{/if}
          </div>
          <div><Button variant="outline" size="sm" disabled={!!running} onclick={() => open('foreign')}><History aria-hidden="true" /> Restore it</Button></div>
        </div>
      {/if}
    </section>

    <section aria-labelledby="tl-title" class="grid gap-2">
      <div class="flex flex-wrap items-center gap-2">
        <h2 id="tl-title" class="eyebrow text-muted-foreground">Timeline · {list.channel}</h2>
        <span class="flex-1"></span>
        <span class="text-muted-foreground hidden max-w-[50%] text-[11.5px] lg:inline"><PathText path={list.folder} /></span>
        <Button variant="outline" size="sm" onclick={backupNow}><HardDriveDownload aria-hidden="true" /> Back up now</Button>
      </div>
      {#if !list.backups.length}
        <p class="text-muted-foreground bg-card rounded-lg border py-8 text-center">No backups yet. The first apply keeps the file that was there before.</p>
      {:else}
        <ol class="bg-card divide-y rounded-lg border">
          {#each list.backups as b (b.id)}
            <li class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-4 py-2.5">
              <div class="min-w-0">
                <div class="flex flex-wrap items-center gap-2">
                  <span class="font-medium">{BACKUP_REASON[b.reason]}</span>
                  {#if b.pinned}<ToneBadge tone="info" icon={Pin} label="Kept" hint="The file from before StarOrg Strings is never deleted by retention" />{/if}
                  {#if mismatch(b.build)}<ToneBadge tone="warning" label="Other build" hint="Made for {buildLabel(b.build)}" />{/if}
                </div>
                <div class="text-muted-foreground text-[12px]">
                  {fmtDateTime(b.created)} · {originLabel(b)} · <span class="mono">{buildLabel(b.build)}</span> · {fmtBytes(b.size)}
                </div>
              </div>
              <div class="flex items-center gap-1.5">
                <Button variant="ghost" size="sm" disabled={!!running} aria-label={`Restore backup from ${fmtDateTime(b.created)}`} onclick={() => open('restore', b)}>Restore</Button>
                {#if b.pinned}
                  <span class="text-muted-foreground px-2 text-[12px]">Always kept</span>
                {:else}
                  <ConfirmButton size="sm" variant="ghost" confirmLabel="Delete? Click again" aria-label={`Delete backup from ${fmtDateTime(b.created)}`} onconfirm={() => del(b)}>Delete</ConfirmButton>
                {/if}
              </div>
            </li>
          {/each}
        </ol>
        <p class="text-muted-foreground text-[12px]">
          Kept automatically: the newest {list.keep_newest} backups of this channel, the newest backup of each of the last {list.keep_builds} game builds, and the file from before StarOrg Strings. Older ones are deleted after an apply.
        </p>
      {/if}
    </section>
  </div>

  <RemoveDialog bind:open={isOpen('remove'), setOpen('remove')} channel={ch} ondone={() => tick++} />

  <ActionDialog
    bind:open={isOpen('undo'), setOpen('undo')}
    title={`Undo the last apply on ${ch.channel}?`}
    description={undoBackup ? `Puts back the file from ${fmtDateTime(undoBackup.created)}, before the last apply.` : 'Puts back the file from before the last apply.'}
    confirmLabel="Undo last apply"
    busyLabel="Restoring…"
    disabledReason={running}
    onconfirm={doUndo}
  >
    <p>Backup <span class="mono">{o.undo_last_apply}</span>. Settings that changed since stay changed; the channel shows “Re-apply needed”.</p>
  </ActionDialog>

  {#if o.restore_foreign}
    {@const f = o.restore_foreign}
    <ActionDialog
      bind:open={isOpen('foreign'), setOpen('foreign')}
      title="Restore the file from before StarOrg Strings?"
      description={`Puts ${f.tool ?? 'the other tool'}'s file back in place of ours.`}
      confirmLabel={`Restore ${f.tool ?? 'the old'} file`}
      busyLabel="Restoring…"
      destructive
      disabledReason={running}
      ack={f.build_mismatch ? 'I understand that text added since that build will show as raw @keys until the other tool updates it.' : null}
      onconfirm={() => doRestore(f.backup, f.build_mismatch)}
    >
      <p>Our text is backed up first. Run only one text tool per channel.</p>
      <dl class="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
        <dt class="text-muted-foreground">Made for</dt><dd class="mono">{buildLabel(f.made_for)}</dd>
        <dt class="text-muted-foreground">Game is now</dt><dd class="mono">{buildLabel(f.current)}</dd>
      </dl>
      {#if f.build_mismatch}
        <Alert.Root variant="warning"><TriangleAlert aria-hidden="true" /><Alert.Title>Made for another game build</Alert.Title><Alert.Description>Keys the game added since then are missing from this file and show as raw @keys in game.</Alert.Description></Alert.Root>
      {/if}
    </ActionDialog>
  {/if}

  {#if picked}
    {@const b = picked}
    <ActionDialog
      bind:open={isOpen('restore'), setOpen('restore')}
      title="Restore this backup?"
      description={`${BACKUP_REASON[b.reason]}, ${fmtDateTime(b.created)}: ${originLabel(b)}.`}
      confirmLabel="Restore backup"
      busyLabel="Restoring…"
      destructive={b.origin === 'foreign'}
      disabledReason={running}
      ack={mismatch(b.build) ? 'I understand that this file was made for another build and new text may show as raw @keys.' : b.origin === 'foreign' ? `Put ${b.foreign_tool ?? 'the other tool'}'s file back instead of ours.` : null}
      onconfirm={() => doRestore(b.id, mismatch(b.build))}
    >
      <dl class="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
        <dt class="text-muted-foreground">Made for</dt><dd class="mono">{buildLabel(b.build)}</dd>
        <dt class="text-muted-foreground">Game is now</dt><dd class="mono">{buildLabel(ch.build)}</dd>
      </dl>
      {#if mismatch(b.build)}
        <Alert.Root variant="warning"><TriangleAlert aria-hidden="true" /><Alert.Description>Made for another game build: keys added since then show as raw @keys.</Alert.Description></Alert.Root>
      {/if}
      <p class="text-muted-foreground text-[12px]">The file in place now is backed up first.</p>
    </ActionDialog>
  {/if}
{/if}
