<script lang="ts">
  /**
   * Asks before unsaved edits are dropped (Enhancements): leaving the screen or switching channel
   * offers Save / Discard / Stay; Apply offers Save & apply / Apply saved settings / Cancel.
   */
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { app } from '@/lib/stores/app.svelte';

  let saving = $state(false);
  const p = $derived(app.pending);
  const label = $derived(app.draft?.label ?? 'this screen');

  function close() {
    app.pending = null;
  }
  async function save() {
    const pending = app.pending;
    if (!pending || !app.draft) return close();
    saving = true;
    const ok = await app.draft.save();
    saving = false;
    if (!ok) return;
    close();
    pending.proceed();
  }
  function discard() {
    const pending = app.pending;
    app.draft?.discard();
    close();
    pending?.proceed();
  }
  function applySaved() {
    const pending = app.pending;
    close();
    pending?.proceed();
  }
</script>

<Dialog.Root bind:open={() => !!p, (v) => { if (!v && !saving) close(); }}>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>{p?.kind === 'apply' ? 'Apply without your unsaved changes?' : 'Save your changes first?'}</Dialog.Title>
      <Dialog.Description>
        {#if p?.kind === 'apply'}
          {label} has changes that are not saved. Apply writes the saved settings only, unless you save first.
        {:else}
          {label} has changes that are not saved. Save them before you {p?.what ?? 'leave'}?
        {/if}
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      {#if p?.kind === 'apply'}
        <Button variant="ghost" disabled={saving} onclick={close}>Cancel</Button>
        <Button variant="outline" disabled={saving} onclick={applySaved}>Apply saved settings</Button>
        <Button disabled={saving} onclick={save}>{saving ? 'Saving…' : 'Save & apply'}</Button>
      {:else}
        <Button variant="ghost" disabled={saving} onclick={close}>Stay</Button>
        <Button variant="outline" disabled={saving} onclick={discard}>Discard changes</Button>
        <Button disabled={saving} onclick={save}>{saving ? 'Saving…' : 'Save'}</Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
