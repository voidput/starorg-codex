<script lang="ts">
  /**
   * Export diagnostics (spec 9.4 Activity / 10.5): doctor + recent log with home paths, user
   * names, character ids and the StarOrg user replaced. Shown in full before it is copied or saved.
   */
  import Copy from '@lucide/svelte/icons/copy';
  import Download from '@lucide/svelte/icons/download';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import { api, toContractError } from '@/lib/contract/client';
  import type { Diagnostics } from '@/lib/contract/types';
  import { toast } from '@/lib/stores/toast.svelte';

  let { open = $bindable(false) }: { open: boolean } = $props();
  let d = $state<Diagnostics | null>(null);

  $effect(() => {
    if (!open) return;
    let stale = false;
    d = null;
    api
      .exportDiagnostics()
      .then((x) => !stale && (d = x))
      .catch((e) => {
        if (stale) return;
        open = false;
        toast('error', 'Could not build the diagnostics', toContractError(e).message);
      });
    return () => (stale = true);
  });

  async function copy() {
    if (!d) return;
    try {
      await navigator.clipboard.writeText(d.text);
      toast('success', 'Diagnostics copied', 'Paste them into your bug report.');
    } catch {
      toast('error', 'Copy failed', 'Select the text and copy it by hand.');
    }
  }
  function save() {
    if (!d) return;
    const url = URL.createObjectURL(new Blob([d.text], { type: 'text/plain' }));
    const a = Object.assign(document.createElement('a'), { href: url, download: d.file_name });
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>Export diagnostics</Dialog.Title>
      <Dialog.Description>Everything below is what would be shared. Nothing is sent anywhere by this.</Dialog.Description>
    </Dialog.Header>
    {#if !d}
      <Skeleton class="h-64 w-full" />
    {:else}
      <p class="text-success flex items-center gap-1.5 text-[12.5px]"><ShieldCheck class="size-4" aria-hidden="true" /> {d.redacted} personal values replaced: home folder, user name, character names and ids, StarOrg user.</p>
      <label for="diag-text" class="sr-only">Diagnostics text</label>
      <textarea id="diag-text" readonly class="mono bg-background h-72 w-full resize-none rounded-md border p-3 text-[11.5px] outline-none focus-visible:ring-2 focus-visible:ring-ring/50">{d.text}</textarea>
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>Close</Button>
      <Button variant="outline" disabled={!d} onclick={copy}><Copy aria-hidden="true" /> Copy</Button>
      <Button disabled={!d} onclick={save}><Download aria-hidden="true" /> Save {d?.file_name ?? ''}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
