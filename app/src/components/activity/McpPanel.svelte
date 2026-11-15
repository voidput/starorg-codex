<script lang="ts">
  /** Activity: the local MCP server's state and its recent requests (docs/mcp.md). Hidden while it is off and has answered nothing. */
  import * as Card from '@/components/ui/card';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { api } from '@/lib/contract/client';
  import type { McpStatus } from '@/lib/contract/types';
  import { fmtDateTime } from '@/lib/format';

  let { tick = 0 }: { tick?: number } = $props();
  let st = $state<McpStatus | null>(null);

  $effect(() => {
    void tick;
    let stale = false;
    api
      .mcpStatus()
      .then((s) => !stale && (st = s))
      .catch(() => !stale && (st = null));
    return () => (stale = true);
  });

  const shown = $derived((Array.isArray(st?.requests) ? st!.requests : []).slice(0, 20));
  const what = (r: McpStatus['requests'][number]) => r.tool ?? r.method ?? (r.status === 401 ? 'refused: no or wrong token' : 'request');
</script>

{#if st && (st.running || shown.length)}
  <Card.Root aria-label="Local MCP server">
    <Card.Header class="flex flex-row flex-wrap items-center gap-2">
      <Card.Title class="text-[13px]">Local MCP server</Card.Title>
      <ToneBadge tone={st.running ? 'success' : 'muted'} label={st.running ? 'Running' : 'Off'} />
      {#if st.url}<span class="mono text-muted-foreground text-[12px]">{st.url}</span>{/if}
      <span class="text-muted-foreground ml-auto text-[12px]">{st.requests.length} request{st.requests.length === 1 ? '' : 's'}{st.started ? ` since ${fmtDateTime(st.started)}` : ''}</span>
    </Card.Header>
    {#if shown.length}
      <Card.Content>
        <ol class="divide-y text-[12px]" aria-label="MCP requests, newest first">
          {#each shown as r, i (i)}
            <li class="grid grid-cols-[176px_minmax(0,1fr)_60px_64px] items-center gap-3 py-1">
              <time class="mono text-muted-foreground" datetime={r.time}>{fmtDateTime(r.time)}</time>
              <span class="mono truncate">{what(r)}</span>
              <span class={r.ok ? 'text-muted-foreground' : 'text-destructive'}>{r.status}{r.ok ? '' : ' ✕'}</span>
              <span class="text-muted-foreground text-right">{r.millis} ms</span>
            </li>
          {/each}
        </ol>
      </Card.Content>
    {/if}
  </Card.Root>
{/if}
