<script lang="ts">
  /**
   * StarOrg Tools account link (addendum §5): sign in in the system browser, sign out, and the
   * characters to sync. Push sends the union of the ticked characters' owned sets (Q22 = b).
   */
  import LogIn from '@lucide/svelte/icons/log-in';
  import LogOut from '@lucide/svelte/icons/log-out';
  import CloudOff from '@lucide/svelte/icons/cloud-off';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import { Label } from '@/components/ui/label';
  import { Skeleton } from '@/components/ui/skeleton';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { StarOrgStatus } from '@/lib/contract/types';
  import { fmtDate } from '@/lib/format';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  let st = $state<StarOrgStatus | null>(null);
  let busy = $state(false);
  const channel = $derived(app.channel?.channel ?? 'LIVE');
  const enabled = $derived(app.settings?.network.starorg ?? false);

  $effect(() => {
    const c = channel;
    void enabled;
    let stale = false;
    api
      .starorgStatus(c)
      .then((s) => !stale && (st = s))
      .catch((e) => !stale && toast('error', 'Could not read the StarOrg status', toContractError(e).message));
    return () => (stale = true);
  });

  async function run(f: () => Promise<StarOrgStatus>, ok: string) {
    busy = true;
    try {
      st = await f();
      toast('success', ok, null);
    } catch (e) {
      const c = toContractError(e);
      toast('error', c.message, c.detail);
    } finally {
      busy = false;
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
</script>

<Card.Root id="starorg">
  <Card.Header>
    <Card.Title tabindex={-1}>StarOrg Tools</Card.Title>
    <Card.Description>Link your starorg.tools account to pull and push owned blueprints. Sign-in happens in your browser; the token is kept in the system keyring.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    {#if !st}
      <Skeleton class="h-16 w-full" />
    {:else}
      {#if !enabled}
        <div class="bg-muted/40 flex items-start gap-2 rounded-md border p-3" role="status">
          <CloudOff class="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <p>Off. Turn on “Connect StarOrg Tools” under Network above to sign in. With it off, nothing is sent to or read from starorg.tools.</p>
        </div>
      {/if}
      <div class="flex flex-wrap items-center gap-2">
        {#if st.connected}
          <ToneBadge tone="success" label="Signed in" />
          <span>{st.user} · {st.guild}</span>
          <span class="text-muted-foreground text-[12px]">until {fmtDate(st.expires)} · {st.environment === 'dev' ? 'dev.starorg.tools' : 'starorg.tools'}</span>
          <span class="flex-1"></span>
          <Button variant="outline" size="sm" disabled={busy} onclick={() => run(() => api.starorgLogout(channel), 'Signed out of StarOrg Tools')}><LogOut aria-hidden="true" /> Sign out</Button>
        {:else}
          <ToneBadge tone="muted" label="Not signed in" />
          <span class="flex-1"></span>
          <Button size="sm" disabled={!enabled || busy} onclick={() => run(() => api.starorgLogin(channel), 'Signed in to StarOrg Tools')}><LogIn aria-hidden="true" /> Sign in with Discord</Button>
        {/if}
      </div>
      <fieldset class="grid gap-1.5 rounded-md border p-3" disabled={!enabled}>
        <legend class="px-1 font-medium">Characters to sync</legend>
        <p class="text-muted-foreground text-[12px]">StarOrg keeps one owned list per account. Push sends the blueprints owned by any ticked character; un-ticking one never removes anything on StarOrg.</p>
        <ul class="grid gap-1 sm:grid-cols-3">
          {#each st.sync_characters as c (c.id)}
            <li class="flex items-center gap-2">
              <Checkbox id="sync-{c.id}" checked={c.ticked} disabled={!enabled} onCheckedChange={(v) => tick(c.id, v === true)} />
              <Label for="sync-{c.id}" class="font-normal">{c.name} <span class="text-muted-foreground mono text-[11.5px]">{c.owned}</span></Label>
            </li>
          {/each}
        </ul>
        <p class="text-[12px]" aria-live="polite">A push on {channel} sends <span class="mono font-semibold">{st.push_count}</span> blueprints.</p>
      </fieldset>
    {/if}
  </Card.Content>
</Card.Root>
