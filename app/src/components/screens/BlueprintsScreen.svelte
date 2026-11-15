<script lang="ts">
  /**
   * Blueprints (addendum §3-5, §7.19): which character's set drives the [Owned] tags (auto / one
   * character / any of them), the blueprint finder (the build's catalogue with every character's
   * ownership, per channel or all channels side by side), Smart Citizen import, and StarOrg
   * pull/push (disabled with the reason when StarOrg is off or not signed in).
   */
  import Upload from '@lucide/svelte/icons/upload';
  import Download from '@lucide/svelte/icons/download';
  import FileInput from '@lucide/svelte/icons/file-input';
  import CloudOff from '@lucide/svelte/icons/cloud-off';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import Segmented from '@/components/common/Segmented.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ChannelBadge from '@/components/common/ChannelBadge.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import ImportDialog from '@/components/blueprints/ImportDialog.svelte';
  import BlueprintFinder from '@/components/blueprints/BlueprintFinder.svelte';
  import ReimportCard from '@/components/blueprints/ReimportCard.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { CharacterMode, CharactersView, StarOrgStatus, SyncReport } from '@/lib/contract/types';
  import { fmtDateTime } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  let view = $state<CharactersView | null>(null);
  let starorg = $state<StarOrgStatus | null>(null);
  /** The finder's first picked character: the import dialog starts there. */
  let picked = $state<string | null>(null);
  let error = $state<ContractError | null>(null);
  let importing = $state(false);
  let pullInto = $state<string | null>(null);
  let syncing = $state(false);
  let lastReport = $state<SyncReport | null>(null);
  let tick = $state(0);

  const key = $derived(app.target ? `${app.target.install}/${app.target.channel}/${app.rev}/${tick}` : '');
  $effect(() => {
    if (!key || !app.target) return;
    const t = $state.snapshot(app.target);
    let stale = false;
    Promise.all([api.listCharacters(t), api.starorgStatus(t.channel)])
      .then(([v, s]) => {
        if (stale) return;
        view = v;
        starorg = s;
        error = null;
        pullInto ??= v.active.resolved ?? v.characters[0]?.id ?? null;
      })
      .catch((e) => !stale && (error = toContractError(e)));
    return () => (stale = true);
  });

  const name = (id: string | null | undefined) => view?.characters.find((c) => c.id === id)?.name ?? '—';
  const channelNames = $derived(app.channels.map((c) => c.channel));

  async function setMode(mode: CharacterMode, character: string | null = null) {
    if (!app.target || !view) return;
    try {
      view = await api.setActiveCharacter({ target: app.target, mode, character: mode === 'manual' ? (character ?? view.active.resolved ?? view.characters[0]?.id ?? null) : null });
      toast('success', 'Character for [Owned] tags saved', 'Re-apply to update the tags in game.');
    } catch (e) {
      toast('error', 'Not saved', toContractError(e).message);
    }
  }

  const syncBlocked = $derived(
    !starorg ? 'Loading…' : !starorg.enabled ? 'StarOrg Tools is off. Turn on “Connect StarOrg Tools” under Settings → Network; until then nothing is sent or received.' : !starorg.connected ? 'Not signed in. Sign in under Settings → StarOrg Tools.' : null,
  );

  const pullOff = $derived(app.settings?.starorg.pull_blueprints ? null : 'Pull is off under Settings → StarOrg Tools → What to sync.');
  const pushOff = $derived(app.settings?.starorg.push_blueprints ? null : 'Push is off under Settings → StarOrg Tools → What to sync.');

  async function pull() {
    if (!app.target || !pullInto) return;
    syncing = true;
    try {
      lastReport = await api.starorgPull({ channel: app.target.channel, character: pullInto });
      toast('success', `Pulled ${lastReport.added.length} new blueprints into ${name(pullInto)}`, `${lastReport.already} already known.`);
      tick++;
    } catch (e) {
      toast('error', 'Pull failed', toContractError(e).message);
    } finally {
      syncing = false;
    }
  }
  async function push() {
    if (!app.target) return;
    syncing = true;
    try {
      lastReport = await api.starorgPush(app.target.channel);
      toast('success', `Pushed ${lastReport.sent} blueprints to StarOrg`, 'Removals are never pushed.');
      tick++;
    } catch (e) {
      toast('error', 'Push failed', toContractError(e).message);
    } finally {
      syncing = false;
    }
  }
</script>

{#if error}
  <ErrorState title="Could not read characters" {error} onretry={() => tick++} />
{:else if !view || !app.target}
  <div class="grid gap-3"><Skeleton class="h-28 w-full" /><Skeleton class="h-64 w-full" /></div>
{:else}
  {@const v = view}
  <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(320px,380px)]">
    <Card.Root class="min-w-0 xl:col-span-2">
      <Card.Header>
        <Card.Title>Blueprints</Card.Title>
        <Card.Description>Every blueprint of the build and who owns it. Kept per character and per channel; PTU progress is not LIVE progress.</Card.Description>
        <Card.Action>
          <Button variant="outline" size="sm" onclick={() => (importing = true)}><FileInput aria-hidden="true" /> Import from Smart Citizen</Button>
        </Card.Action>
      </Card.Header>
      <Card.Content>
        <BlueprintFinder target={app.target} channels={channelNames} rev={key} bind:picked />
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title class="flex items-center gap-2">Character for [Owned] tags <ChannelBadge channel={v.channel} /></Card.Title>
        <Card.Description>Which character's blueprints are marked as owned on {v.channel}. Characters come from the game logs (last scan {fmtDateTime(v.last_scan)}).</Card.Description>
      </Card.Header>
      <Card.Content class="grid gap-3">
        <Segmented
          ariaLabel="Character mode"
          options={[
            { id: 'auto', label: 'Newest session', hint: 'The character of the newest session in this channel\'s logs' },
            { id: 'manual', label: 'One character' },
            { id: 'union', label: 'Any of my characters' },
          ]}
          value={v.active.mode}
          onchange={(m) => setMode(m as CharacterMode)}
        />
        {#if v.active.mode === 'manual'}
          <SelectField ariaLabel="Character" options={v.characters.map((c) => ({ id: c.id, label: c.name }))} value={v.active.character} onchange={(c) => setMode('manual', c)} class="max-w-60" />
        {/if}
        <p class="text-muted-foreground text-[12px]" aria-live="polite">
          {#if v.active.mode === 'union'}Owned by any of {v.characters.length} characters counts.{:else}Now using <span class="text-foreground font-medium">{name(v.active.resolved)}</span>.{/if}
          {#if v.unattributed}{v.unattributed} blueprint{v.unattributed === 1 ? ' was' : 's were'} seen before any login in a log file and belong to no character.{/if}
        </p>
      </Card.Content>
    </Card.Root>

    <ReimportCard target={app.target} view={v} shown={picked ?? v.active.resolved} ondone={() => tick++} />


    <Card.Root>
      <Card.Header>
        <Card.Title>StarOrg Tools</Card.Title>
        <Card.Description>Pull your guild's owned list into one character; push the characters you ticked in Settings as one list.</Card.Description>
      </Card.Header>
      <Card.Content class="grid gap-3">
        {#if syncBlocked}
          <div class="bg-muted/40 flex items-start gap-2 rounded-md border p-3" role="status">
            <CloudOff class="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden="true" />
            <div class="grid gap-2">
              <p>{syncBlocked}</p>
              {#if starorg}<div><Button variant="outline" size="sm" onclick={() => app.go('settings')}>Open Settings</Button></div>{/if}
            </div>
          </div>
        {:else if starorg}
          <p class="text-muted-foreground text-[12px]">Signed in as <span class="text-foreground">{starorg.user}</span> in <span class="text-foreground">{starorg.guild}</span> on {starorg.host}. Last sync {fmtDateTime(starorg.last_sync)}.</p>
        {/if}
        <div class="grid gap-1.5">
          <span class="text-muted-foreground text-[12px]" id="pull-into">Pull into</span>
          <div class="flex flex-wrap items-center gap-2">
            <SelectField ariaLabel="Pull into character" disabled={!!syncBlocked} options={v.characters.map((c) => ({ id: c.id, label: c.name }))} value={pullInto} onchange={(c) => (pullInto = c)} />
            <Button size="sm" variant="outline" disabled={!!syncBlocked || !!pullOff || syncing || !pullInto} aria-describedby={syncBlocked ? undefined : 'pull-into'} onclick={pull}><Download aria-hidden="true" /> Pull</Button>
          </div>
          {#if !syncBlocked && pullOff}<span class="text-muted-foreground text-[12px]">{pullOff}</span>{/if}
        </div>
        <div class="grid gap-1.5">
          <span class="text-muted-foreground text-[12px]">Push {starorg?.sync_characters.filter((c) => c.ticked).map((c) => c.name).join(', ') || 'no characters'}</span>
          <div class="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" disabled={!!syncBlocked || !!pushOff || syncing || !starorg?.push_count} onclick={push}><Upload aria-hidden="true" /> Push {starorg?.push_count ?? 0} blueprints</Button>
            <button type="button" class="text-primary text-[12px] underline-offset-2 hover:underline" onclick={() => app.go('settings')}>Change characters</button>
          </div>
          {#if !syncBlocked && pushOff}<span class="text-muted-foreground text-[12px]">{pushOff}</span>{/if}
        </div>
        {#if lastReport}
          <div class="rounded-md border p-3 text-[12px]" aria-live="polite">
            {#if lastReport.direction === 'pull'}
              Pulled into {name(lastReport.character)}: {lastReport.added.length} added ({lastReport.added.join(', ') || 'none'}), {lastReport.already} already known{lastReport.unmatched.length ? `, ${lastReport.unmatched.length} not in this build (${lastReport.unmatched.join(', ')})` : ''}.
            {:else}
              Pushed {lastReport.sent} blueprints to {lastReport.guild}: {lastReport.added.length} new there, {lastReport.already} already listed{lastReport.unmatched.length ? `; ${lastReport.unmatched.length} not known to StarOrg Tools, not sent (${lastReport.unmatched.join(', ')})` : ''}.
            {/if}
          </div>
        {/if}
      </Card.Content>
    </Card.Root>

  </div>
  <ImportDialog bind:open={importing} target={app.target} characters={v.characters} initial={picked ?? v.active.resolved} ondone={() => tick++} />
{/if}
