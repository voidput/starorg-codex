<script lang="ts">
  /**
   * Apply on the chosen channels with progress per sub-step and Stop while the core can still stop
   * (reading game data is the long, memory-heavy part), then Done with CLI automation (spec 9.4).
   * A returned report that is not "applied" (skipped: game running, ...) is shown as such.
   */
  import Check from '@lucide/svelte/icons/check';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Circle from '@lucide/svelte/icons/circle';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import * as Alert from '@/components/ui/alert';
  import { Progress } from '@/components/ui/progress';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import type { ApplyReport, JobProgress } from '@/lib/contract/types';
  import type { ContractError } from '@/lib/contract/client';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';
  import Square from '@lucide/svelte/icons/square';
  import { Button } from '@/components/ui/button';
  import PathText from '@/components/common/PathText.svelte';
  import { describeOutcome } from '@/lib/outcome';
  import { app } from '@/lib/stores/app.svelte';

  let {
    channels,
    phase,
    jobs,
    reports,
    error,
    onretry,
  }: {
    channels: string[];
    phase: 'idle' | 'running' | 'done' | 'failed';
    jobs: Record<string, JobProgress>;
    reports: ApplyReport[];
    error: ContractError | null;
    onretry: () => void;
  } = $props();

  const STEPS = ['Checking the game is not running', 'Reading game data', 'Building text', 'Validating', 'Backing up and writing'];
  const reportOf = (ch: string) => reports.find((r) => r.channel === ch);
  const snippet = `starorg-strings apply --all --if-stale --timeout 60 --notify`;
</script>

{#if phase === 'done'}
  <div class="flex items-center gap-2 text-[15px] font-semibold"><CircleCheck class="text-success size-5" aria-hidden="true" /> StarOrg Strings is set up</div>
{/if}

<ul class="grid gap-3" aria-label="Channels">
  {#each channels as ch (ch)}
    {@const job = jobs[ch]}
    {@const rep = reportOf(ch)}
    {@const at = rep ? STEPS.length + 1 : (job?.step ?? 0)}
    <li class="bg-card/80 rounded-lg border">
      <div class="flex items-center justify-between border-b px-3 py-2">
        <span class="font-semibold">{ch}</span>
        {#if rep && rep.result === 'applied'}
          <span class="text-success flex items-center gap-1.5"><Check class="size-4" aria-hidden="true" /> {int(rep.counts.changed)} strings changed</span>
        {:else if rep}
          <span class="text-warning flex items-center gap-1.5"><TriangleAlert class="size-4" aria-hidden="true" /> {describeOutcome(rep, 'apply').title}</span>
        {:else if job}
          <span class="text-muted-foreground flex items-center gap-2">
            {job.label}
            {#if job.cancellable && job.phase === 'running'}
              <Button variant="outline" size="xs" onclick={() => app.cancel()} aria-label={`Stop applying to ${ch}`}><Square aria-hidden="true" /> Stop</Button>
            {/if}
          </span>
        {:else}
          <span class="text-muted-foreground">Waiting</span>
        {/if}
      </div>
      <div class="grid gap-2 px-3 py-2">
        <Progress value={Math.min(at, STEPS.length)} max={STEPS.length} class="h-1" aria-label="{ch} progress" />
        <ol class="grid gap-1 sm:grid-cols-5">
          {#each STEPS as s, i (s)}
            {@const done = at > i + 1 || !!rep}
            {@const now = !rep && at === i + 1}
            <li class={cn('flex items-center gap-1.5 text-[12px]', done ? 'text-foreground' : now ? 'text-primary font-medium' : 'text-muted-foreground')}>
              {#if done}<Check class="text-success size-3.5 shrink-0" aria-hidden="true" />{:else if now}<LoaderCircle class="size-3.5 shrink-0 animate-spin" aria-hidden="true" />{:else}<Circle class="size-3.5 shrink-0" aria-hidden="true" />{/if}
              <span>{s}</span>
              <span class="sr-only">{done ? 'done' : now ? 'in progress' : 'waiting'}</span>
            </li>
          {/each}
        </ol>
        {#if rep && rep.result !== 'applied'}
          <Alert.Root variant="warning">
            <TriangleAlert aria-hidden="true" />
            <Alert.Title>{describeOutcome(rep, 'apply').title}</Alert.Title>
            <Alert.Description>{describeOutcome(rep, 'apply').detail}</Alert.Description>
          </Alert.Root>
        {:else if rep}
          {#if rep.recovered}<p class="text-muted-foreground text-[12px]">{rep.recovered}</p>{/if}
          {#if rep.warnings.length}
            <Alert.Root variant="warning">
              <TriangleAlert aria-hidden="true" />
              <Alert.Title>{rep.warnings.length} warning{rep.warnings.length === 1 ? '' : 's'}</Alert.Title>
              <Alert.Description>{#each rep.warnings as w (w.detail)}<div>{w.detail}</div>{/each}</Alert.Description>
            </Alert.Root>
          {/if}
          <dl class="text-muted-foreground grid grid-cols-[7rem_1fr] gap-x-3 text-[12px]">
            <dt>Written</dt><dd class="min-w-0"><PathText path={rep.target} /></dd>
            <dt>Backup</dt><dd class="mono">{rep.backup ?? 'none needed'}</dd>
          </dl>
        {/if}
      </div>
    </li>
  {/each}
</ul>

{#if phase === 'failed' && error}
  <ErrorState title="Apply stopped" {error} {onretry} />
{/if}

{#if phase === 'done'}
  <section aria-labelledby="automate-title" class="bg-card/80 grid gap-2 rounded-lg border p-3">
    <h3 id="automate-title" class="font-semibold">Keep it current automatically</h3>
    <p class="text-muted-foreground">
      After a game patch the text needs one re-apply. Add this to your LUG Helper pre-launch and post-exit scripts (or let Settings → Command line install the hooks):
    </p>
    <pre class="bg-background mono overflow-x-auto rounded-md border p-2"><code>{snippet}</code></pre>
    <p class="text-muted-foreground text-[12px]">It does nothing when everything is up to date, never blocks the launch for more than 60 seconds, and only touches the channels you set up here.</p>
  </section>
{/if}
