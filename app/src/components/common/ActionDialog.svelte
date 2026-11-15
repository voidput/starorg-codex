<script lang="ts">
  /**
   * A confirmation dialog for one action that writes to the game: what happens (description),
   * an optional acknowledgement the user must tick (build mismatch, another tool's file), and the
   * error inline if it fails. Rule for the confirm button: writing our text or a backup to the
   * game is `primary`; deleting or replacing another tool's file is `destructive`.
   * While the write runs the confirm button shows a spinner and its busy label, and Cancel waits.
   */
  import type { Snippet } from 'svelte';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Label } from '@/components/ui/label';
  import { toContractError } from '@/lib/contract/client';

  let {
    open = $bindable(false),
    title,
    description,
    confirmLabel,
    busyLabel = 'Working…',
    destructive = false,
    ack = null,
    disabledReason = null,
    onconfirm,
    children,
  }: {
    open: boolean;
    title: string;
    /** One sentence: what the action does. Read out as the dialog's description. */
    description: string;
    confirmLabel: string;
    busyLabel?: string;
    destructive?: boolean;
    /** When set, a checkbox with this text must be ticked first. */
    ack?: string | null;
    /** When set, the action cannot run now (game running); shown instead of the button. */
    disabledReason?: string | null;
    /** Throw to keep the dialog open and show the error. */
    onconfirm: () => Promise<void>;
    children?: Snippet;
  } = $props();

  const uid = $props.id();
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

<Dialog.Root bind:open={() => open, (v) => { if (!busy) open = v; }}>
  <Dialog.Content class="sm:max-w-lg" showCloseButton={!busy}>
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      <Dialog.Description>{description}</Dialog.Description>
    </Dialog.Header>
    {#if children}<div class="grid gap-3">{@render children()}</div>{/if}
    {#if ack}
      <div class="flex items-start gap-2 rounded-md border p-3">
        <Checkbox id="{uid}-ack" checked={acked} disabled={busy} onCheckedChange={(v) => (acked = v === true)} class="mt-0.5" />
        <Label for="{uid}-ack" class="leading-snug font-normal">{ack}</Label>
      </div>
    {/if}
    {#if error}
      <div role="alert" class="text-destructive grid gap-0.5">
        <p class="font-medium">{error.message}</p>
        {#if error.detail}<p class="text-[12px]">{error.detail}</p>{/if}
      </div>
    {/if}
    {#if disabledReason}
      <p class="text-warning text-[12.5px]" role="status">{disabledReason}</p>
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" disabled={busy} onclick={() => (open = false)}>Cancel</Button>
      <Button variant={destructive ? 'destructive' : 'default'} disabled={busy || !!disabledReason || (!!ack && !acked)} aria-busy={busy} onclick={go}>
        {#if busy}<LoaderCircle class="animate-spin" aria-hidden="true" />{busyLabel}{:else}{confirmLabel}{/if}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
