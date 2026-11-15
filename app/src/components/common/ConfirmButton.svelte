<script lang="ts">
  /**
   * A cheap destructive action in two clicks (delete one backup, reset one edit). The first click
   * arms it; it stays armed until clicked again, Escape, or focus leaves (no timer: WCAG 2.2.1).
   * The armed state is announced through a live region next to the button. Anything that writes
   * to the game uses ActionDialog instead.
   */
  import type { Snippet } from 'svelte';
  import { Button, type ButtonProps } from '@/components/ui/button';

  let {
    onconfirm,
    confirmLabel = 'Sure?',
    variant = 'destructive',
    size = 'xs',
    children,
    ...rest
  }: ButtonProps & { onconfirm: () => void; confirmLabel?: string; children: Snippet } = $props();
  let armed = $state(false);
  let wrap = $state<HTMLElement | null>(null);

  function click() {
    if (armed) {
      armed = false;
      onconfirm();
      return;
    }
    armed = true;
  }
  function key(e: KeyboardEvent) {
    if (armed && e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      armed = false;
    }
  }
  function out(e: FocusEvent) {
    if (!wrap?.contains(e.relatedTarget as Node | null)) armed = false;
  }
</script>

<span bind:this={wrap} class="inline-flex" onfocusout={out}>
  <Button {...rest} variant={armed ? 'destructive' : variant} {size} onclick={click} onkeydown={key}>{#if armed}{confirmLabel}{:else}{@render children()}{/if}</Button>
  <span class="sr-only" role="status">{armed ? `${confirmLabel} Press again to confirm, Escape to cancel.` : ''}</span>
</span>
