<script lang="ts">
  /**
   * Before an apply replaces another tool's global.ini (Smart Citizen, ...): what happens to that
   * file, why both tools must not run, and an explicit "Back up and replace" tick.
   */
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { app } from '@/lib/stores/app.svelte';
  import { ACK_LABEL, foreignBeforeApply } from '@/lib/foreign';

  let ack = $state(false);
  const f = $derived(app.replaceAsk);
  const t = $derived(f ? foreignBeforeApply(f) : null);

  function close() {
    app.replaceAsk = null;
    ack = false;
  }
  function proceed() {
    close();
    void app.apply();
  }
</script>

<Dialog.Root bind:open={() => !!f, (v) => { if (!v) close(); }}>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title class="flex items-center gap-2"><ShieldAlert class="text-warning size-5" aria-hidden="true" /> {t?.title}</Dialog.Title>
      <Dialog.Description>{t?.body}</Dialog.Description>
    </Dialog.Header>
    <p class="text-muted-foreground">{t?.stop}</p>
    <div class="flex items-center gap-2">
      <Checkbox id="replace-ack-home" checked={ack} onCheckedChange={(v) => (ack = v === true)} />
      <label for="replace-ack-home" class="cursor-pointer font-medium">{ACK_LABEL}</label>
    </div>
    <Dialog.Footer>
      <Button variant="ghost" onclick={close}>Cancel</Button>
      <Button disabled={!ack} onclick={proceed}>Apply to {f?.channel}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
