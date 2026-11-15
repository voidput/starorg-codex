<script lang="ts">
  /** Review with jump-to-edit (spec 9.4 step 5): target path and link target, counts, backup location, the user.cfg line, and the Preferences. */
  import Pencil from '@lucide/svelte/icons/pencil';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { Button } from '@/components/ui/button';
  import * as Alert from '@/components/ui/alert';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { InstallSummary, SetupPlan } from '@/lib/contract/types';
  import { int } from '@/lib/format';
  import { updates } from '@/lib/stores/update.svelte';
  import { storeManaged } from '@/lib/update';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import WorkingState from '@/components/common/WorkingState.svelte';
  import { latest, type Load } from '@/lib/loadstate';
  import { ACK_LABEL, foreignBefore } from '@/lib/foreign';
  import { BUSY_RETRY_MS, foreignPicked, prefsSummary, request, unreadChannels, type Draft, type StepId } from './draft.svelte';

  let {
    draft,
    install,
    onedit,
    plan = $bindable(null),
  }: {
    draft: Draft;
    install: InstallSummary;
    onedit: (s: StepId) => void;
    /** The answered plan, for the wizard's gate (reviewBlock); null while reading or failed. */
    plan?: SetupPlan | null;
  } = $props();

  let load = $state<Load<SetupPlan, ContractError>>({ state: 'idle' });
  /** When the current read started, kept across busy retries so the timer does not reset. */
  let since = $state(Date.now());
  const reqJson = $derived(JSON.stringify(request(draft, install)));
  // One runner per mount: a slower, older answer never replaces the newer one.
  const runner = latest<SetupPlan, ContractError>((x) => (load = x), toContractError);
  const reload = (json: string) => runner.run(() => api.planSetup(JSON.parse(json)));
  $effect(() => {
    since = Date.now();
    void reload(reqJson);
  });
  $effect(() => () => runner.stop());
  // Another process (a launch hook, the command line) is reading game data: ask again shortly.
  const busy = $derived(load.state === 'error' && load.error.code === 'busy');
  $effect(() => {
    if (!busy) return;
    const json = reqJson;
    const t = setTimeout(() => void reload(json), BUSY_RETRY_MS);
    return () => clearTimeout(t);
  });
  $effect(() => {
    plan = load.state === 'ok' ? load.value : null;
  });
  const unread = $derived(load.state === 'ok' ? unreadChannels(load.value) : []);
  const unreadOf = (ch: string) => unread.find((u) => u.channel === ch) ?? null;

  const picked = $derived(install.channels.filter((c) => draft.channels[c.channel]?.consent));
  const prefRows = $derived(draft.prefs ? prefsSummary(draft.prefs, storeManaged(updates.status)) : []);
  const foreign = $derived(foreignPicked(draft, install, load.state === 'ok' ? load.value : null));
  const reading = $derived(picked.map((c) => c.channel).join(' and ') || 'the chosen channels');
  const presetName = $derived({ recommended: 'Recommended', minimal: 'Minimal', everything: 'Everything' }[draft.preset]);
  import PathText from '@/components/common/PathText.svelte';
</script>

{#snippet edit(step: StepId, what: string)}
  <Button variant="ghost" size="xs" onclick={() => onedit(step)} aria-label="Edit {what}"><Pencil aria-hidden="true" /> Edit</Button>
{/snippet}

<dl class="bg-card/80 divide-y rounded-lg border">
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Game</dt>
    <dd class="min-w-0"><PathText path={install.root} /></dd>
    {@render edit('install', 'game')}
  </div>
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Channels</dt>
    <dd>
      {picked.map((c) => c.channel).join(', ') || 'None'}
      {#if install.channels.length > picked.length}<span class="text-muted-foreground"> · not set up: {install.channels.filter((c) => !picked.includes(c)).map((c) => c.channel).join(', ')}</span>{/if}
    </dd>
    {@render edit('channels', 'channels')}
  </div>
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Game data</dt>
    <dd>{draft.data_source === 'local' ? 'Read on this computer (no network)' : 'Published game data when available (network)'}</dd>
    {@render edit('source', 'game data source')}
  </div>
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Enhancements</dt>
    <dd>{presetName}{draft.fix_known_bugs ? ', with text fixes' : ', no text fixes'}</dd>
    {@render edit('enhancements', 'enhancements')}
  </div>
  {#each prefRows as row, i (row.label)}
    <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
      <dt class="text-muted-foreground">{row.label}</dt>
      <dd>{row.value}</dd>
      {@render edit('prefs', row.label.toLowerCase())}
    </div>
  {/each}
</dl>

{#if foreign.length}
  <section aria-labelledby="foreign-title" class="border-warning/50 bg-warning/10 grid gap-2 rounded-lg border p-3" data-testid="foreign-review">
    <h3 id="foreign-title" class="flex items-center gap-2 font-semibold"><ShieldAlert class="text-warning size-4" aria-hidden="true" /> Another tool's text is in place</h3>
    {#each foreign as f (f.channel)}
      {@const t = foreignBefore(f)}
      <div class="grid gap-0.5">
        <p class="font-medium">{t.title}</p>
        <p class="text-muted-foreground">{t.body}</p>
        <p class="text-muted-foreground">{t.stop}</p>
      </div>
    {/each}
    <p class="text-[12.5px] font-medium">Tick “{ACK_LABEL}” next to “Set up and apply” to go on.</p>
  </section>
{/if}

<section aria-labelledby="writes-title" class="grid gap-2">
  <h3 id="writes-title" class="eyebrow text-muted-foreground">What will be written</h3>
  {#if busy}
    <WorkingState
      label="Waiting for another read"
      title="Waiting for another game-data read to finish…"
      detail="A launch hook or the command line is reading Data.p4k right now. The review starts by itself when it is done."
      since={since}
    />
  {:else if load.state === 'error'}
    <ErrorState title="Could not prepare the review" error={load.error} onretry={() => reload(reqJson)} />
  {:else if load.state !== 'ok'}
    <WorkingState
      label="Preparing the review"
      title="Reading game data for {reading}…"
      detail="A dry run: StarOrg Codex reads each channel's text and game data from its Data.p4k and counts what would change. Nothing is written yet."
      slow="The first read of a large Data.p4k can take a few minutes on a hard drive, or while the RSI Launcher is verifying or downloading. Later runs use the saved data and take seconds."
      {since}
    />
  {:else}
    {@const plan = load.value}
    {#if !plan.channels.length && !plan.warnings.length}
      <p class="text-muted-foreground bg-card/80 rounded-lg border p-3">Nothing will be written: no channel is chosen.</p>
    {/if}
    {#each unread.filter((u) => !plan.channels.some((c) => c.channel === u.channel)) as u (u.channel + u.why)}
      <ErrorState title="Could not read the game data{u.channel ? ` of ${u.channel}` : ''}" error={{ message: u.why }} onretry={() => reload(reqJson)} />
    {/each}
    {#each plan.warnings.filter((w) => w.kind !== 'game-data') as w (w.kind + w.detail)}
      <Alert.Root variant="warning"><TriangleAlert aria-hidden="true" /><Alert.Description>{w.detail}</Alert.Description></Alert.Root>
    {/each}
    {#each plan.channels as c (c.channel)}
      {@const bad = unreadOf(c.channel)}
      <article class="bg-card/80 rounded-lg border" aria-label="{c.channel} plan">
        <header class="flex items-baseline justify-between border-b px-3 py-2">
          <span class="font-semibold">{c.channel}</span>
          {#if bad}
            <span class="text-destructive">Game data not read</span>
          {:else}
            <span class="text-muted-foreground"><span class="mono text-foreground">{int(c.counts.changed)}</span> strings changed</span>
          {/if}
        </header>
        {#if bad}
          <div class="px-3 pt-2">
            <ErrorState title="Could not read {c.channel}'s game data" error={{ message: bad.why }} onretry={() => reload(reqJson)} />
          </div>
        {/if}
        <dl class="grid grid-cols-[9rem_1fr] gap-x-3 gap-y-1 px-3 py-2">
          <dt class="text-muted-foreground">Writes</dt>
          <dd class="min-w-0"><PathText path={c.target} />{#if c.link_target}<span class="text-muted-foreground"> → <PathText path={c.link_target} /></span>{/if}</dd>
          <dt class="text-muted-foreground">Backups</dt>
          <dd class="min-w-0"><PathText path={c.backup_dir} /></dd>
          <dt class="text-muted-foreground">user.cfg</dt>
          <dd>{#if c.user_cfg_line}adds <span class="mono">{c.user_cfg_line}</span>{:else}No change needed{/if}</dd>
          {#if c.replaces_tool}
            <dt class="text-muted-foreground">Replaces</dt>
            <dd>{c.replaces_tool}'s file (backed up first, kept for good)</dd>
          {/if}
          {#if !bad}
          <dt class="text-muted-foreground">Counts</dt>
          <dd class="mono text-muted-foreground">
            ships {int(c.counts.ships)} · components {int(c.counts.components)} · ship weapons {int(c.counts.ship_weapons)} · FPS {int(c.counts.fps_weapons)} · missiles {int(c.counts.missiles)} · missions {int(c.counts.missions)}
          </dd>
          {/if}
        </dl>
      </article>
    {/each}
  {/if}
</section>
