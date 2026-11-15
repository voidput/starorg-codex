<script lang="ts">
  /**
   * Patches (spec 9.4): fixes for known text bugs with their state for this channel's build
   * (applied / not needed / mismatch ...), an enabled switch, and a detail sheet with before/after.
   * Your own patches: Add (a one-entry text patch), Import (.patch.toml), Export and Delete.
   */
  import Plus from '@lucide/svelte/icons/plus';
  import FileUp from '@lucide/svelte/icons/file-up';
  import FileDown from '@lucide/svelte/icons/file-down';
  import Copy from '@lucide/svelte/icons/copy';
  import * as Dialog from '@/components/ui/dialog';
  import { Input } from '@/components/ui/input';
  import { Label } from '@/components/ui/label';
  import SelectField from '@/components/common/SelectField.svelte';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import type { LocOp, PatchExport } from '@/lib/contract/types';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { Switch } from '@/components/ui/switch';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import * as Alert from '@/components/ui/alert';
  import Info from '@lucide/svelte/icons/info';
  import Sheet from '@/components/common/Sheet.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import StringDiff from '@/components/common/StringDiff.svelte';
  import InGamePreview from '@/components/common/InGamePreview.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { PatchDetail, PatchView } from '@/lib/contract/types';
  import { int, patchStateView } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  let patches = $state<PatchView[] | null>(null);
  let error = $state<ContractError | null>(null);
  let detail = $state<PatchDetail | null>(null);
  let sheet = $state(false);
  const key = $derived(app.target ? `${app.target.install}/${app.target.channel}/${app.rev}` : '');

  $effect(() => {
    if (!key || !app.target) return;
    const t = $state.snapshot(app.target);
    let stale = false;
    api
      .listPatches(t)
      .then((p) => !stale && ((patches = p), (error = null)))
      .catch((e) => !stale && (error = toContractError(e)));
    return () => (stale = true);
  });

  async function toggle(p: PatchView, enabled: boolean) {
    if (!app.target || !patches) return;
    try {
      const np = await api.setPatchEnabled({ target: app.target, id: p.id, enabled });
      patches = patches.map((x) => (x.id === np.id ? np : x));
      toast('success', `${np.title}: ${enabled ? 'on' : 'off'}`, 'Takes effect on the next apply.');
    } catch (e) {
      toast('error', 'Could not change the patch', toContractError(e).message);
    }
  }

  async function open(p: PatchView) {
    if (!app.target) return;
    detail = null;
    sheet = true;
    try {
      detail = await api.patchDetail({ target: app.target, id: p.id });
    } catch (e) {
      sheet = false;
      toast('error', `Could not open ${p.title}`, toContractError(e).message);
    }
  }

  // Your own patches.
  let adding = $state(false);
  let form = $state({ title: '', description: '', key: '', op: 'set' as LocOp, find: '', value: '' });
  let formError = $state<string | null>(null);
  let exported = $state<PatchExport | null>(null);
  const ops: { id: LocOp; label: string }[] = [
    { id: 'set', label: 'Replace the whole text' },
    { id: 'replace', label: 'Replace part of the text' },
    { id: 'append', label: 'Add text at the end' },
    { id: 'prepend', label: 'Add text at the start' },
  ];

  async function addPatch() {
    if (!app.target) return;
    formError = null;
    try {
      const v = await api.addUserPatch({ target: app.target, title: form.title, description: form.description, key: form.key.trim(), op: form.op, find: form.op === 'replace' ? form.find : null, value: form.value });
      patches = [...(patches ?? []), v];
      adding = false;
      form = { title: '', description: '', key: '', op: 'set', find: '', value: '' };
      toast('success', `Added ${v.title}`, 'Written on the next apply.');
    } catch (e) {
      formError = toContractError(e).message;
    }
  }
  async function importPatch() {
    if (!app.target) return;
    try {
      const v = await api.importUserPatch(app.target);
      patches = [...(patches ?? []), v];
      toast('success', `Imported ${v.title}`, 'Written on the next apply.');
    } catch (e) {
      toast('error', 'Not imported', toContractError(e).message);
    }
  }
  async function exportPatch(p: PatchView) {
    if (!app.target) return;
    try {
      exported = await api.exportUserPatch({ target: app.target, id: p.id });
    } catch (e) {
      toast('error', 'Not exported', toContractError(e).message);
    }
  }
  async function deletePatch(p: PatchView) {
    if (!app.target) return;
    try {
      patches = await api.deleteUserPatch({ target: app.target, id: p.id });
      toast('success', `Deleted ${p.title}`, 'Export it first if you want to keep a copy; Import brings a file back.');
    } catch (e) {
      toast('error', 'Not deleted', toContractError(e).message);
    }
  }
  async function copyText(t: string) {
    try {
      await navigator.clipboard.writeText(t);
      toast('success', 'Copied');
    } catch {
      toast('error', 'Could not copy', 'Select the text and copy it by hand.');
    }
  }

  const counts = $derived.by(() => {
    const c = { applies: 0, warn: 0, other: 0 };
    for (const p of patches ?? []) {
      const v = patchStateView(p.state, p.enabled);
      if (v.tone === 'success') c.applies++;
      else if (v.tone === 'warning' || v.tone === 'destructive') c.warn++;
      else c.other++;
    }
    return c;
  });
</script>

{#if error}
  <ErrorState title="Could not read the patches" {error} onretry={() => app.refresh()} />
{:else if !patches}
  <Skeleton class="h-48 w-full" />
{:else}
  <div class="grid gap-3">
    {#if !patches.length}
      <p class="text-muted-foreground">No patches for this build.</p>
    {/if}
    <p class="text-muted-foreground" aria-live="polite">
      {counts.applies} applied · {counts.warn} skipped with a warning · {counts.other} not needed or off, for {app.channel?.channel} build {app.channel?.build.change ?? '—'}.
    </p>
    <div class="flex flex-wrap items-center gap-2" role="toolbar" aria-label="Your patches">
      <Button variant="outline" size="sm" onclick={() => { formError = null; adding = true; }}><Plus aria-hidden="true" /> Add patch</Button>
      <Button variant="outline" size="sm" onclick={importPatch}><FileUp aria-hidden="true" /> Import patch file</Button>
    </div>
    {#if counts.warn}
      <Alert.Root variant="warning">
        <Info aria-hidden="true" />
        <Alert.Description>A patch in “Mismatch” is skipped: the game text changed, so applying it could break the new text. It is reported on every apply until updated.</Alert.Description>
      </Alert.Root>
    {/if}
    <div class="bg-card overflow-x-auto rounded-lg border">
      <table class="w-full text-left">
        <caption class="sr-only">Patches for {app.channel?.channel}</caption>
        <thead class="text-muted-foreground border-b text-[12px]">
          <tr>
            <th scope="col" class="px-3 py-2 font-medium">Patch</th>
            <th scope="col" class="px-3 py-2 font-medium">Keys</th>
            <th scope="col" class="px-3 py-2 font-medium">Origin</th>
            <th scope="col" class="px-3 py-2 font-medium">State</th>
            <th scope="col" class="px-3 py-2 font-medium">On</th>
            <th scope="col" class="px-3 py-2"><span class="sr-only">Details</span></th>
          </tr>
        </thead>
        <tbody class="divide-y">
          {#each patches as p (p.id)}
            {@const v = patchStateView(p.state, p.enabled)}
            <tr class="hover:bg-accent/40">
              <td class="px-3 py-2">
                <div class="font-medium">{p.title}</div>
                <div class="text-muted-foreground text-[12px]">{p.description}</div>
                <div class="mono text-muted-foreground text-[11px]">{p.id}</div>
              </td>
              <td class="mono px-3 py-2">{int(p.keys)}</td>
              <td class="px-3 py-2">{p.origin === 'built-in' ? 'Built-in' : 'Yours'}</td>
              <td class="px-3 py-2"><ToneBadge tone={v.tone} label={v.label} hint={v.hint} /></td>
              <td class="px-3 py-2">
                <div class="flex items-center gap-2">
                  <Switch checked={p.enabled} aria-label={`${p.title} on`} aria-describedby={p.enabled && v.skipped ? `skip-${p.id}` : undefined} onCheckedChange={(on) => toggle(p, on)} />
                  {#if p.enabled && v.skipped}<span id="skip-{p.id}" class="text-muted-foreground text-[12px] whitespace-nowrap">On, skipped</span>{/if}
                </div>
              </td>
              <td class="px-3 py-2 text-right">
                <div class="flex items-center justify-end gap-1">
                  {#if p.origin === 'user'}
                    <Button variant="ghost" size="sm" aria-label={`Export ${p.title}`} onclick={() => exportPatch(p)}><FileDown aria-hidden="true" /> Export</Button>
                    <ConfirmButton variant="ghost" size="sm" confirmLabel="Delete? Press again" aria-label={`Delete ${p.title}`} onconfirm={() => deletePatch(p)}>Delete</ConfirmButton>
                  {/if}
                  <Button variant="ghost" size="sm" aria-label={`Details of ${p.title}`} onclick={() => open(p)}>Details <ChevronRight aria-hidden="true" /></Button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="text-muted-foreground text-[12px]">Your patches are kept per channel in the app's data folder; the command line reads the same files.</p>
  </div>
{/if}

<Sheet bind:open={sheet} title={detail?.patch.title ?? 'Patch'} description={detail ? `${detail.patch.id} · ${detail.kind === 'data' ? 'Game data patch' : 'Text patch'}${detail.range ? ` · builds ${detail.range}` : ''}` : null}>
  {#if !detail}
    <Skeleton class="h-40 w-full" />
  {:else}
    {@const v = patchStateView(detail.patch.state, detail.patch.enabled)}
    {@const skip = !!v.skipped || !detail.patch.enabled}
    <div class="grid gap-4">
      <div class="flex flex-wrap items-center gap-2"><ToneBadge tone={v.tone} label={v.label} /><span class="text-muted-foreground">{detail.reason}</span></div>
      <p>{detail.patch.description}</p>
      {#each detail.examples as ex (ex.key)}
        <section class="grid gap-1.5 rounded-lg border p-3" aria-label={ex.key}>
          <div class="mono text-[12px]">{ex.key}</div>
          <div class="text-muted-foreground text-[11px] font-medium uppercase">{skip ? 'Stock text now' : 'Stock'}</div>
          <InGamePreview text={ex.before} />
          <div class={skip ? 'text-muted-foreground text-[11px] font-medium uppercase' : 'text-primary text-[11px] font-medium uppercase'}>{skip ? 'What the patch would write (not written on this build)' : 'With the patch'}</div>
          <InGamePreview text={ex.after} class={skip ? 'opacity-70' : ''} />
          <div class="text-muted-foreground text-[11px] font-medium uppercase">{skip ? 'Difference (not applied)' : 'What changes'}</div>
          <StringDiff before={ex.before} after={ex.after} />
        </section>
      {/each}
    </div>
  {/if}
</Sheet>

<Dialog.Root bind:open={adding}>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Add a patch for {app.channel?.channel}</Dialog.Title>
      <Dialog.Description>A text patch changes one key's text on every apply, and is checked against the stock text of each new build.</Dialog.Description>
    </Dialog.Header>
    <form class="grid gap-3" onsubmit={(e) => { e.preventDefault(); void addPatch(); }}>
      <div class="grid gap-1"><Label for="np-title">Title</Label><Input id="np-title" bind:value={form.title} required placeholder="What it fixes" /></div>
      <div class="grid gap-1"><Label for="np-desc">Description (optional)</Label><Input id="np-desc" bind:value={form.description} /></div>
      <div class="grid gap-1"><Label for="np-key">Key</Label><Input id="np-key" class="mono" bind:value={form.key} required placeholder="item_Name..." /></div>
      <div class="grid gap-1"><span class="text-[13px] font-medium" id="np-op">Change</span><SelectField ariaLabel="Change" options={ops} value={form.op} onchange={(v) => (form.op = v)} class="w-64" /></div>
      {#if form.op === 'replace'}
        <div class="grid gap-1"><Label for="np-find">Text to find (must appear once)</Label><Input id="np-find" class="mono" bind:value={form.find} required /></div>
      {/if}
      <div class="grid gap-1"><Label for="np-value">{form.op === 'set' ? 'New text' : form.op === 'replace' ? 'Replace with' : 'Text to add'}</Label><Input id="np-value" class="mono" bind:value={form.value} /></div>
      {#if formError}<p role="alert" class="text-destructive text-[12.5px]">{formError}</p>{/if}
      <Dialog.Footer>
        <Button variant="ghost" type="button" onclick={() => (adding = false)}>Cancel</Button>
        <Button type="submit">Add patch</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={() => !!exported, (v) => { if (!v) exported = null; }}>
  <Dialog.Content class="sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>Export {exported?.file_name}</Dialog.Title>
      <Dialog.Description>The app also offers to save this file. Import brings it back, here or on another computer.</Dialog.Description>
    </Dialog.Header>
    <pre class="bg-background mono max-h-72 overflow-auto rounded-md border p-2 text-[12px]">{exported?.text}</pre>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => exported && copyText(exported.text)}><Copy aria-hidden="true" /> Copy</Button>
      <Button onclick={() => (exported = null)}>Done</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
