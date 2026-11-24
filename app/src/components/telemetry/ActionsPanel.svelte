<script lang="ts">
  /**
   * Live telemetry -> Actions: the inputs the game lists as triggerable, by category, with a
   * Press (and for holdable ones a Hold) button. Disabled unless Settings > Experimental >
   * "Allow game actions" is on. The first press of each app session asks first; the dangerous
   * actions (self destruct, eject, power off...) ask every time. The current state next to an
   * action comes from the frame (lights, gear, shields).
   */
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Search from '@lucide/svelte/icons/search';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import ActionDialog from '@/components/common/ActionDialog.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { telemetryApi, type TelemetryActions, type TelemetryJson } from '@/lib/contract/telemetry';
  import { toContractError } from '@/lib/contract/client';
  import { actionState, groupActions, humanName, needsConfirm, triggerText } from '@/lib/telemetry';
  import { telemetry } from '@/lib/stores/telemetry.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  let { raw, connected }: { raw: { [k: string]: TelemetryJson } | null; connected: boolean } = $props();

  let list = $state<TelemetryActions | null>(null);
  let loading = $state(false);
  let query = $state('');
  let holdMs = $state(1000);
  let result = $state<{ ok: boolean; text: string; action: string } | null>(null);
  let pending = $state<{ action: string; hold: number | null; dangerous: boolean } | null>(null);
  let confirmOpen = $state(false);

  const triggersOn = $derived(!!telemetry.status?.settings.enabled && !!telemetry.status?.settings.triggers);
  const dangerous = $derived(telemetry.status?.dangerous ?? []);

  async function load() {
    loading = true;
    try {
      list = await telemetryApi.actions();
    } catch (e) {
      result = { ok: false, text: toContractError(e).message, action: '' };
    } finally {
      loading = false;
    }
  }

  // Again when the switch changes or the game connects (the list is empty until you spawn).
  $effect(() => {
    void triggersOn;
    void connected;
    void load();
  });

  const groups = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const acts = (list?.actions ?? []).filter((a) => !q || a.action_name.includes(q) || a.category.includes(q) || humanName(a.action_name).toLowerCase().includes(q));
    return groupActions(acts);
  });
  const count = $derived(list?.actions.length ?? 0);

  async function send(action: string, hold: number | null, confirmed: boolean) {
    const r = await telemetryApi.trigger({ action, hold_ms: hold, confirmed });
    if (r.refused?.reason === 'needs_confirm') {
      pending = { action, hold, dangerous: true };
      confirmOpen = true;
      return;
    }
    result = { ...triggerText(r), action };
  }

  function press(action: string, hold: number | null) {
    if (needsConfirm(action, dangerous, telemetry.confirmed)) {
      pending = { action, hold, dangerous: dangerous.includes(action) };
      confirmOpen = true;
      return;
    }
    void send(action, hold, false).catch((e) => (result = { ok: false, text: toContractError(e).message, action }));
  }

  async function confirmed() {
    const p = pending;
    if (!p) return;
    if (!p.dangerous) telemetry.confirmed = true;
    await send(p.action, p.hold, true);
  }
</script>

<Card.Root class="lg:col-span-2" aria-labelledby="tele-actions">
  <Card.Header>
    <Card.Title id="tele-actions" class="flex flex-wrap items-center gap-2">
      Actions
      <ToneBadge tone={triggersOn ? 'warning' : 'muted'} label={triggersOn ? 'Game actions allowed' : 'Read only'} />
      <span class="text-muted-foreground text-[12px] font-normal">{count} listed by the game{list?.source === 'mock' ? ' (synthetic)' : ''}</span>
    </Card.Title>
    <Card.Description>
      {#if triggersOn}
        A press acts in your game at once. The first one of this session asks first; dangerous ones always do.
      {:else}
        Pressing is off. Turn on "Allow game actions" under <button type="button" class="text-primary underline-offset-2 hover:underline" onclick={() => ((app.settingsSection = 'experimental'), app.go('settings'))}>Settings → Experimental</button> to use these buttons.
      {/if}
    </Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative min-w-48 flex-1">
        <Search class="text-muted-foreground absolute top-1/2 left-2 size-3.5 -translate-y-1/2" aria-hidden="true" />
        <Input class="h-8 pl-7 text-[13px]" placeholder="Filter actions" aria-label="Filter actions" bind:value={query} />
      </div>
      <label class="text-muted-foreground flex items-center gap-1.5 text-[12px]">Hold for
        <Input type="number" min={100} max={10000} step={100} class="h-8 w-20 font-mono text-[12px]" bind:value={holdMs} /> ms
      </label>
      <Button variant="ghost" size="sm" disabled={loading} onclick={load}><RefreshCw class={cn(loading && 'animate-spin')} aria-hidden="true" /> Refresh</Button>
    </div>
    {#if list?.error}<p class="text-warning text-[12px]">The game did not answer: {list.error}</p>{/if}
    {#if count === 0}
      <p class="text-muted-foreground text-[12px]">{list?.source === 'none' ? 'Start reading the game to see its actions.' : 'The game lists its actions once you have spawned (and only with its trigger setting on).'}</p>
    {:else}
      <div class="grid max-h-[28rem] gap-3 overflow-y-auto pr-1">
        {#each groups as g (g.category)}
          <section aria-label={humanName(g.category)}>
            <h3 class="eyebrow text-muted-foreground mb-1">{humanName(g.category)}</h3>
            <ul class="grid gap-1 sm:grid-cols-2">
              {#each g.actions as a (a.action_name)}
                {@const state = raw ? actionState(raw, a.action_name) : null}
                {@const danger = dangerous.includes(a.action_name)}
                <li class="flex items-center gap-2 rounded-md border px-2 py-1">
                  <div class="min-w-0 flex-1">
                    <div class={cn('truncate text-[13px]', danger && 'text-destructive')} title={a.action_name}>{humanName(a.action_name)}</div>
                    <div class="mono text-muted-foreground truncate text-[11px]">{a.action_name}</div>
                  </div>
                  {#if state}<span class="mono text-[11px]" aria-label="Now {state}">{state}</span>{/if}
                  <Button size="xs" variant={danger ? 'destructive' : 'outline'} disabled={!triggersOn} onclick={() => press(a.action_name, null)}>Press</Button>
                  {#if a.holdable}
                    <Button size="xs" variant="outline" disabled={!triggersOn} onclick={() => press(a.action_name, Math.max(1, Math.min(10000, Math.round(holdMs) || 1000)))}>Hold</Button>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}
    <p class="text-[12px]" aria-live="polite">
      {#if result}<span class={result.ok ? 'text-success' : 'text-warning'}>{result.action ? `${humanName(result.action)}: ` : ''}{result.text}</span>{/if}
    </p>
  </Card.Content>
</Card.Root>

<ActionDialog
  bind:open={confirmOpen}
  title={pending?.dangerous ? `${humanName(pending.action)}?` : 'Press game inputs from the app?'}
  description={pending?.dangerous
    ? `This presses ${pending.action} in your game now. It cannot be undone there.`
    : `This presses ${pending?.action ?? 'the action'} in your game now, as if you pressed the key. Further presses this session go straight through.`}
  confirmLabel={pending?.hold ? `Hold ${pending.hold} ms` : 'Press'}
  busyLabel="Pressing…"
  destructive={!!pending?.dangerous}
  onconfirm={confirmed}
/>
