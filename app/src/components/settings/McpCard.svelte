<script lang="ts">
  /**
   * Settings -> Experimental (docs/mcp.md): the local MCP server switch. Off at every start;
   * when on, shows the loopback URL, this run's token and ready-to-paste client configs.
   */
  import Copy from '@lucide/svelte/icons/copy';
  import Eye from '@lucide/svelte/icons/eye';
  import EyeOff from '@lucide/svelte/icons/eye-off';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { McpStatus } from '@/lib/contract/types';
  import { toast } from '@/lib/stores/toast.svelte';
  import MemoryWarningDialog from '@/components/settings/MemoryWarningDialog.svelte';
  import { reminderText } from '@/lib/memory';
  import { ensureMemory, memory } from '@/lib/stores/memory.svelte';

  /** Turning the server on asks first: its extraction workers can use several GB (7.18). */
  let warning = $state(false);
  $effect(() => ensureMemory());

  let st = $state<McpStatus | null>(null);
  let busy = $state(false);
  let reveal = $state(false);

  $effect(() => {
    let stale = false;
    api
      .mcpStatus()
      .then((s) => !stale && (st = s))
      .catch((e) => !stale && toast('error', 'Could not read the MCP server state', toContractError(e).message));
    return () => (stale = true);
  });

  async function toggle(on: boolean) {
    busy = true;
    try {
      st = await api.setMcpEnabled(on);
      if (on && !st.running) toast('error', 'Local MCP server not started', st.error ?? 'Unknown reason');
      else toast('success', on ? 'Local MCP server on' : 'Local MCP server off', on ? 'Listening on this computer only.' : null);
    } catch (e) {
      toast('error', 'Local MCP server not changed', toContractError(e).message);
    } finally {
      busy = false;
    }
  }

  async function copy(text: string | null, what: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      toast('success', `Copied ${what}`, what.includes('token') || what.includes('Claude') ? 'It holds this run’s token: paste it only into your own MCP client.' : null);
    } catch {
      toast('error', 'Copy failed', 'The clipboard is not available.');
    }
  }

  const masked = (t: string) => `${t.slice(0, 6)}${'•'.repeat(10)}${t.slice(-4)}`;
</script>

<Card.Root id="experimental">
  <Card.Header>
    <Card.Title tabindex={-1} class="focus-visible:ring-ring/60 rounded-sm outline-none focus-visible:ring-2">Experimental</Card.Title>
    <Card.Description>Early features. They may change or go away.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    {#if !st}
      <Skeleton class="h-12 w-full" />
    {:else}
      <SwitchRow
        id="mcp-server"
        label="Local MCP server"
        hint="Lets an AI assistant on this computer (Claude Code, Claude Desktop) ask read-only questions about your game data: items, where to get them, missions, strings, changes between channels. Listens on 127.0.0.1 only, needs a token made fresh at each start, and stops when you switch it off or close the app. Your owned blueprints and character names go only to that local client; nothing is sent anywhere."
        checked={st.running || warning}
        disabled={busy}
        onchange={(on) => (on ? (warning = true) : void toggle(false))}
      />
      <MemoryWarningDialog bind:open={warning} what="mcp" onconfirm={() => toggle(true)} />
      <p class="text-muted-foreground -mt-1 pl-11 text-[12px]" data-memory-reminder>Reading a dataset the first time: {reminderText(memory.status)}</p>
      {#if st.error && !st.running}
        <p class="text-destructive text-[12px]">Not started: {st.error}</p>
      {/if}
      {#if st.running && st.url && st.token}
        <div class="grid gap-2 rounded-md border px-3 py-2.5" aria-label="MCP server connection">
          <div class="flex flex-wrap items-center gap-2">
            <ToneBadge tone="success" label="Running" />
            <span class="mono min-w-0 flex-1 truncate text-[12px]" title={st.url}>{st.url}</span>
            <Button variant="ghost" size="xs" onclick={() => copy(st!.url, 'the URL')}><Copy aria-hidden="true" /> URL</Button>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-muted-foreground w-12 text-[12px]">Token</span>
            <span class="mono min-w-0 flex-1 truncate text-[12px]">{reveal ? st.token : masked(st.token)}</span>
            <Button variant="ghost" size="xs" aria-pressed={reveal} onclick={() => (reveal = !reveal)}>
              {#if reveal}<EyeOff aria-hidden="true" /> Hide{:else}<Eye aria-hidden="true" /> Show{/if}
            </Button>
            <Button variant="ghost" size="xs" onclick={() => copy(st!.token, 'the token')}><Copy aria-hidden="true" /> Token</Button>
          </div>
          <div class="flex flex-wrap gap-2 pt-1">
            <Button size="sm" variant="outline" onclick={() => copy(st!.claude_code, 'the Claude Code command')}><Copy aria-hidden="true" /> Copy for Claude Code</Button>
            <Button size="sm" variant="outline" onclick={() => copy(st!.claude_desktop, 'the Claude Desktop config')}><Copy aria-hidden="true" /> Copy for Claude Desktop</Button>
          </div>
          <p class="text-muted-foreground text-[12px]">
            Channels: {st.channels.length ? st.channels.join(', ') : 'none found'}. The token changes every time the server starts; copy the config again after a restart. Claude Desktop reaches the server through the <span class="mono">mcp-remote</span> bridge, which its <span class="mono">npx</span> downloads. The first question about a build reads the game files once (seconds to a minute). Requests show in Activity.
          </p>
        </div>
      {/if}
      <div class="flex flex-wrap items-center gap-2 text-[12px]">
        <span class="text-muted-foreground">Without the app, over stdio (no token):</span>
        <span class="mono">{st.stdio}</span>
        <Button variant="ghost" size="xs" onclick={() => copy(st!.stdio, 'the command')}><Copy aria-hidden="true" /> Copy</Button>
      </div>
    {/if}
  </Card.Content>
</Card.Root>
