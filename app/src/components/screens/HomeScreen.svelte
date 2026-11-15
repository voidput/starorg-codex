<script lang="ts">
  /**
   * Home (spec 9.4): the current channel's status card (applied build vs game build, last apply,
   * warnings), quick actions, stat tiles from the last apply, and other channels as rows.
   * The one Apply button is in the header; the card offers only the action the status calls for
   * (re-apply after a game update, replace another tool's file, set up the channel).
   */
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Wand from '@lucide/svelte/icons/wand-sparkles';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import FolderSearch from '@lucide/svelte/icons/folder-search';
  import { Button } from '@/components/ui/button';
  import * as Alert from '@/components/ui/alert';
  import RemoveDialog from '@/components/common/RemoveDialog.svelte';
  import PathText from '@/components/common/PathText.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import ApplyStatusPill from '@/components/shell/ApplyStatusPill.svelte';
  import { Skeleton } from '@/components/ui/skeleton';
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import Archive from '@lucide/svelte/icons/archive';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import Table2 from '@lucide/svelte/icons/table-2';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Stat from '@/components/common/Stat.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import RotateCw from '@lucide/svelte/icons/rotate-cw';
  import Replace from '@lucide/svelte/icons/replace';
  import Eraser from '@lucide/svelte/icons/eraser';
  import { foreignBeforeApply, foreignOf } from '@/lib/foreign';
  import { buildLabel, fmtDateTime, fmtMs, int, sameBuild, statusView } from '@/lib/format';

  const ch = $derived(app.channel);
  const others = $derived(app.channels.filter((c) => c.channel !== ch?.channel));
  let removing = $state(false);
  const disabledWhy = $derived(app.blocked ?? undefined);
</script>

{#if app.loading && !app.overview}
  <div class="flex flex-col gap-3"><Skeleton class="h-40 w-full" /><Skeleton class="h-16 w-full" /></div>
{:else if app.error}
  <ErrorState title="Could not read your game channels" error={app.error} onretry={() => app.refresh()} />
{:else if !app.install}
  <FullEmpty icon={FolderSearch} title="No Star Citizen install found">
    Looked in the LUG Helper config, default Wine prefixes, Lutris and the RSI Launcher logs.
    {#snippet actions()}<Button onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Find my game</Button>{/snippet}
  </FullEmpty>
{:else if ch}
  {@const s = statusView(ch.status, ch.set_up)}
  {@const fc = foreignOf(ch)}
  <div class="flex flex-col gap-4">
    {#if fc}
      {@const t = foreignBeforeApply(fc)}
      <Alert.Root variant="warning" data-testid="foreign-home">
        <TriangleAlert aria-hidden="true" />
        <Alert.Title>{t.title}</Alert.Title>
        <Alert.Description>
          <p>{t.body}</p>
          <p>{t.stop}</p>
        </Alert.Description>
      </Alert.Root>
    {/if}
    {#if ch.status.state === 'stale' && ch.status.reasons.includes('build')}
      <Alert.Root variant="warning">
        <RefreshCw aria-hidden="true" />
        <Alert.Title>The game was updated</Alert.Title>
        <Alert.Description>
          The text in place was made for {ch.applied_build ? buildLabel(ch.applied_build) : 'an older build'}. Re-apply so it matches build {buildLabel(ch.build)}; until then new items show stock text.
        </Alert.Description>
      </Alert.Root>
    {/if}
    {#if ch.interrupted_apply}
      <Alert.Root variant="destructive" role="alert">
        <CircleAlert aria-hidden="true" />
        <Alert.Title>The last apply did not finish</Alert.Title>
        <Alert.Description>The game file was left as it was before. Apply again, or restore a backup.</Alert.Description>
      </Alert.Root>
    {/if}
    {#if ch.build_error}
      <Alert.Root variant="warning">
        <TriangleAlert aria-hidden="true" />
        <Alert.Title>Could not read the game build</Alert.Title>
        <Alert.Description class="mono break-all">{ch.build_error}</Alert.Description>
      </Alert.Root>
    {/if}
    <article class="bg-card chamfer-md chamfer-edge shadow-glow-sm relative border" aria-labelledby="status-title">
      <div class="flex flex-wrap items-start justify-between gap-3 border-b px-5 py-4">
        <div>
          <div class="eyebrow text-muted-foreground">Channel status</div>
          <h2 id="status-title" class="text-[16px] font-semibold">{ch.channel}</h2>
          <p class="text-muted-foreground">{s.hint}</p>
        </div>
        <ApplyStatusPill channel={ch} />
      </div>
      <dl class="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-6 gap-y-1.5 px-5 py-4">
        <dt class="text-muted-foreground">Game build</dt>
        <dd class="mono">{ch.build.branch ?? '—'} · {ch.build.version ?? '—'} · {ch.build.change ?? 'unknown'}</dd>
        <dt class="text-muted-foreground">Applied build</dt>
        <dd class="flex flex-wrap items-center gap-2">
          {#if ch.applied_build}
            <span class="mono">{buildLabel(ch.applied_build)}</span>
            {#if sameBuild(ch.applied_build, ch.build)}
              <ToneBadge tone="success" label="Matches the game" />
            {:else}
              <ToneBadge tone="warning" label="Made for another build" hint="Re-apply to match the build the game has now" />
            {/if}
          {:else}
            <span class="text-muted-foreground">Nothing of ours is in place</span>
          {/if}
        </dd>
        <dt class="text-muted-foreground">Last apply</dt>
        <dd>
          {#if ch.last_apply}
            {fmtDateTime(ch.last_apply.time)} · {int(ch.last_apply.changed)} strings · {fmtMs(ch.last_apply.duration_ms)}
            {#if ch.last_apply.warnings}
              <button type="button" class="text-warning ml-1 underline-offset-2 hover:underline" onclick={() => app.go('activity')}>{ch.last_apply.warnings} warning{ch.last_apply.warnings === 1 ? '' : 's'}</button>
            {/if}
          {:else}
            <span class="text-muted-foreground">Never</span>
          {/if}
        </dd>
        <dt class="text-muted-foreground">Install</dt>
        <dd class="min-w-0"><PathText path={app.install.root} /></dd>
        <dt class="text-muted-foreground">Data.p4k</dt>
        <dd class="mono">{fmtDateTime(ch.p4k_time)}</dd>

      </dl>
      {#if ch.game_running}
        <div class="px-5 pb-3">
          <Alert.Root variant="warning"><TriangleAlert aria-hidden="true" /><Alert.Description>The game is running on {ch.channel}. Close it to apply, remove or restore.</Alert.Description></Alert.Root>
        </div>
      {/if}
      <div class="flex flex-wrap items-center gap-x-2 gap-y-2 border-t px-5 py-3">
        {#if ch.set_up}
          <div class="flex flex-wrap items-center gap-2">
            {#if ch.status.state === 'stale'}
              <Button size="sm" disabled={!!app.blocked} title={disabledWhy} onclick={() => app.requestApply()}>
                <RotateCw aria-hidden="true" /> {ch.status.reasons.includes('build') ? 'Re-apply for the new build' : 'Re-apply with your changes'}
              </Button>
            {:else if ch.status.state === 'changed-by-another-tool'}
              <Button size="sm" disabled={!!app.blocked} title={disabledWhy} onclick={() => app.requestApply()}>
                <Replace aria-hidden="true" /> Replace {ch.foreign_tool ? `${ch.foreign_tool}'s` : 'the other'} file with ours
              </Button>
            {/if}
            <Button variant="outline" size="sm" disabled={!!app.blocked} title={disabledWhy} onclick={() => app.apply(true)}><FlaskConical aria-hidden="true" /> Dry run</Button>
            {#if ch.status.state !== 'not-applied'}
              <Button variant="outline" size="sm" disabled={!!app.blocked} title={disabledWhy} onclick={() => (removing = true)}><Eraser aria-hidden="true" /> Remove changes</Button>
            {/if}
          </div>
          <nav aria-label="Go to" class="ml-auto flex flex-wrap items-center gap-1">
            <Button variant="ghost" size="sm" onclick={() => app.go('enhancements')}><Sparkles aria-hidden="true" /> Enhancements</Button>
            <Button variant="ghost" size="sm" onclick={() => app.go('strings')}><Table2 aria-hidden="true" /> Strings</Button>
            <Button variant="ghost" size="sm" onclick={() => app.go('backups')}><Archive aria-hidden="true" /> Backups</Button>
          </nav>
        {:else}
          <Button onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Set up {ch.channel}</Button>
        {/if}
      </div>
    </article>

    {#if ch.last_apply}
      <section aria-label="Last apply in numbers" class="grid grid-cols-2 gap-2 lg:grid-cols-4">
        <Stat label="Strings changed" value={int(ch.last_apply.changed)} />
        <Stat label="Warnings" value={int(ch.last_apply.warnings)} hint={ch.last_apply.warnings ? 'See Activity' : null} />
        <Stat label="Edits dropped" value={int(ch.last_apply.dropped)} hint="Overrides that broke a rule" />
        <Stat label="Took" value={fmtMs(ch.last_apply.duration_ms)} />
      </section>
    {/if}

    {#if others.length}
      <section aria-labelledby="others-title" class="flex flex-col gap-2">
        <h2 id="others-title" class="eyebrow text-muted-foreground">Other channels</h2>
        <ul class="bg-card divide-y rounded-lg border">
          {#each others as o (o.channel)}
            <li class="flex items-center gap-3 px-4 py-2">
              <span class="w-20 font-semibold">{o.channel}</span>
              <span class="mono text-muted-foreground min-w-0 flex-1 truncate">{buildLabel(o.build)}</span>
              <span class="text-muted-foreground hidden text-[12px] lg:inline">{o.last_apply ? `Applied ${fmtDateTime(o.last_apply.time)}` : 'Never applied'}</span>
              <ApplyStatusPill channel={o} />
              <Button variant="ghost" size="sm" aria-label={`Open ${o.channel}`} onclick={() => app.selectChannel(o.channel)}>Open</Button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
{/if}
{#if ch}
  <RemoveDialog bind:open={removing} channel={ch} />
{/if}
