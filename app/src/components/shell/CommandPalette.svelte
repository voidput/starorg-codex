<script lang="ts">
  /**
   * Ctrl/Cmd+K command palette (spec 9.3): go to a screen, switch channel, apply, re-run setup.
   * Apply is listed only when writing is allowed (not while the game runs or a job runs).
   * With Developer mode on (addendum 7.8): "Find record…" searches the explorer for the typed
   * text, "Open record by GUID" opens a pasted GUID.
   */
  import * as Dialog from '@/components/ui/dialog';
  import { Input } from '@/components/ui/input';
  import { navItems, type ScreenId } from '@/lib/nav';
  import { app } from '@/lib/stores/app.svelte';
  import { dev } from '@/lib/stores/dev.svelte';
  import { telemetry } from '@/lib/stores/telemetry.svelte';
  import { isGuidLike } from '@/lib/devnav';
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
  const devOn = $derived(!!app.settings?.advanced.developer_mode);
  // The typed text is captured now: running a command clears the input first.
  const devCmds = $derived.by<Cmd[]>(() => {
    const typed = q.trim();
    return devOn
      ? [
          { id: 'dev-find', label: typed ? `Find record “${typed}”` : 'Find record…', hint: 'Explorer', run: () => dev.findRecord(typed, () => go('explorer')) },
          ...(isGuidLike(typed) ? [{ id: 'dev-guid', label: `Open record ${typed}`, hint: 'Explorer', run: () => dev.openRecord(typed, () => go('explorer')) }] : [{ id: 'dev-guid', label: 'Open record by GUID', hint: 'Paste a GUID', run: () => dev.findRecord('', () => go('explorer')) }]),
        ]
      : [];
  });
  const cmds = $derived<Cmd[]>([
    ...navItems(devOn, telemetry.enabled).map((n) => ({ id: `go-${n.id}`, label: `Go to ${n.label}`, hint: n.key ? `Alt+${n.key}` : (n.badge ?? 'Developer'), run: () => go(n.id) })),
    ...app.channels.map((c) => ({ id: `ch-${c.channel}`, label: `Switch to ${c.channel}`, hint: 'Channel', run: () => app.selectChannel(c.channel) })),
    ...(app.channel?.set_up && !app.blocked ? [{ id: 'apply', label: `Apply to ${app.channel.channel}`, hint: 'Action', run: () => app.requestApply() }] : []),
    { id: 'setup', label: 'Run setup again', hint: 'Action', run: () => (app.wizard = true) },
  ]);
  // The dev commands take the typed text as their argument, so they always show.
  const shown = $derived([...cmds.filter((c) => c.label.toLowerCase().includes(q.trim().toLowerCase())), ...devCmds]);
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
