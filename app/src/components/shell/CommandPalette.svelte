<script lang="ts">
  /** Ctrl/Cmd+K command palette (spec 9.3): go to a screen, switch channel, apply, re-run setup. */
  import * as Dialog from '@/components/ui/dialog';
  import { Input } from '@/components/ui/input';
  import { NAV_ITEMS, type ScreenId } from '@/lib/nav';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  let { open = $bindable(false), go }: { open: boolean; go: (s: ScreenId) => void } = $props();
  let q = $state('');
  let sel = $state(0);

  interface Cmd {
    id: string;
    label: string;
    hint: string;
    run: () => void;
  }
  const cmds = $derived<Cmd[]>([
    ...NAV_ITEMS.map((n) => ({ id: `go-${n.id}`, label: `Go to ${n.label}`, hint: `Alt+${n.key}`, run: () => go(n.id) })),
    ...app.channels.map((c) => ({ id: `ch-${c.channel}`, label: `Switch to ${c.channel}`, hint: 'Channel', run: () => app.selectChannel(c.channel) })),
    ...(app.channel?.set_up ? [{ id: 'apply', label: `Apply to ${app.channel.channel}`, hint: 'Action', run: () => void app.apply() }] : []),
    { id: 'setup', label: 'Run setup again', hint: 'Action', run: () => (app.wizard = true) },
  ]);
  const shown = $derived(cmds.filter((c) => c.label.toLowerCase().includes(q.trim().toLowerCase())));
  $effect(() => {
    void q;
    sel = 0;
  });

  function run(c: Cmd | undefined) {
    if (!c) return;
    open = false;
    q = '';
    c.run();
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') sel = Math.min(shown.length - 1, sel + 1);
    else if (e.key === 'ArrowUp') sel = Math.max(0, sel - 1);
    else if (e.key === 'Enter') run(shown[sel]);
    else return;
    e.preventDefault();
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="top-[20%] translate-y-0 gap-2 p-2 sm:max-w-md" showCloseButton={false}>
    <Dialog.Title class="sr-only">Command palette</Dialog.Title>
    <Input
      bind:value={q}
      placeholder="Type a command or screen"
      aria-label="Command"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-activedescendant={shown[sel] ? `pal-${shown[sel]!.id}` : undefined}
      onkeydown={key}
    />
    <ul id="palette-list" role="listbox" class="max-h-72 overflow-y-auto">
      {#each shown as c, i (c.id)}
        <li
          id="pal-{c.id}"
          role="option"
          aria-selected={i === sel}
          class={cn('flex h-8 cursor-pointer items-center justify-between rounded-md px-2', i === sel && 'bg-accent')}
          onpointerenter={() => (sel = i)}
          onclick={() => run(c)}
          onkeydown={() => {}}
        >
          <span>{c.label}</span><span class="text-muted-foreground mono text-[11px]">{c.hint}</span>
        </li>
      {:else}
        <li class="text-muted-foreground px-2 py-4 text-center">No command matches.</li>
      {/each}
    </ul>
  </Dialog.Content>
</Dialog.Root>
