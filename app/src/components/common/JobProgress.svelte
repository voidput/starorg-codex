<script lang="ts">
  /**
   * Progress of the running job (spec 9.4 wizard step 6, header while busy): label, step n of m,
   * a bar, and Stop when the core can still stop it (before it writes). One polite live region
   * stays in the footer; this one is not live, so steps are not read out twice.
   */
  import Square from '@lucide/svelte/icons/square';
  import { Button } from '@/components/ui/button';
  import { Progress } from '@/components/ui/progress';
  import type { JobProgress } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  let { job, compact = false, class: className = '' }: { job: JobProgress | null; compact?: boolean; class?: string } = $props();
  let stopping = $state(false);
  $effect(() => {
    if (!job) stopping = false;
  });
  async function stop() {
    stopping = true;
    await app.cancel();
  }
</script>

{#if job}
  <div class={cn('flex min-w-0 items-center gap-2', compact ? 'w-60' : 'w-full', className)} data-job={job.job}>
    <div class="grid min-w-0 flex-1 gap-1">
      <div class="flex items-baseline justify-between gap-2 text-[12px]">
        <span class="truncate">{job.label}</span>
        <span class="text-muted-foreground mono shrink-0">{job.step} of {job.steps}</span>
      </div>
      <Progress value={job.step} max={job.steps} class="h-1" aria-label={`${job.label}, step ${job.step} of ${job.steps}`} />
    </div>
    {#if job.cancellable}
      <Button variant="outline" size="xs" disabled={stopping} onclick={stop} aria-label={`Stop: ${job.label}`}><Square aria-hidden="true" /> {stopping ? 'Stopping…' : 'Stop'}</Button>
    {/if}
  </div>
{/if}
