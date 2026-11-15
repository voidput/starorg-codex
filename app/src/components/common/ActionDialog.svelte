<script lang="ts">
  /**
   * A confirmation dialog for one action that writes to the game: what happens, an optional
   * acknowledgement the user must tick (build mismatch), and the error inline if it fails.
   */
  import type { Snippet } from 'svelte';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Label } from '@/components/ui/label';
  import { toContractError } from '@/lib/contract/client';

  let {
    open = $bindable(false),
    title,
    confirmLabel,
    destructive = false,
    ack = null,
    onconfirm,
    children,
  }: {
    open: boolean;
    title: string;
    confirmLabel: string;
    destructive?: boolean;
    /** When set, a checkbox with this text must be ticked first. */
    ack?: string | null;
    /** Throw to keep the dialog open and show the error. */
    onconfirm: () => Promise<void>;
    children: Snippet;
  } = $props();

  let acked = $state(false);
  let busy = $state(false);
  let error = $state<{ message: string; detail: string | null } | null>(null);
  $effect(() => {
    if (open) {
      acked = false;
      error = null;
    }
  });

  async function go() {
    busy = true;
    error = null;
    try {
      await onconfirm();
      open = false;
    } catch (e) {
      const c = toContractError(e);
      error = { message: c.message, detail: c.detail };
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
    </Dialog.Header>
    <div class="grid gap-3">{@render children()}</div>
    {#if ack}
      <div class="flex items-start gap-2 rounded-md border p-3">
        <Checkbox id="ack" checked={acked} onCheckedChange={(v) => (acked = v === true)} class="mt-0.5" />
        <Label for="ack" class="leading-snug font-normal">{ack}</Label>
      </div>
    {/if}
    {#if error}
      <div role="alert" class="text-destructive grid gap-0.5">
        <p>{error.message}</p>
        {#if error.detail}<p class="text-[12px]">{error.detail}</p>{/if}
      </div>
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      <Button variant={destructive ? 'destructive' : 'default'} disabled={busy || (!!ack && !acked)} onclick={go}>{confirmLabel}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
