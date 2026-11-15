<script lang="ts">
  /** LUG Helper launch hooks (addendum §2, Linux only): status per script, install, update, remove. */
  import Plug from '@lucide/svelte/icons/plug';
  import Unplug from '@lucide/svelte/icons/unplug';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { HookState, HooksStatus } from '@/lib/contract/types';
  import type { StatusTone } from '@/lib/format';
  import { toast } from '@/lib/stores/toast.svelte';

  let hs = $state<HooksStatus | null>(null);
  let busy = $state(false);

  $effect(() => {
    let stale = false;
    api
      .hooksStatus()
      .then((h) => !stale && (hs = h))
      .catch((e) => !stale && toast('error', 'Could not read the launch hooks', toContractError(e).message));
    return () => (stale = true);
  });

  const view = (s: HookState): { tone: StatusTone; label: string; hint: string } =>
    ({
      absent: { tone: 'muted', label: 'Not installed', hint: 'The script exists; our block is not in it.' },
      current: { tone: 'success', label: 'Installed', hint: 'Our block is in the script and up to date.' },
      outdated: { tone: 'warning', label: 'Outdated', hint: 'Our block is from an older version. Update it.' },
      broken: { tone: 'destructive', label: 'Broken', hint: s.state === 'broken' ? s.why : '' },
      missing: { tone: 'muted', label: 'No script', hint: 'The LUG Helper script does not exist yet; installing creates it.' },
    })[s.state] as { tone: StatusTone; label: string; hint: string };

  const installed = $derived(hs?.scripts.some((s) => s.status.state === 'current' || s.status.state === 'outdated') ?? false);
  const needsUpdate = $derived(hs?.scripts.some((s) => s.status.state !== 'current') ?? true);

  async function run(f: () => Promise<HooksStatus>, ok: string) {
    busy = true;
    try {
      hs = await f();
      toast('success', ok, 'A backup of each script was kept.');
    } catch (e) {
      toast('error', 'Launch hooks not changed', toContractError(e).message);
    } finally {
      busy = false;
    }
  }
</script>

<Card.Root id="hooks">
  <Card.Header>
    <Card.Title tabindex={-1}>Launch hooks</Card.Title>
    <Card.Description>Re-apply after game updates before launch, and pick up new blueprints after you quit. Adds a marked block to the LUG Helper's sc-prelaunch.sh and sc-postexit.sh; the rest of each script is left alone.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    {#if !hs}
      <Skeleton class="h-16 w-full" />
    {:else if !hs.supported}
      <p class="text-muted-foreground">Launch hooks are for the LUG Helper on Linux. On Windows, apply from Home after a game update, or schedule <span class="mono">starorg-codex apply --all --if-stale</span>.</p>
    {:else}
      <ul class="grid gap-2">
        {#each hs.scripts as s (s.kind)}
          {@const v = view(s.status)}
          <li class="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border px-3 py-2">
            <span class="w-24 font-medium">{s.kind === 'prelaunch' ? 'Before launch' : 'After exit'}</span>
            <ToneBadge tone={v.tone} label={v.label} hint={v.hint} />
            <span class="mono text-muted-foreground min-w-0 flex-1 truncate text-[12px]" title={s.path ?? ''}>{s.path ?? 'will be created next to the LUG Helper launch script'}{s.is_link ? ' (link)' : ''}</span>
          </li>
        {/each}
      </ul>
      {#if !hs.cli_on_path}<p class="text-warning text-[12px]">The starorg-codex command is not on PATH; the hooks call it by full path.</p>{/if}
      <div class="flex flex-wrap gap-2">
        <Button size="sm" disabled={busy || !needsUpdate} onclick={() => run(() => api.hooksInstall(), installed ? 'Launch hooks updated' : 'Launch hooks installed')}><Plug aria-hidden="true" /> {installed ? 'Update hooks' : 'Install hooks'}</Button>
        {#if installed}
          <ConfirmButton variant="outline" size="sm" confirmLabel="Remove? Click again" onconfirm={() => run(() => api.hooksUninstall(), 'Launch hooks removed')}><Unplug aria-hidden="true" /> Remove hooks</ConfirmButton>
        {/if}
      </div>
    {/if}
  </Card.Content>
</Card.Root>
