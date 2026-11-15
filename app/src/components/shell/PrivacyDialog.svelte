<script lang="ts">
  /**
   * Footer -> Privacy (docs/privacy.md): what leaves the computer, from the live settings. Nothing
   * by default; each network feature is listed with its current state and what it would send.
   */
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { api } from '@/lib/contract/client';
  import type { McpStatus } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';
  import { updates } from '@/lib/stores/update.svelte';
  import { storeManaged } from '@/lib/update';

  let { open = $bindable(false) }: { open: boolean } = $props();
  let mcp = $state<McpStatus | null>(null);

  $effect(() => {
    if (!open) return;
    let stale = false;
    api
      .mcpStatus()
      .then((s) => !stale && (mcp = s))
      .catch(() => !stale && (mcp = null));
    return () => (stale = true);
  });

  const net = $derived(app.settings?.network ?? null);
  const store = $derived(storeManaged(updates.status));
  const rows = $derived([
    store
      ? { name: 'Updates', on: false, state: 'Microsoft Store', what: 'The Store installs updates. The app never contacts GitHub.' }
      : {
          name: 'Check for updates',
          on: !!net?.check_updates,
          state: net?.check_updates ? 'On' : 'Off',
          what: 'Asks api.github.com for the release list (at most once a day, or when you press Check now). Sends only the app name and version. Installing an update downloads it from github.com.',
        },
    {
      name: 'StarOrg Tools',
      on: !!net?.starorg,
      state: net?.starorg ? 'On' : 'Off',
      what: 'Only after you sign in at starorg.tools in your browser: pulls and pushes the owned blueprints you choose. Nothing is sent before you sign in.',
    },
    {
      name: 'Local MCP server',
      on: !!mcp?.running,
      state: mcp?.running ? 'Running' : 'Off',
      what: 'Answers AI assistants on this computer only (127.0.0.1, token required). Makes no network request itself.',
    },
  ]);

  function settings() {
    open = false;
    app.settingsSection = 'network';
    app.go('settings');
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[85vh] overflow-y-auto sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title class="flex items-center gap-2"><ShieldCheck class="text-success size-5" aria-hidden="true" /> Privacy</Dialog.Title>
      <Dialog.Description>StarOrg Codex has no telemetry, analytics, crash upload or account of its own.</Dialog.Description>
    </Dialog.Header>
    <div class="grid gap-3 text-[13px]">
      <p>
        Everything happens on this computer: it reads your game files and writes the game's text file, one line in
        <span class="mono">USER.cfg</span>, and its own settings, backups and cache. With the default settings the app makes
        <strong>no network request</strong>.
      </p>
      <p class="text-muted-foreground">Only these features talk to anything, and each is off until you turn it on:</p>
      <ul class="grid gap-2">
        {#each rows as r (r.name)}
          <li class="rounded-md border p-2.5">
            <div class="flex items-center justify-between gap-2">
              <span class="font-medium">{r.name}</span>
              <ToneBadge tone={r.on ? 'info' : 'muted'} label={r.state} />
            </div>
            <p class="text-muted-foreground mt-1 text-[12.5px]">{r.what}</p>
          </li>
        {/each}
      </ul>
      <p class="text-muted-foreground text-[12.5px]">
        Like any web request, the GitHub and starorg.tools ones show your IP address to that server. Diagnostics exports
        are files you save and share yourself, with your user name and paths replaced. Full policy:
        <span class="mono">docs/privacy.md</span> in the GitHub repository.
      </p>
    </div>
    <Dialog.Footer>
      <Button variant="outline" onclick={settings}>Network settings</Button>
      <Button onclick={() => (open = false)}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
