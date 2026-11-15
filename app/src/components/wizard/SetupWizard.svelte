<script lang="ts">
  /**
   * First-run setup wizard (spec 9.4, issue #15): Find your game -> Channels (consent) -> Data source
   * -> Enhancements -> Review -> Apply. Full window, on the tactical grid, chamfered frame.
   * Practice: few steps; Back/Next with the reason a step is blocked; review with jump-to-edit;
   * close guard while there are unsaved choices; focus moves to the step heading; the draft resumes.
   * `?wizard=<step>` opens at a step (headless renders); earlier steps are filled with defaults.
   */
  import { onMount, tick } from 'svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { Button } from '@/components/ui/button';
  import * as Dialog from '@/components/ui/dialog';
  import ApplyButton from '@/components/shell/ApplyButton.svelte';
  import StepInstall from './StepInstall.svelte';
  import StepChannels from './StepChannels.svelte';
  import StepSource from './StepSource.svelte';
  import StepEnhancements from './StepEnhancements.svelte';
  import StepReview from './StepReview.svelte';
  import StepApply from './StepApply.svelte';
  import { STEPS, blocker, clearDraft, emptyDraft, ensureChoices, loadDraft, request, saveDraft, type Draft, type StepId } from './draft.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { ApplyReport, InstallSummary, JobProgress } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  let { onclose }: { onclose: () => void } = $props();

  const params = new URLSearchParams(window.location.search);
  const jump = params.get('wizard') as StepId | null;
  const resumed = loadDraft();
  let draft = $state<Draft>(jump ? emptyDraft() : (resumed ?? emptyDraft()));
  let installs = $state<InstallSummary[]>([]);
  let scanning = $state(true);
  let scanError = $state<ContractError | null>(null);
  let step = $state(0);
  let showBlock = $state(false);
  let guard = $state(false);
  let heading = $state<HTMLElement | null>(null);

  let phase = $state<'idle' | 'running' | 'done' | 'failed'>('idle');
  let jobs = $state<Record<string, JobProgress>>({});
  let reports = $state<ApplyReport[]>([]);
  let runError = $state<ContractError | null>(null);
  let applyChannels = $state<string[]>([]);

  const install = $derived(installs.find((i) => i.id === draft.install) ?? null);
  const cur = $derived(STEPS[step]!);
  const block = $derived(blocker(cur.id, draft, install));
  const firstTarget = $derived.by(() => {
    if (!install) return null;
    const c = install.channels.find((x) => draft.channels[x.channel]?.consent) ?? install.channels[0];
    return c ? { install: install.id, channel: c.channel } : null;
  });
  /** Choices worth guarding: anything beyond the first step, before setup is done. */
  const dirty = $derived(phase !== 'done' && draft.reached > 0);

  $effect(() => {
    if (phase !== 'done') saveDraft($state.snapshot(draft) as Draft);
  });
  $effect(() => {
    ensureChoices(draft, install);
  });

  async function scan(deep = false) {
    scanning = true;
    scanError = null;
    try {
      const found = await api.discoverInstalls(deep);
      // Keep a folder the user browsed to.
      installs = [...found, ...installs.filter((i) => i.found_by === 'manual' && !found.some((f) => f.root === i.root))];
      if (!draft.install || !installs.some((i) => i.id === draft.install)) draft.install = installs[0]?.id ?? null;
    } catch (e) {
      scanError = toContractError(e);
    } finally {
      scanning = false;
    }
  }

  async function browse() {
    try {
      const picked = await api.pickInstallFolder();
      if (!picked) return;
      installs = [...installs.filter((i) => i.root !== picked.root), picked];
      draft.install = picked.id;
    } catch (e) {
      toast('error', 'That folder is not a Star Citizen install', toContractError(e).message);
    }
  }

  async function focusStep() {
    await tick();
    heading?.focus();
  }

  function go(i: number) {
    step = Math.max(0, Math.min(STEPS.length - 1, i));
    draft.reached = Math.max(draft.reached, step);
    showBlock = false;
    void focusStep();
  }

  function next() {
    if (block) {
      showBlock = true;
      return;
    }
    go(step + 1);
  }

  /** A rail step is reachable when it was reached before and nothing before it blocks. */
  function reachable(i: number): boolean {
    if (phase === 'running' || phase === 'done') return false;
    if (i > draft.reached || i === STEPS.length - 1) return false;
    return STEPS.slice(0, i).every((s) => !blocker(s.id, draft, install));
  }

  async function run() {
    if (!install) return;
    const req = request(draft, install);
    applyChannels = req.channels.filter((c) => c.consent).map((c) => c.channel);
    go(STEPS.length - 1);
    phase = 'running';
    runError = null;
    jobs = {};
    reports = [];
    const off = await api.onJob((p) => {
      if (p.channel) jobs = { ...jobs, [p.channel]: p };
    });
    try {
      const res = await api.setup(req);
      applyChannels = res.set_up;
      for (const ch of res.set_up) {
        const r = await api.apply({ target: { install: install.id, channel: ch }, dry_run: false });
        reports = [...reports, r];
      }
      phase = 'done';
      clearDraft();
      await app.refresh();
      app.selectChannel(res.set_up[0] ?? app.channelName ?? '');
    } catch (e) {
      runError = toContractError(e);
      phase = 'failed';
    } finally {
      off();
    }
  }

  function requestClose() {
    if (phase === 'running') return;
    if (dirty) guard = true;
    else finish();
  }
  function finish() {
    guard = false;
    onclose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !guard && !document.querySelector('[data-slot=dialog-content]')) {
      e.preventDefault();
      requestClose();
    }
  }

  onMount(() => {
    void (async () => {
      await scan();
      if (jump) {
        const i = STEPS.findIndex((s) => s.id === jump);
        if (i >= 0) {
          ensureChoices(draft, install);
          draft.reached = i;
          step = i;
          if (jump === 'apply') void run();
        }
      } else if (resumed) {
        // Resume at the furthest step that is still valid.
        let i = 0;
        while (i < Math.min(resumed.reached, STEPS.length - 2) && !blocker(STEPS[i]!.id, draft, install)) i++;
        step = i;
      }
    })();
    const beforeUnload = (e: BeforeUnloadEvent) => {
      if (dirty || phase === 'running') e.preventDefault();
    };
    window.addEventListener('beforeunload', beforeUnload);
    return () => window.removeEventListener('beforeunload', beforeUnload);
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="bg-background tactical-grid fixed inset-0 z-40 flex items-stretch justify-center overflow-y-auto p-4 sm:p-6" role="presentation">
  <div
    class="glass chamfer-md shadow-glow-md border-primary/30 relative my-auto flex h-full max-h-[760px] w-full max-w-5xl flex-col border"
    role="dialog"
    aria-modal="true"
    aria-labelledby="wizard-title"
  >
    <header class="flex items-start justify-between gap-4 border-b px-6 pt-5 pb-4">
      <div>
        <div class="eyebrow text-primary">First-run setup</div>
        <h1 id="wizard-title" class="text-h1">Set up StarOrg Strings</h1>
      </div>
      <Button variant="ghost" size="sm" onclick={requestClose} disabled={phase === 'running'}>
        {#if phase === 'done'}Close{:else}<X aria-hidden="true" /> Set up later{/if}
      </Button>
    </header>

    <nav aria-label="Setup steps" class="border-b px-6 py-3">
      <ol class="flex items-center gap-1">
        {#each STEPS as s, i (s.id)}
          {@const isNow = i === step}
          {@const isDone = i < step || phase === 'done'}
          {@const can = reachable(i) && !isNow}
          <li class="flex min-w-0 flex-1 items-center gap-1">
            <button
              type="button"
              class={cn(
                'focus-visible:ring-ring/70 flex min-w-0 items-center gap-2 rounded-md px-1 py-1 text-left outline-none focus-visible:ring-2',
                can ? 'hover:bg-accent/60 cursor-pointer' : 'cursor-default',
              )}
              aria-current={isNow ? 'step' : undefined}
              aria-disabled={!can}
              onclick={() => can && go(i)}
            >
              <span
                class={cn(
                  'chamfer-xs mono flex size-7 shrink-0 items-center justify-center text-[12px] font-semibold',
                  isNow ? 'bg-primary text-primary-foreground shadow-glow-sm' : isDone ? 'bg-success/20 text-success' : 'bg-muted text-muted-foreground',
                )}
                aria-hidden="true"
              >
                {#if isDone && !isNow}<Check class="size-3.5" />{:else}{i + 1}{/if}
              </span>
              <span class={cn('hidden truncate text-[12.5px] md:inline', isNow ? 'font-semibold' : 'text-muted-foreground')}>{s.short}</span>
              <span class="sr-only">Step {i + 1} of {STEPS.length}: {s.title}{isDone && !isNow ? ', done' : ''}</span>
            </button>
            {#if i < STEPS.length - 1}<span class={cn('h-px min-w-3 flex-1', i < step ? 'bg-success/50' : 'bg-border')} aria-hidden="true"></span>{/if}
          </li>
        {/each}
      </ol>
    </nav>

    <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
      <div class="mx-auto grid max-w-4xl gap-4">
        <div>
          <div class="eyebrow text-muted-foreground">Step {step + 1} of {STEPS.length}</div>
          <h2 id="wizard-step-title" bind:this={heading} tabindex="-1" class="text-[18px] font-semibold outline-none">{phase === 'done' ? 'Done' : cur.title}</h2>
        </div>
        {#if cur.id === 'install'}
          <StepInstall {installs} {scanning} error={scanError} bind:value={draft.install} onscan={scan} onbrowse={browse} />
        {:else if cur.id === 'channels' && install}
          <StepChannels {install} {draft} />
        {:else if cur.id === 'source'}
          <StepSource {draft} />
        {:else if cur.id === 'enhancements'}
          <StepEnhancements {draft} target={firstTarget} />
        {:else if cur.id === 'review' && install}
          <StepReview {draft} {install} onedit={(s) => go(STEPS.findIndex((x) => x.id === s))} />
        {:else if cur.id === 'apply'}
          <StepApply channels={applyChannels} {phase} {jobs} {reports} error={runError} onretry={run} />
        {/if}
      </div>
    </div>

    <footer class="flex flex-wrap items-center gap-3 border-t px-6 py-3">
      {#if showBlock && block}
        <p id="wizard-block" class="text-warning flex items-center gap-1.5" role="alert"><TriangleAlert class="size-4" aria-hidden="true" /> {block}</p>
      {/if}
      <span class="flex-1"></span>
      {#if cur.id !== 'apply'}
        <Button variant="outline" onclick={() => go(step - 1)} disabled={step === 0}><ArrowLeft aria-hidden="true" /> Back</Button>
      {/if}
      {#if cur.id === 'review'}
        <ApplyButton label="Set up and apply" onclick={run} disabled={!install} />
      {:else if cur.id === 'apply'}
        {#if phase === 'failed'}
          <Button variant="outline" onclick={() => { phase = 'idle'; go(STEPS.length - 2); }}><ArrowLeft aria-hidden="true" /> Back to review</Button>
        {:else if phase === 'done'}
          <Button onclick={finish}>Open StarOrg Strings <ArrowRight aria-hidden="true" /></Button>
        {/if}
      {:else}
        <Button onclick={next} aria-describedby={showBlock && block ? "wizard-block" : undefined}>Next <ArrowRight aria-hidden="true" /></Button>
      {/if}
    </footer>
  </div>
</div>

<Dialog.Root bind:open={guard}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Leave setup for now?</Dialog.Title>
      <Dialog.Description>Nothing has been written to the game yet. Your choices are kept, and setup continues where you left off next time.</Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (guard = false)}>Keep setting up</Button>
      <Button onclick={finish}>Leave setup</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
