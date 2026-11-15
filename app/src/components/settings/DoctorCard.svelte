<script lang="ts">
  /** Diagnostics (spec 10.5): the doctor's checks, and Export diagnostics with redaction. */
  import Stethoscope from '@lucide/svelte/icons/stethoscope';
  import FileDown from '@lucide/svelte/icons/file-down';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Info from '@lucide/svelte/icons/info';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import DiagnosticsDialog from '@/components/activity/DiagnosticsDialog.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { CheckLevel, DoctorReport } from '@/lib/contract/types';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  let report = $state<DoctorReport | null>(null);
  let busy = $state(false);
  let diag = $state(false);

  async function run() {
    busy = true;
    try {
      report = await api.doctor();
    } catch (e) {
      toast('error', 'Doctor failed', toContractError(e).message);
    } finally {
      busy = false;
    }
  }
  const lv: Record<CheckLevel, { icon: typeof Info; label: string; cls: string }> = {
    ok: { icon: CircleCheck, label: 'OK', cls: 'text-success' },
    info: { icon: Info, label: 'Info', cls: 'text-muted-foreground' },
    warn: { icon: TriangleAlert, label: 'Warning', cls: 'text-warning' },
    error: { icon: CircleX, label: 'Problem', cls: 'text-destructive' },
  };
</script>

<Card.Root id="diagnostics">
  <Card.Header>
    <Card.Title tabindex={-1}>Diagnostics</Card.Title>
    <Card.Description>Checks installs, paths, other tools' traces and the graphics session. Changes nothing. Shown here as is; the export replaces personal values.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    <div class="flex flex-wrap gap-2">
      <Button variant="outline" size="sm" disabled={busy} onclick={run}><Stethoscope aria-hidden="true" /> {report ? 'Run again' : 'Run doctor'}</Button>
      <Button variant="outline" size="sm" onclick={() => (diag = true)}><FileDown aria-hidden="true" /> Export diagnostics</Button>
    </div>
    {#if report}
      <div class="grid gap-3" aria-live="polite">
        {#each report.sections as s (s.title)}
          <section aria-label={s.title}>
            <h3 class="eyebrow text-muted-foreground mb-1">{s.title}</h3>
            <ul class="grid gap-1">
              {#each s.checks as c, i (i)}
                {@const l = lv[c.level]}
                <li class="grid grid-cols-[84px_minmax(0,1fr)] items-start gap-2">
                  <span class={cn('inline-flex items-center gap-1 text-[12px] font-medium', l.cls)}><l.icon class="size-3.5" aria-hidden="true" />{l.label}</span>
                  <span class="min-w-0">{c.label}{#if c.detail}<span class="mono text-muted-foreground block truncate text-[11.5px]" title={c.detail}>{c.detail}</span>{/if}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}
  </Card.Content>
</Card.Root>
<DiagnosticsDialog bind:open={diag} />
