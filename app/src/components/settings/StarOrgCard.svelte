<script lang="ts">
  /**
   * StarOrg Tools (addendum §5): environment (starorg.tools / dev.starorg.tools), sign-in in the
   * system browser (loopback PKCE; cancellable while it waits), the org, "What to sync" (one switch
   * per kind, stored in config.toml so the CLI and launch hooks follow them), the characters to
   * push (Q22 = b: the union of the ticked ones) and Sync now. Switching environment signs out
   * after a confirm: tokens are per environment.
   */
  import LogIn from '@lucide/svelte/icons/log-in';
  import LogOut from '@lucide/svelte/icons/log-out';
  import CloudOff from '@lucide/svelte/icons/cloud-off';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Label } from '@/components/ui/label';
  import { Skeleton } from '@/components/ui/skeleton';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import Segmented from '@/components/common/Segmented.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ActionDialog from '@/components/common/ActionDialog.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { Settings, StarOrgEnv, StarOrgSettings, StarOrgStatus } from '@/lib/contract/types';
  import { fmtDate, fmtDateTime } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  const HOSTS: Record<StarOrgEnv, string> = { production: 'starorg.tools', dev: 'dev.starorg.tools' };

  let st = $state<StarOrgStatus | null>(null);
  let busy = $state(false);
  let waiting = $state(false);
  let syncing = $state(false);
  let confirmEnv = $state<StarOrgEnv | null>(null);
  let confirmOpen = $state(false);
  const channel = $derived(app.channel?.channel ?? 'LIVE');
  const enabled = $derived(app.settings?.network.starorg ?? false);
  const so = $derived(app.settings?.starorg ?? null);
  const env = $derived<StarOrgEnv>(so?.environment ?? 'production');

  $effect(() => {
    const c = channel;
    void enabled;
    void env;
    let stale = false;
    api
      .starorgStatus(c)
      .then((s) => !stale && (st = s))
      .catch((e) => !stale && toast('error', 'Could not read the StarOrg status', toContractError(e).message));
    return () => (stale = true);
  });

  function patch(f: (s: Settings) => void): Promise<boolean> {
    if (!app.settings) return Promise.resolve(false);
    const s = structuredClone($state.snapshot(app.settings)) as Settings;
    f(s);
    return app.saveSettings(s);
  }

  async function refresh() {
    try {
      st = await api.starorgStatus(channel);
    } catch {
      /* the effect reports read errors */
    }
  }

  function setSync<K extends keyof StarOrgSettings>(k: K, v: StarOrgSettings[K]) {
    void patch((x) => (x.starorg[k] = v));
  }

  function pickEnv(v: StarOrgEnv) {
    if (v === env) return;
    if (st?.connected) {
      confirmEnv = v;
      confirmOpen = true;
    } else void patch((x) => (x.starorg.environment = v)).then(refresh);
  }

  async function switchEnv() {
    const v = confirmEnv;
    if (!v) return;
    const from = HOSTS[env];
    const ok = await patch((x) => (x.starorg.environment = v));
    if (!ok) throw new Error('Not saved');
    await refresh();
    toast('success', `Now using ${HOSTS[v]}`, `Signed out of ${from}.`);
  }

  async function run(f: () => Promise<StarOrgStatus>, ok: string) {
    busy = true;
    try {
      st = await f();
      toast('success', ok, null);
    } catch (e) {
      const c = toContractError(e);
      toast('error', c.message, c.detail);
      await refresh();
    } finally {
      busy = false;
    }
  }

  async function signIn() {
    waiting = true;
    await run(() => api.starorgLogin(channel), `Signed in to ${HOSTS[env]}`);
    waiting = false;
  }

  async function cancelSignIn() {
    try {
      await api.starorgCancelLogin();
    } catch {
      /* nothing waiting */
    }
  }

  async function syncNow() {
    syncing = true;
    try {
      const r = await api.starorgSync(channel);
      const parts = r.reports.map((x) => (x.direction === 'pull' ? `pulled ${x.added.length} new` : `pushed ${x.sent}`));
      if (r.fleet) parts.push(`hangar ${r.fleet.hulls.length} models`);
      toast(r.skipped.length ? 'warning' : 'success', `Synced with ${HOSTS[env]}: ${parts.join(', ') || 'nothing to do'}`, r.skipped.join('\n') || null);
      await refresh();
    } catch (e) {
      const c = toContractError(e);
      toast('error', c.message, c.detail);
    } finally {
      syncing = false;
    }
  }

  async function tick(id: string, on: boolean) {
    if (!st) return;
    const ticked = st.sync_characters.filter((c) => (c.id === id ? on : c.ticked)).map((c) => c.id);
    try {
      st = await api.setSyncCharacters({ channel, ticked });
    } catch (e) {
      toast('error', 'Not saved', toContractError(e).message);
    }
  }

  async function pickGuild(id: string) {
    try {
      st = await api.setStarorgGuild({ channel, guild: id });
    } catch (e) {
      toast('error', 'Not saved', toContractError(e).message);
    }
  }

  const anyNetwork = $derived(!!so && (so.push_blueprints || so.pull_blueprints || so.pull_fleet));
  const syncBlocked = $derived(!enabled ? 'Turn on “Connect StarOrg Tools” first.' : !st?.connected ? 'Sign in first.' : !anyNetwork ? 'Turn on what to sync above.' : null);
</script>

<Card.Root id="starorg">
  <Card.Header>
    <Card.Title tabindex={-1}>StarOrg Tools</Card.Title>
    <Card.Description>Link your StarOrg Tools account to pull and push owned blueprints and read your hangar. Sign-in happens in your browser; the token is kept on this computer only (where is shown below once you are signed in).</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    {#if !st || !so}
      <Skeleton class="h-16 w-full" />
    {:else}
      {#if !enabled}
        <div class="bg-muted/40 flex items-start gap-2 rounded-md border p-3" role="status">
          <CloudOff class="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <p>Off. Turn on “Connect StarOrg Tools” under Network above to sign in. With it off, nothing is sent to or read from {HOSTS[env]}.</p>
        </div>
      {/if}

      <div class="flex flex-wrap items-center gap-3">
        <span class="w-24">Environment</span>
        <Segmented
          ariaLabel="StarOrg Tools environment"
          disabled={busy || waiting}
          options={[
            { id: 'production', label: 'Production', hint: 'starorg.tools' },
            { id: 'dev', label: 'Dev', hint: 'dev.starorg.tools (test server)' },
          ] as { id: StarOrgEnv; label: string; hint: string }[]}
          value={env}
          onchange={pickEnv}
        />
        <span class="text-muted-foreground mono text-[12px]">{st.host}</span>
      </div>

      <div class="flex flex-wrap items-center gap-2" aria-live="polite">
        {#if st.connected}
          <ToneBadge tone="success" label="Signed in" />
          <span>{st.user}{#if st.guild}&nbsp;· {st.guild}{/if}</span>
          <span class="text-muted-foreground text-[12px]">until {fmtDate(st.expires)} · {st.token_store === 'file' ? 'token in a private file' : 'token in the system keyring'}</span>
          <span class="flex-1"></span>
          <Button variant="outline" size="sm" disabled={busy} onclick={() => run(() => api.starorgLogout(channel), `Signed out of ${st?.host}`)}><LogOut aria-hidden="true" /> Sign out</Button>
        {:else if waiting}
          <ToneBadge tone="info" label="Waiting for the browser" />
          <span class="text-muted-foreground text-[12px]">Finish the sign-in in your browser, then come back here.</span>
          <span class="flex-1"></span>
          <Button variant="outline" size="sm" onclick={cancelSignIn}>Cancel</Button>
        {:else}
          <ToneBadge tone={st.expired ? 'warning' : 'muted'} label={st.expired ? 'Signed out: sign in again' : 'Not signed in'} />
          <span class="flex-1"></span>
          <Button size="sm" disabled={!enabled || busy} onclick={signIn}><LogIn aria-hidden="true" /> Sign in with Discord</Button>
        {/if}
      </div>

      {#if st.connected && st.guilds.length}
        <div class="flex flex-wrap items-center gap-3">
          <Label for="starorg-guild" class="w-24 font-normal">Org</Label>
          <SelectField id="starorg-guild" ariaLabel="Org to sync with" options={st.guilds.map((g) => ({ id: g.id, label: g.blueprints ? g.name : `${g.name} (no blueprint tracking)` }))} value={st.guild_id} onchange={pickGuild} />
        </div>
      {/if}

      <fieldset class="grid gap-0.5 rounded-md border p-3">
        <legend class="px-1 font-medium">What to sync</legend>
        <SwitchRow id="sync-gamelog" label="Import blueprints from Game.log" hint="Reads new “Received Blueprint” lines from the game's logs on this computer (here, in the CLI and in the post-exit hook). Off: the logs are not read at all. Nothing is sent." checked={so.import_game_log} onchange={(v) => setSync('import_game_log', v)} />
        <SwitchRow id="sync-push" label="Push owned blueprints to StarOrg Tools" hint="Sends blueprint ids, and when each was received, of the ticked characters to {st.host}. Never character names or ids; never removals." checked={so.push_blueprints} disabled={!enabled} onchange={(v) => setSync('push_blueprints', v)} />
        <SwitchRow id="sync-pull" label="Pull owned blueprints from StarOrg Tools" hint="Reads your owned list from {st.host} into one character here (adds only)." checked={so.pull_blueprints} disabled={!enabled} onchange={(v) => setSync('pull_blueprints', v)} />
        <SwitchRow id="sync-fleet" label="Pull fleet/hangar" hint="Reads your hangar from {st.host} into Ships (adds to the fleet)." checked={so.pull_fleet} disabled={!enabled} onchange={(v) => setSync('pull_fleet', v)} />
        <SwitchRow id="sync-auto" label="Sync automatically after game exit" hint="Runs the switches above when the game closes (while the app runs, or after the post-exit hook). Off: only when you press Sync now." checked={so.auto_sync} disabled={!enabled || !anyNetwork} onchange={(v) => setSync('auto_sync', v)} />
      </fieldset>

      <fieldset class="grid gap-1.5 rounded-md border p-3" disabled={!enabled || !so.push_blueprints}>
        <legend class="px-1 font-medium">Characters to push</legend>
        <p class="text-muted-foreground text-[12px]">StarOrg keeps one owned list per account. Push sends the blueprints owned by any ticked character; un-ticking one never removes anything on StarOrg.</p>
        <ul class="grid gap-1 sm:grid-cols-3">
          {#each st.sync_characters as c (c.id)}
            <li class="flex items-center gap-2">
              <Checkbox id="sync-{c.id}" checked={c.ticked} disabled={!enabled || !so.push_blueprints} onCheckedChange={(v) => tick(c.id, v === true)} />
              <Label for="sync-{c.id}" class="font-normal">{c.name} <span class="text-muted-foreground mono text-[11.5px]">{c.owned}</span></Label>
            </li>
          {/each}
        </ul>
        <p class="text-[12px]" aria-live="polite">A push on {channel} sends <span class="mono font-semibold">{st.push_count}</span> blueprints.</p>
      </fieldset>

      <div class="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="outline" disabled={!!syncBlocked || syncing} onclick={syncNow}>
          {#if syncing}<LoaderCircle class="animate-spin" aria-hidden="true" />Syncing…{:else}<RefreshCw aria-hidden="true" /> Sync now{/if}
        </Button>
        <span class="text-muted-foreground text-[12px]">{syncBlocked ?? `Last sync ${st.last_sync ? fmtDateTime(st.last_sync) : 'never'}`}</span>
      </div>
    {/if}
  </Card.Content>
</Card.Root>

<ActionDialog
  bind:open={confirmOpen}
  title="Switch to {confirmEnv ? HOSTS[confirmEnv] : ''}?"
  description="Sign-ins are per environment: you will be signed out of {HOSTS[env]} (the token is removed here, and revoked there while Connect StarOrg Tools is on and the host answers). Your owned blueprints on this computer stay."
  confirmLabel="Switch and sign out"
  busyLabel="Switching…"
  onconfirm={switchEnv}
/>
