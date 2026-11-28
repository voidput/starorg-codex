<script lang="ts">
  /**
   * Live telemetry (experimental; docs/user-guide.md#live-telemetry): the game's own live data,
   * read from its local service on 127.0.0.1 while this screen is open. Panels after
   * sc-telemetry's dashboard (same owner, MIT) in this app's style, top to bottom:
   * - the connection bar (or the replay bar), the key readouts, pilot, ship, shields, systems;
   * - Alerts and Actions, then the History graphs;
   * - Game.log: Events and People, then Sessions (the shard join history); with "Read Game.log
   *   events" off, one card says so with a link to Settings (Sessions stays while it has rows);
   * - Recordings and the raw frame.
   * Every panel below the live readouts folds to its title (CollapsibleCard, remembered). Each
   * of alerts, actions, recordings and Game.log is its own switch, off by default; an off panel
   * says how to turn it on.
   *
   * Replay: a recording plays into the same panels in place of the game (a "Replay" badge and
   * its controls replace the connection and Start/Stop); "Back to live" reads the game again.
   *
   * Lazy: the stream starts when the screen opens and stops when it closes. Frames come at most
   * 10 a second into one raw state; each panel reads only its values from it, so nothing heavier
   * than a few text nodes changes per frame. With the mock backend the frames are synthetic.
   */
  import { onDestroy, onMount, untrack } from 'svelte';
  import Gauge from '@lucide/svelte/icons/gauge';
  import Play from '@lucide/svelte/icons/play';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import Square from '@lucide/svelte/icons/square';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import CollapsibleCard from '@/components/common/CollapsibleCard.svelte';
  import ActionsPanel from '@/components/telemetry/ActionsPanel.svelte';
  import AlertsPanel from '@/components/telemetry/AlertsPanel.svelte';
  import EventsPanel from '@/components/telemetry/EventsPanel.svelte';
  import HistoryPanel from '@/components/telemetry/HistoryPanel.svelte';
  import PeoplePanel from '@/components/telemetry/PeoplePanel.svelte';
  import SessionsPanel from '@/components/telemetry/SessionsPanel.svelte';
  import RawFramePanel from '@/components/telemetry/RawFramePanel.svelte';
  import RecordingsPanel from '@/components/telemetry/RecordingsPanel.svelte';
  import ReplayBar from '@/components/telemetry/ReplayBar.svelte';
  import type { TelemetryChannel } from '@/lib/contract/telemetry';
  import { annunciators, barWidth, faceLabels, fixed, linkView, pct, systems, telemetryTarget, GAMELOG_LOCAL, TELEMETRY_LOCAL } from '@/lib/telemetry';
  import { telemetry } from '@/lib/stores/telemetry.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  const st = $derived(telemetry.status);
  const offered = $derived(st?.channels.filter((c) => c.offered) ?? []);
  const key = (c: { install: string; channel: string }) => `${c.install}/${c.channel}`;
  const target = $derived<TelemetryChannel | null>(telemetryTarget(offered, telemetry.picked, st?.streaming ?? null, app.targetKey));
  const targetKey = $derived(target ? key(target) : '');
  const running = $derived(!!st?.streaming);
  const view = $derived(linkView(telemetry.link, running));
  const replay = $derived(telemetry.replay ?? st?.replay ?? null);

  const enabled = $derived(!!st?.settings.enabled);
  const gamelogOn = $derived(enabled && !!st?.settings.gamelog);
  /** "Shard history": rows are kept and logbackups read in (its own switch under Game.log events). */
  const sessionsOn = $derived(gamelogOn && !!st?.settings.sessions);
  /** Rows of the join history kept from earlier (its panel shows with Game.log off too). */
  const savedSessions = $derived(Array.isArray(telemetry.sessions?.items) ? telemetry.sessions.items.length : 0);
  onMount(() => void telemetry.loadSessions());
  const openSettings = () => ((app.settingsSection = 'experimental'), app.go('settings'));

  // Start for the chosen channel when the screen opens or the channel changes; keyed on
  // primitives only, so a Stop (a new status) does not start it again.
  $effect(() => {
    const k = targetKey;
    if (!k || !enabled) return;
    untrack(() => {
      const t = offered.find((c) => key(c) === k);
      if (t && (!st?.streaming || key(st.streaming) !== k)) void telemetry.start({ install: t.install, channel: t.channel });
    });
  });
  onDestroy(() => void telemetry.stop());

  const frame = $derived(telemetry.frame);
  const snap = $derived(frame?.snapshot ?? null);
  const ship = $derived(snap?.ship ?? null);
  const sys = $derived(frame ? systems(frame.raw) : []);
  const warn = $derived(frame && snap ? annunciators(snap, frame.raw) : []);
  const faces = $derived(ship?.shield_faces ?? []);
  const labels = $derived(faceLabels(faces.length));

  function toggle() {
    if (running) {
      // Keep the channel that was streaming: without the pin, Stop would fall back to
      // another channel and the effect above would start that one.
      if (targetKey) telemetry.picked = targetKey;
      void telemetry.stop();
    } else if (target) void telemetry.start({ install: target.install, channel: target.channel });
  }

  async function backToLive() {
    await telemetry.stop();
    if (target) void telemetry.start({ install: target.install, channel: target.channel });
  }
</script>

{#snippet bar(label: string, value: number | null, text: string, low = 0.25)}
  <div class="grid gap-1">
    <div class="flex items-baseline justify-between gap-2 text-[12px]">
      <span class="text-muted-foreground">{label}</span>
      <span class="mono">{text}</span>
    </div>
    <div class="bg-muted h-1.5 overflow-hidden rounded-full" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={value === null ? undefined : Math.round(value * 100)}>
      <div class={cn('h-full rounded-full transition-[width] duration-100', value !== null && value < low ? 'bg-destructive' : 'bg-primary')} style="width: {barWidth(value)}"></div>
    </div>
  </div>
{/snippet}

{#snippet readout(label: string, text: string)}
  <div class="bg-card rounded-lg border px-3 py-2">
    <div class="text-muted-foreground text-[11.5px]">{label}</div>
    <div class="mono text-[16px] font-semibold">{text}</div>
  </div>
{/snippet}

{#if !st?.settings.enabled}
  <FullEmpty icon={Gauge} title="Live telemetry is off">
    Turn it on under Settings → Experimental. It reads the game's own local service on this computer, for 5.0 builds and newer.
    {#snippet actions()}<Button onclick={openSettings}>Open Settings</Button>{/snippet}
  </FullEmpty>
{:else if offered.length === 0}
  <FullEmpty icon={Gauge} title="No 5.0 channel">None of your channels runs game version 5.0 or newer yet. Live telemetry is offered for those only.</FullEmpty>
{:else}
  <div class="grid gap-3">
    <div class="flex flex-wrap items-center gap-2">
      <ToneBadge tone="warning" label="Experimental" />
      {#if replay}
        <ToneBadge tone="info" label="Replay" hint="A recording plays instead of the game" />
      {:else}
        <ToneBadge tone={view.tone} label={view.label} />
      {/if}
      {#if offered.length > 1 && target}
        <SelectField ariaLabel="Channel" options={offered.map((c) => ({ id: key(c), label: c.channel }))} value={targetKey} onchange={(v) => (telemetry.picked = v)} />
      {:else if target}
        <span class="font-medium">{target.channel}</span>
      {/if}
      <span class="mono text-muted-foreground text-[12px]">{st.endpoint}</span>
      {#if st.mock}<span class="text-muted-foreground text-[12px]">synthetic frames (mock)</span>{/if}
      <span class="flex-1"></span>
      {#if !replay}
        <Button size="sm" variant="outline" onclick={toggle} disabled={!target || telemetry.starting}>
          {#if running}<Square aria-hidden="true" /> Stop{:else if telemetry.starting}<Play aria-hidden="true" /> Starting…{:else}<Play aria-hidden="true" /> Start{/if}
        </Button>
      {/if}
    </div>
    <p class="text-[12px]" role="note">{TELEMETRY_LOCAL}</p>
    {#if replay}
      <ReplayBar {replay} onlive={() => void backToLive()} />
      <p class="text-muted-foreground text-[12px]">Replaying a recording from this computer: the game is not read meanwhile, and actions are off.</p>
    {:else}
      <p class="text-muted-foreground text-[12px]" aria-live="polite">
        {#if telemetry.starting && !running}Reading the game's schema (the first start of a build takes a few seconds)…{:else}{view.detail}{/if}
        {#if st.schema}Schema {st.schema.key} ({st.schema.origin === 'cache' ? 'cached' : st.schema.origin === 'extracted' ? 'read from StarCitizen.exe just now' : 'synthetic'}).{/if}
        {#if st.recording}Recording this session.{/if}
      </p>
    {/if}

    {#if !frame}
      <Card.Root>
        <Card.Content class="text-muted-foreground py-6 text-center text-sm">
          {replay ? 'Loading the recording…' : telemetry.link.state === 'connected' ? 'Connected. The game sends frames once you have spawned.' : 'No frames yet.'}
        </Card.Content>
      </Card.Root>
    {:else}
      <section class="grid grid-cols-2 gap-2 sm:grid-cols-4 lg:grid-cols-6" aria-label="Key readouts">
        {@render readout('Speed', fixed(ship?.speed, 0, 'm/s'))}
        {@render readout('Vertical', fixed(ship?.vsi, 1, 'm/s'))}
        {@render readout('G', fixed(ship?.g, 2))}
        {@render readout('Mode', ship?.master_mode ?? '—')}
        {@render readout('Gear', ship?.gear ?? '—')}
        {@render readout('Frame', String(frame.seq))}
      </section>

      <div class="grid gap-3 lg:grid-cols-2">
        <Card.Root aria-labelledby="tele-pilot">
          <Card.Header><Card.Title id="tele-pilot">Pilot</Card.Title></Card.Header>
          <Card.Content class="grid gap-2.5">
            {@render bar('Health', snap!.vitals.health === null ? null : snap!.vitals.health / 100, snap!.vitals.health === null ? '—' : `${Math.round(snap!.vitals.health)}%`, 0.3)}
            {@render bar('Oxygen', snap!.vitals.oxygen, pct(snap!.vitals.oxygen), 0.2)}
            <div class="grid grid-cols-3 gap-2 text-[12px]">
              <div><div class="text-muted-foreground">Heart rate</div><div class="mono">{fixed(snap!.vitals.heart_rate, 0, 'bpm')}</div></div>
              <div><div class="text-muted-foreground">Body temp</div><div class="mono">{fixed(snap!.vitals.body_temperature, 1, '°C')}</div></div>
              <div><div class="text-muted-foreground">Gravity</div><div class="mono">{fixed(snap!.vitals.gravity, 2, 'G')}</div></div>
            </div>
          </Card.Content>
        </Card.Root>

        <Card.Root aria-labelledby="tele-ship">
          <Card.Header>
            <Card.Title id="tele-ship" class="flex flex-wrap items-center gap-2">Ship {#if ship?.name}<span class="text-muted-foreground text-[13px] font-normal">{ship.name}</span>{/if}</Card.Title>
            {#if ship?.class_name}<Card.Description class="mono">{ship.class_name}</Card.Description>{/if}
          </Card.Header>
          <Card.Content class="grid gap-2.5">
            {#if ship}
              {@render bar('Hull', ship.hull, pct(ship.hull), 0.3)}
              {@render bar('Hydrogen fuel', ship.hydrogen_fuel, pct(ship.hydrogen_fuel), 0.15)}
              {@render bar('Quantum fuel', ship.quantum_fuel, pct(ship.quantum_fuel), 0.15)}
              {@render bar('Boost', ship.boost, pct(ship.boost), 0.1)}
            {:else}
              <p class="text-muted-foreground text-[12px]">Not in a vehicle.</p>
            {/if}
          </Card.Content>
        </Card.Root>

        <Card.Root aria-labelledby="tele-shields">
          <Card.Header><Card.Title id="tele-shields">Shields</Card.Title></Card.Header>
          <Card.Content class="grid gap-2.5">
            {#if ship}
              {@render bar('Level', ship.shield, pct(ship.shield), 0.1)}
              {#if faces.length}
                <div class="grid gap-1.5" style="grid-template-columns: repeat({Math.min(faces.length, 4)}, minmax(0, 1fr))" aria-label="Shield faces">
                  {#each faces as v, i (i)}
                    {@render bar(labels[i] ?? `F${i + 1}`, v, pct(v), 0.1)}
                  {/each}
                </div>
                <p class="text-muted-foreground text-[11.5px]">Face order is not confirmed for every ship yet: F1, F2… as the game lists them.</p>
              {/if}
            {:else}
              <p class="text-muted-foreground text-[12px]">Not in a vehicle.</p>
            {/if}
          </Card.Content>
        </Card.Root>

        <Card.Root aria-labelledby="tele-systems">
          <Card.Header><Card.Title id="tele-systems">Systems</Card.Title></Card.Header>
          <Card.Content class="grid gap-3">
            <ul class="grid grid-cols-2 gap-1.5 text-[12px]">
              {#each sys as s (s.id)}
                <li class="flex items-center justify-between gap-2 rounded-md border px-2 py-1">
                  <span>{s.label}</span>
                  <span class={cn('mono', s.on === true ? 'text-success' : s.on === false ? 'text-muted-foreground' : 'text-muted-foreground/60')}>{s.text ?? (s.on === null ? '—' : s.on ? 'ON' : 'OFF')}</span>
                </li>
              {/each}
            </ul>
            <div class="grid gap-1">
              <h3 class="eyebrow text-muted-foreground">Annunciators</h3>
              {#if warn.length}
                <div class="flex flex-wrap gap-1.5" role="status">
                  {#each warn as w (w.id)}<ToneBadge tone={w.tone} label={w.label} />{/each}
                </div>
              {:else}
                <p class="text-muted-foreground text-[12px]">All clear.</p>
              {/if}
            </div>
          </Card.Content>
        </Card.Root>
      </div>
    {/if}

    <div class="grid gap-3 lg:grid-cols-2">
      <CollapsibleCard key="telemetry.alerts" bare><AlertsPanel /></CollapsibleCard>
      <CollapsibleCard key="telemetry.actions" bare><ActionsPanel raw={frame?.raw ?? null} connected={!replay && telemetry.link.state === 'connected'} /></CollapsibleCard>
    </div>
    <CollapsibleCard key="telemetry.history" bare><HistoryPanel /></CollapsibleCard>

    {#if gamelogOn}
      <div class={cn('grid gap-3', st.settings.people && 'lg:grid-cols-[3fr_2fr]')}>
        <CollapsibleCard key="telemetry.events" bare><EventsPanel view={telemetry.gamelog} /></CollapsibleCard>
        {#if st.settings.people}<CollapsibleCard key="telemetry.people" bare><PeoplePanel people={telemetry.gamelog?.people ?? null} onclear={() => void telemetry.clearPeople()} /></CollapsibleCard>{/if}
      </div>
    {:else}
      <Card.Root aria-labelledby="tele-gamelog-off">
        <Card.Header>
          <Card.Title id="tele-gamelog-off" class="flex items-center gap-2"><ScrollText class="size-4" aria-hidden="true" /> Game.log events are off</Card.Title>
          <Card.Description>
            Turn on Read Game.log events for where you are, the shard you are on, quantum travel, deaths and respawns, and a history of the shards you join. It reads this channel's Game.log: {GAMELOG_LOCAL}
          </Card.Description>
          <Card.Action><Button size="xs" variant="outline" onclick={openSettings}>Settings</Button></Card.Action>
        </Card.Header>
      </Card.Root>
    {/if}
    {#if gamelogOn || savedSessions > 0}
      <CollapsibleCard key="telemetry.sessions" bare><SessionsPanel current={telemetry.gamelog?.stint ?? null} rev={telemetry.gamelog?.sessions_rev ?? null} reading={sessionsOn} /></CollapsibleCard>
    {/if}

    <div class="grid gap-3 lg:grid-cols-2">
      <CollapsibleCard key="telemetry.recordings" bare><RecordingsPanel /></CollapsibleCard>
      <RawFramePanel {frame} />
    </div>
  </div>
{/if}
