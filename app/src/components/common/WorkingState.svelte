<script lang="ts">
  /**
   * A call that is running and may take a while (reading Data.p4k the first time): what it is
   * doing, an indeterminate bar, the elapsed time, and after SLOW_AFTER_S why it can take minutes.
   * Never a bare skeleton: a skeleton that never resolves looks like a broken screen.
   */
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { elapsed, fmtElapsed, SLOW_AFTER_S } from '@/lib/loadstate';

  let { title, detail, slow, since, label }: { title: string; detail?: string; slow?: string; since: number; label: string } = $props();

  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });
  const secs = $derived(elapsed(since, now));
</script>

<div class="bg-card/80 grid gap-2 rounded-lg border p-3" role="status" aria-label={label} aria-busy="true" data-testid="working">
  <div class="flex items-center gap-2 font-medium">
    <LoaderCircle class="text-primary size-4 shrink-0 animate-spin" aria-hidden="true" />
    <span class="min-w-0 flex-1">{title}</span>
    <span class="mono text-muted-foreground text-[12px]" aria-hidden="true">{fmtElapsed(secs)}</span>
  </div>
  <div class="bg-muted relative h-1 overflow-hidden rounded-full" aria-hidden="true">
    <div class="working-bar bg-primary absolute inset-y-0 w-1/3 rounded-full"></div>
  </div>
  {#if detail}<p class="text-muted-foreground text-[12.5px]">{detail}</p>{/if}
  {#if slow && secs >= SLOW_AFTER_S}<p class="text-muted-foreground text-[12.5px]">{slow}</p>{/if}
</div>

<style>
  .working-bar {
    animation: working 1.4s ease-in-out infinite;
  }
  @keyframes working {
    from {
      left: -33%;
    }
    to {
      left: 100%;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .working-bar {
      animation: none;
      left: 0;
      width: 100%;
      opacity: 0.45;
    }
  }
</style>
