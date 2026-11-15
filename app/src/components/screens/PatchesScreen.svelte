<script lang="ts">
  /**
   * Patches (spec 9.4): fixes for known text bugs with their state for this channel's build
   * (applied / not needed / mismatch ...), an enabled switch, and a detail sheet with before/after.
   */
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
      {counts.applies} applied · {counts.warn} need a look · {counts.other} not needed or off, for {app.channel?.channel} build {app.channel?.build.change ?? '—'}.
    </p>
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
              <td class="px-3 py-2"><Switch checked={p.enabled} aria-label={`${p.title} on`} onCheckedChange={(on) => toggle(p, on)} /></td>
              <td class="px-3 py-2 text-right">
                <Button variant="ghost" size="sm" aria-label={`Details of ${p.title}`} onclick={() => open(p)}>Details <ChevronRight aria-hidden="true" /></Button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="text-muted-foreground text-[12px]">Adding, importing and exporting your own patches comes with the patch file format (spec 4.5); the command line already reads them from the patches folder.</p>
  </div>
{/if}

<Sheet bind:open={sheet} title={detail?.patch.title ?? 'Patch'} description={detail ? `${detail.patch.id} · ${detail.kind === 'data' ? 'Game data patch' : 'Text patch'}${detail.range ? ` · builds ${detail.range}` : ''}` : null}>
  {#if !detail}
    <Skeleton class="h-40 w-full" />
  {:else}
    {@const v = patchStateView(detail.patch.state, detail.patch.enabled)}
    <div class="grid gap-4">
      <div class="flex flex-wrap items-center gap-2"><ToneBadge tone={v.tone} label={v.label} /><span class="text-muted-foreground">{detail.reason}</span></div>
      <p>{detail.patch.description}</p>
      {#each detail.examples as ex (ex.key)}
        <section class="grid gap-1.5 rounded-lg border p-3" aria-label={ex.key}>
          <div class="mono text-[12px]">{ex.key}</div>
          <div class="text-muted-foreground text-[11px] font-medium uppercase">Stock</div>
          <InGamePreview text={ex.before} />
          <div class="text-primary text-[11px] font-medium uppercase">With the patch</div>
          <InGamePreview text={ex.after} />
          <StringDiff before={ex.before} after={ex.after} />
        </section>
      {/each}
    </div>
  {/if}
</Sheet>
