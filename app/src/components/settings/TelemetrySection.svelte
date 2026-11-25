<script lang="ts">
  /**
   * Settings -> Experimental -> Live telemetry (docs/user-guide.md#live-telemetry): off by
   * default. Offered only for a channel whose build is 5.0 or newer (each channel says why
   * not). One choice: Off, Metrics (read only) or Metrics + actions. Metrics writes one line to
   * that channel's USER.cfg, actions a second; the launcher needs two environment variables, which the app never
   * sets: the steps are shown with a copy button. The app reads the game's own service on
   * 127.0.0.1 only. "Record sessions" is a switch of its own (off): it saves streams to the
   * app's data folder for replay, and turning Metrics on never turns it on. Notifications
   * (alerts) are a separate choice below (TelemetryAlertsSection).
   *
   * "More from Live telemetry" groups the other switches, each off by default and each its own
   * choice: "Record sessions", "Read Game.log events" (the channel's own Game.log on this
   * computer, for the Events and Sessions panels) and, under it, "People" (the handles Game.log
   * names, memory only) and "Shard history" (the shards joined, in a file in the app's data
   * folder, with the channel's logbackups read in: the one that writes, so its own switch).
   * None changes USER.cfg or sends anything.
   */
  import Copy from '@lucide/svelte/icons/copy';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import Segmented from '@/components/common/Segmented.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import PathText from '@/components/common/PathText.svelte';
  import TelemetryAlertsSection from '@/components/settings/TelemetryAlertsSection.svelte';
  import { toContractError } from '@/lib/contract/client';
  import type { CvarState, TelemetryCfgLine, TelemetryChannel, TelemetryConfigureResult, TelemetrySettings } from '@/lib/contract/telemetry';
  import { telemetry } from '@/lib/stores/telemetry.svelte';
  import { GAMELOG_LOCAL, TELEMETRY_LOCAL, telemetryTarget } from '@/lib/telemetry';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  $effect(() => {
    if (!telemetry.status) void telemetry.load();
  });

  const st = $derived(telemetry.status);
  /** Channels of several installs: each row names its install. */
  const multi = $derived(new Set(st?.channels.map((c) => c.install) ?? []).size > 1);
  const installName = (id: string) => app.overview?.installs.find((i) => i.id === id)?.found_by_label ?? id;
  const offered = $derived(st?.channels.filter((c) => c.offered) ?? []);
  const key = (c: { install: string; channel: string }) => `${c.install}/${c.channel}`;
  /**
   * The channel whose USER.cfg gets the lines: the one picked (kept in the store while the app
   * runs), the streaming one, the one whose USER.cfg has the line, the app's channel, else the
   * first offered.
   */
  const target = $derived<TelemetryChannel | null>(telemetryTarget(offered, telemetry.picked, st?.streaming ?? null, app.targetKey));

  /** Another channel picked while on: its USER.cfg gets the lines now, the old one's go. */
  function pick(v: string) {
    telemetry.picked = v;
    if (st?.settings.enabled) void save({});
  }
  let port = $state('');
  const savedPort = $derived(st?.settings.port ?? null);
  $effect(() => {
    if (savedPort !== null) port = String(savedPort);
  });
  let error = $state<string | null>(null);
  /** USER.cfg has another value for a line; the user may replace it. */
  let decision = $state<{ line: string; value: string } | null>(null);

  type Mode = 'off' | 'metrics' | 'actions';
  const MODES: { id: Mode; label: string; hint: string }[] = [
    { id: 'off', label: 'Off', hint: 'Nothing is read and USER.cfg is left as it was.' },
    { id: 'metrics', label: 'Metrics', hint: 'Read only: health, ship, fuel and shields while you play.' },
    { id: 'actions', label: 'Metrics + actions', hint: 'Also buttons that press game inputs for you (lights, landing gear, shields…), only those the game itself lists.' },
  ];
  const mode = $derived<Mode>(!st?.settings.enabled ? 'off' : st.settings.triggers ? 'actions' : 'metrics');
  function setMode(m: Mode) {
    if (m === mode) return;
    void save({ enabled: m !== 'off', triggers: m === 'actions' });
  }

  const ref = (c: TelemetryChannel) => ({ install: c.install, channel: c.channel });

  async function save(next: Partial<TelemetrySettings>, replaceOther = false) {
    if (!st) return;
    const settings: TelemetrySettings = { ...st.settings, ...next };
    error = null;
    try {
      const r: TelemetryConfigureResult = await telemetry.configure({ target: target ? ref(target) : null, settings, replace_other: replaceOther });
      const ask = [r.telemetry_line, r.triggers_line].find((l) => l?.outcome === 'needs-decision');
      decision = ask ? { line: ask.line, value: ask.value ?? '' } : null;
      for (const w of r.warnings) toast('warning', 'USER.cfg not changed', w);
      if (next.enabled === true && !st.settings.enabled && !ask) toast('success', 'Live telemetry on', `Reads 127.0.0.1:${settings.port} only. Open it from the sidebar under Game.`);
      if (next.enabled === false) toast('success', 'Live telemetry off', 'Its USER.cfg lines were taken out again.');
    } catch (e) {
      error = toContractError(e).message;
    }
  }

  function savePort() {
    if (!st) return;
    const n = Number(port);
    if (!Number.isInteger(n) || n < 1024 || n > 65535) {
      error = 'The port is a number from 1024 to 65535.';
      return;
    }
    if (n !== st.settings.port) void save({ port: n });
  }

  async function setRecord(on: boolean) {
    error = null;
    try {
      await telemetry.setRecord(on);
      toast('success', on ? 'Record sessions on' : 'Record sessions off', on ? 'Each stream is saved on this computer from its first frame; replay it on the Live telemetry screen.' : 'A recording in progress was closed.');
    } catch (e) {
      error = toContractError(e).message;
    }
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast('success', 'Copied the launcher steps');
    } catch {
      toast('error', 'Copy failed', 'The clipboard is not available.');
    }
  }

  const lineState: Record<CvarState, { tone: 'success' | 'muted' | 'warning'; label: string }> = {
    set: { tone: 'success', label: 'In USER.cfg' },
    missing: { tone: 'muted', label: 'Not there' },
    'no-file': { tone: 'muted', label: 'No USER.cfg yet' },
    other: { tone: 'warning', label: 'Another value' },
  };
  const lineBadge = (l: TelemetryCfgLine) => (l.state === 'other' ? { ...lineState.other, label: `Set to ${l.value ?? '?'}` } : lineState[l.state]);
</script>

<div class="grid gap-2 border-t pt-3" id="live-telemetry">
  {#if !st}
    <Skeleton class="h-12 w-full" />
  {:else}
    <div class="grid gap-1.5" id="telemetry-on">
      <div class="flex flex-wrap items-center gap-3">
        <span class="text-[13px] font-medium">Live telemetry</span>
        <Segmented ariaLabel="Live telemetry" size="sm" options={MODES} value={mode} disabled={telemetry.busy || (mode === 'off' && !target)} onchange={setMode} />
      </div>
      <p class="text-[12px]" role="note">{TELEMETRY_LOCAL}</p>
      <p class="text-muted-foreground text-[12px]">
        Shows your health, ship, fuel and shields while you play, read from Star Citizen's own local service on this computer (127.0.0.1). For game builds 5.0 and newer only (PTU today, LIVE once it reaches 5.0).
      </p>
      <p class="text-muted-foreground text-[12px]">
        Each part is its own choice and off by default; turning one on never turns on another: <strong class="font-medium">Metrics</strong> or <strong class="font-medium">Metrics + actions</strong> here, then <strong class="font-medium">Record sessions</strong>, <strong class="font-medium">Read Game.log events</strong> (with People and Shard history) and <strong class="font-medium">Notifications</strong> below.
      </p>
    </div>
    <div class="grid gap-2 pl-0">
      <ul class="grid gap-1 text-[12px]" aria-label="Channels for live telemetry">
        {#each st.channels as c (key(c))}
          <li class="flex flex-wrap items-center gap-2">
            <span class="w-16 font-medium">{c.channel}</span>
            {#if multi}<span class="text-muted-foreground w-40 truncate" title={c.install}>{installName(c.install)}</span>{/if}
            <span class="mono text-muted-foreground w-14">{c.game_version ?? '?'}</span>
            {#if c.offered}
              <ToneBadge tone="success" label="Offered" />
              {#if !c.exe_found}<span class="text-warning">No Bin64/StarCitizen.exe to read the schema from.</span>{/if}
            {:else}
              <ToneBadge tone="muted" label="Not offered" />
              <span class="text-muted-foreground">{c.reason}</span>
            {/if}
          </li>
        {/each}
      </ul>
      {#if offered.length === 0}
        <p class="text-muted-foreground text-[12px]">None of your channels runs 5.0 or newer yet. When one does (LIVE once it reaches 5.0), you can turn this on for it.</p>
      {/if}
      {#if target && (st.settings.enabled || offered.length > 1)}
        <div class="flex flex-wrap items-center gap-2 text-[12px]">
          <span id="tele-ch">Channel</span>
          <SelectField ariaLabel="Channel whose USER.cfg gets the line" options={offered.map((c) => ({ id: key(c), label: c.channel }))} value={key(target)} onchange={pick} />
          <label for="tele-port" class="ml-2">Port</label>
          <Input
            id="tele-port"
            class="h-7 w-24 font-mono text-[12px]"
            inputmode="numeric"
            bind:value={port}
            onblur={savePort}
            onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && savePort()}
            disabled={telemetry.busy}
          />
          <span class="text-muted-foreground">The game listens on 127.0.0.1:{st.settings.port}</span>
        </div>
        <div class="grid gap-1 rounded-md border px-3 py-2 text-[12px]" aria-label="USER.cfg change">
          <div class="text-muted-foreground">What it changes in <PathText path={target.user_cfg} /></div>
          {#each [target.telemetry_line, target.triggers_line] as l, i (l.line)}
            {@const b = lineBadge(l)}
            <div class="flex flex-wrap items-center gap-2">
              <span class="mono">{l.line}</span>
              <ToneBadge tone={b.tone} label={b.label} />
              <span class="text-muted-foreground">{i === 0 ? 'while Live telemetry is on' : 'only with Metrics + actions'}</span>
            </div>
          {/each}
          <p class="text-muted-foreground">Taken out again when you switch off (unless you changed it since). Other lines stay as they are.</p>
        </div>
      {/if}
      {#if decision}
        <div class="border-warning/40 grid gap-2 rounded-md border px-3 py-2 text-[12px]" role="alert">
          <p>USER.cfg already has <span class="mono">{decision.line.split('=')[0]}= {decision.value}</span>. Replace it with <span class="mono">{decision.line}</span>? Your line comes back when you switch off.</p>
          <div class="flex gap-2">
            <Button size="xs" onclick={() => void save({}, true)}>Replace</Button>
            <Button size="xs" variant="ghost" onclick={() => (decision = null)}>Leave it</Button>
          </div>
        </div>
      {/if}
      {#if st.settings.enabled}
        <div class="grid gap-1.5 rounded-md border px-3 py-2" role="group" aria-labelledby="tele-extras">
          <div class="grid gap-0.5">
            <span id="tele-extras" class="text-[12.5px] font-medium">More from Live telemetry</span>
            <span class="text-muted-foreground text-[12px]">Each off until you turn it on. All of it stays on this computer.</span>
          </div>
          <SwitchRow
            id="tele-record"
            size="sm"
            label="Record sessions"
            hint="Saves each live stream (4 frames a second) to the app's data folder so you can replay it on the Live telemetry screen. 20 recordings and 256 MB at most, the oldest go first; Settings → Storage shows them. Never sent anywhere."
            checked={st.settings.record}
            disabled={telemetry.busy}
            onchange={(v) => void setRecord(v)}
          />
          <SwitchRow
            id="tele-gamelog"
            size="sm"
            label="Read Game.log events"
            hint={`While the Live telemetry screen is open, follows this channel's Game.log: where you are, the shard you are on, quantum travel, ships nearby, deaths and respawns. Read only, nothing is written. ${GAMELOG_LOCAL}`}
            checked={st.settings.gamelog}
            disabled={telemetry.busy}
            onchange={(v) => void save({ gamelog: v, people: v ? st.settings.people : false, sessions: v ? st.settings.sessions : false })}
          />
          <SwitchRow
            id="tele-people"
            size="sm"
            class="pl-6"
            label="People"
            hint={`Also lists the player handles Game.log names (crew, party, players in ship kills near you). Kept in memory only, never written to a file, the Activity log or a diagnostics report, and never looked up online. ${GAMELOG_LOCAL}`}
            checked={st.settings.gamelog && st.settings.people}
            disabled={telemetry.busy || !st.settings.gamelog}
            onchange={(v) => void save({ people: v })}
          />
          <SwitchRow
            id="tele-sessions"
            size="sm"
            class="pl-6"
            label="Shard history"
            hint={`Keeps a list of the shards you join (region, times, why you left; never a server address, session id or player name) in the app's data folder, and reads in the older logs in this channel's logbackups folder (the newest 40). 2000 rows at most; Settings → Storage shows it and Clear empties it. ${GAMELOG_LOCAL}`}
            checked={st.settings.gamelog && st.settings.sessions}
            disabled={telemetry.busy || !st.settings.gamelog}
            onchange={(v) => void save({ sessions: v })}
          />
        </div>
        {#if st.settings.triggers}
          <p class="text-muted-foreground text-[12px]">Each session asks before the first press; self destruct, eject and power off ask every time.</p>
          <p class="text-warning flex items-start gap-1.5 text-[12px]"><TriangleAlert class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" /> A press acts in your game at once, as if you pressed the key. Use it only while you can see the game.</p>
        {/if}
        <div class="grid gap-1.5">
          <div class="flex flex-wrap items-center gap-2 text-[12px]">
            <span class="font-medium">Launcher setup ({st.help_os === 'windows' ? 'Windows' : 'Linux'})</span>
            <span class="text-muted-foreground">The game also needs two settings from the RSI Launcher, which the app does not change for you.</span>
            <Button variant="ghost" size="xs" onclick={() => copy(st.env_help)}><Copy aria-hidden="true" /> Copy steps</Button>
          </div>
          <pre class="mono bg-background/60 max-h-72 overflow-auto rounded-md border p-3 text-[12px] leading-relaxed whitespace-pre-wrap">{st.env_help}</pre>
        </div>
      {/if}
      {#if error}<p class="text-destructive text-[12px]" role="alert">{error}</p>{/if}
      <p class="text-muted-foreground text-[12px]">Reads only the game's local service, and only while this is on. StarOrg Codex still collects nothing and sends nothing anywhere.</p>
    </div>
    {#if offered.length > 0}<TelemetryAlertsSection enabled={st.settings.enabled} />{/if}
  {/if}
</div>
