<script lang="ts">
  /**
   * Shown before Developer mode or the local MCP server is turned on (addendum 7.17): they
   * start worker processes that can use several GB of RAM. Measured worst cases, this
   * computer's RAM and free memory, and the limit each worker is held to.
   */
  import ActionDialog from '@/components/common/ActionDialog.svelte';
  import { enforcementText, gbText, limitText, machineText } from '@/lib/memory';
  import { loadMemory, memory } from '@/lib/stores/memory.svelte';

  let {
    open = $bindable(false),
    what,
    onconfirm,
  }: {
    open: boolean;
    what: 'dev' | 'mcp';
    onconfirm: () => Promise<void>;
  } = $props();

  $effect(() => {
    if (open) void loadMemory();
  });

  const s = $derived(memory.status);
  const lowFree = $derived(!!s && s.available_bytes !== null && s.available_bytes < s.limit_gb * 1024 ** 3);
  const rows = $derived(
    what === 'dev'
      ? [
          ['Explorer, Files and sounds', '0.5-1 GB while open'],
          ['Model export, Print shell', '1.4-1.8 GB'],
          ['Model export, Full (textures)', '6-7 GB; Idris about 11 GB'],
        ]
      : [
          ['Most datasets (item-tags 1.2 GB, armor 0.9 GB)', 'up to 1.5 GB'],
          ['Ship components', 'about 2.2 GB'],
          ['Ships', 'about 4.1 GB'],
        ],
  );
</script>

<ActionDialog
  bind:open
  title={what === 'dev' ? 'Turn on Developer mode?' : 'Start the local MCP server?'}
  description={what === 'dev'
    ? 'Developer mode reads the game data in separate worker processes that can use several GB of RAM while they work.'
    : 'The MCP server reads each dataset once per build in a short-lived worker process that can use several GB of RAM.'}
  confirmLabel={what === 'dev' ? 'Turn on' : 'Start server'}
  busyLabel="Starting…"
  {onconfirm}
>
  <div class="grid gap-2 text-[12.5px]" data-memory-warning>
    <p class="text-muted-foreground">Measured peaks on LIVE 4.10 (one worker at a time; the memory goes back when it ends):</p>
    <table class="w-full border-collapse text-left">
      <tbody>
        {#each rows as [label, size] (label)}
          <tr class="border-b last:border-0">
            <td class="py-1 pr-3">{label}</td>
            <td class="mono py-1 text-right whitespace-nowrap">{size}</td>
          </tr>
        {/each}
        <tr>
          <td class="text-muted-foreground py-1 pr-3">Every worker, at least (game archive index)</td>
          <td class="mono text-muted-foreground py-1 text-right whitespace-nowrap">0.5-0.9 GB</td>
        </tr>
      </tbody>
    </table>
    {#if s}
      <p>
        This computer: <span class="font-medium">{machineText(s)}</span>. Each worker is held to
        <span class="font-medium">{limitText(s.limit_gb)}</span>{s.is_default ? ' (the default)' : ''}; a job that needs more is refused or stopped with a message. The app and the game keep running.
      </p>
      {#if lowFree}
        <p class="text-warning" role="status">Less than {limitText(s.limit_gb)} is free right now{s.available_bytes !== null ? ` (${gbText(s.available_bytes)})` : ''}. If the game is running, close it before big exports.</p>
      {/if}
      <p class="text-muted-foreground text-[12px]">{enforcementText(s.enforcement)} Change the limit under Settings > Advanced > Memory limit.</p>
    {:else}
      <p class="text-muted-foreground">Reading this computer's memory…</p>
    {/if}
  </div>
</ActionDialog>
