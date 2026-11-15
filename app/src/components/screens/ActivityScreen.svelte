<script lang="ts">
  /** Activity (spec 9.4): the job log (applies, launch hooks, imports, syncs) with levels and filters, Copy, Export diagnostics. */
  import Info from '@lucide/svelte/icons/info';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Copy from '@lucide/svelte/icons/copy';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import FileDown from '@lucide/svelte/icons/file-down';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import Segmented from '@/components/common/Segmented.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import DiagnosticsDialog from '@/components/activity/DiagnosticsDialog.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { JobKind, LogEntry, LogLevel } from '@/lib/contract/types';
  import { fmtDateTime, JOB_LABEL } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  let entries = $state<LogEntry[] | null>(null);
  let error = $state<ContractError | null>(null);
  let level = $state<'all' | 'warn' | 'error'>('all');
  let kind = $state<'all' | JobKind>('all');
  let diag = $state(false);
  let tick = $state(0);

  $effect(() => {
    void app.rev;
    void tick;
    let stale = false;
    api
      .activity(500)
      .then((e) => !stale && ((entries = e), (error = null)))
      .catch((e) => !stale && (error = toContractError(e)));
    return () => (stale = true);
  });

  const rank: Record<LogLevel, number> = { info: 0, warn: 1, error: 2 };
  const shown = $derived(
    (Array.isArray(entries) ? entries : []).filter((e) => (level === 'all' || rank[e.level] >= rank[level]) && (kind === 'all' || e.job === kind)),
  );
  const kinds = $derived([
    { id: 'all' as const, label: 'All jobs' },
    ...[...new Set((entries ?? []).map((e) => e.job).filter((j): j is JobKind => j !== null))].map((j) => ({ id: j, label: JOB_LABEL[j] })),
  ]);
  const lv = {
    info: { icon: Info, label: 'Info', cls: 'text-muted-foreground' },
    warn: { icon: TriangleAlert, label: 'Warning', cls: 'text-warning' },
    error: { icon: CircleX, label: 'Error', cls: 'text-destructive' },
  } as const;

  async function copy() {
    const text = shown.map((e) => `${e.time} ${e.level.toUpperCase()} ${e.job ?? '-'} ${e.channel ?? '-'} ${e.message}`).join('\n');
    try {
      await navigator.clipboard.writeText(text);
      toast('success', `Copied ${shown.length} log lines`, 'Paths are not redacted here; use Export diagnostics to share.');
    } catch {
      toast('error', 'Copy failed', 'The clipboard is not available.');
    }
  }
  import { logText } from '@/lib/messages';
</script>

<div class="grid gap-3">
  <div class="flex flex-wrap items-center gap-2" role="toolbar" aria-label="Log filters">
    <Segmented
      ariaLabel="Level"
      options={[{ id: 'all', label: 'All' }, { id: 'warn', label: 'Warnings and errors' }, { id: 'error', label: 'Errors' }]}
      value={level}
      onchange={(v) => (level = v as typeof level)}
    />
    <SelectField ariaLabel="Job" options={kinds} value={kind} onchange={(v) => (kind = v)} class="min-w-36" />
    <span class="flex-1"></span>
    <Button variant="ghost" size="sm" onclick={() => tick++}><RefreshCw aria-hidden="true" /> Refresh</Button>
    <Button variant="outline" size="sm" disabled={!shown.length} onclick={copy}><Copy aria-hidden="true" /> Copy</Button>
    <Button variant="outline" size="sm" onclick={() => (diag = true)}><FileDown aria-hidden="true" /> Export diagnostics</Button>
  </div>
  {#if error}
    <ErrorState title="Could not read the log" {error} onretry={() => tick++} />
  {:else if !entries}
    <Skeleton class="h-64 w-full" />
  {:else if !shown.length}
    <p class="text-muted-foreground bg-card rounded-lg border py-10 text-center">{entries.length ? 'Nothing matches these filters.' : 'Nothing has happened yet. Applies, launch hooks and imports show here.'}</p>
  {:else}
    <ol class="bg-card divide-y rounded-lg border" aria-label="Log, newest first">
      {#each shown as e (e.id)}
        {@const l = lv[e.level]}
        {@const m = logText(e.message)}
        <li class="grid grid-cols-[176px_96px_minmax(0,1fr)] items-start gap-3 px-4 py-2 lg:grid-cols-[176px_100px_124px_minmax(0,1fr)]">
          <time class="mono text-muted-foreground text-[12px]" datetime={e.time}>{fmtDateTime(e.time)}</time>
          <span class={cn('inline-flex items-center gap-1 text-[12px] font-medium', l.cls)}><l.icon class="size-3.5" aria-hidden="true" />{l.label}</span>
          <span class="text-muted-foreground hidden text-[12px] lg:block">{e.job ? JOB_LABEL[e.job] : 'App'}{e.channel ? ` · ${e.channel}` : ''}</span>
          <span class="min-w-0 break-words">
            <span class="text-muted-foreground text-[12px] lg:hidden">{e.job ? JOB_LABEL[e.job] : 'App'}{e.channel ? ` · ${e.channel}` : ''} — </span>{m.text}
            {#if m.raw}<span class="mono text-muted-foreground block text-[11.5px]">{m.raw}</span>{/if}
          </span>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<DiagnosticsDialog bind:open={diag} />
