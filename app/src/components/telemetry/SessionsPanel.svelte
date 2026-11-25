<script lang="ts">
  /**
   * Live telemetry -> Sessions: the server and shard join history from Game.log ("Shard
   * history" under "Read Game.log events", each off by default), one row per shard stint, newest
   * first, with the stint you are on now on top (or the last one, once it disconnected). Filters by channel and region; marks a drop and a rejoin of the shard you were
   * on before. The file lives in the app's data folder (Settings > Storage counts it); it holds
   * no server address, port, session id or handle. The list is read again only when its
   * revision moves, at most once a second (a backfill moves it many times), never per frame.
   */
  import Eraser from '@lucide/svelte/icons/eraser';
  import Repeat from '@lucide/svelte/icons/repeat';
  import Server from '@lucide/svelte/icons/server';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import * as Card from '@/components/ui/card';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import type { TelemetryStint } from '@/lib/contract/telemetry';
  import { GAMELOG_LOCAL, LEFT_REASON, clock, durationText, regionOf, sessionFilters, sessionRows, whenText } from '@/lib/telemetry';
  import { telemetry } from '@/lib/stores/telemetry.svelte';

  let { current, rev, reading }: { current: TelemetryStint | null; rev: number | null; reading: boolean } = $props();

  /** Rows drawn at most (the file keeps more; a filter narrows them). */
  const SHOWN = 200;

  // On mount (unless the screen's read is held or on its way), and again when the history's
  // revision moves; a primitive, so no reload loop.
  $effect(() => {
    telemetry.sessionsChanged(rev);
  });

  let channel = $state('all');
  let region = $state('all');
  const list = $derived(telemetry.sessions);
  const filters = $derived(sessionFilters(list?.items));
  const rows = $derived(sessionRows(list?.items, channel, region));
  const shown = $derived(rows.slice(0, SHOWN));
  const total = $derived(Array.isArray(list?.items) ? list.items.length : 0);
  const opt = (all: string, xs: string[]) => [{ id: 'all', label: all }, ...xs.map((x) => ({ id: x, label: x }))];
</script>

<Card.Root aria-labelledby="tele-sessions">
  <Card.Header>
    <Card.Title id="tele-sessions" class="flex items-center gap-2">
      <Server class="size-4" aria-hidden="true" /> Sessions
      {#if total}<span class="text-muted-foreground text-[13px] font-normal">{total}</span>{/if}
    </Card.Title>
    <Card.Description>
      Shards you joined (the shard history), from Game.log and its backups. {GAMELOG_LOCAL} No server address, session id or player name is kept.
      {#if list?.backfill_pending}Reading {list.backfill_pending} older logs…{/if}
    </Card.Description>
    <Card.Action>
      <ConfirmButton variant="ghost" confirmLabel="Clear all?" disabled={total === 0} aria-label="Clear the session history" onconfirm={() => void telemetry.clearSessions()}><Eraser aria-hidden="true" /> Clear</ConfirmButton>
    </Card.Action>
  </Card.Header>
  <Card.Content class="grid gap-3 text-[12px]">
    {#if current}
      <!-- A disconnect that nothing followed yet (the game closed): the last shard, not the current one. -->
      {@const gone = current.left_reason ? LEFT_REASON[current.left_reason] : null}
      <div class="flex flex-wrap items-center gap-2 rounded-md border px-2 py-1.5" aria-label={gone ? 'Last shard' : 'Current shard'}>
        {#if gone}<ToneBadge tone={gone.tone} label={gone.label} hint={gone.hint} />{:else}<ToneBadge tone="success" label="Now" />{/if}
        <span class="mono">{current.shard_short ?? current.shard}</span>
        {#if current.region}<span class="text-muted-foreground">{current.region}</span>{/if}
        {#if gone && current.left_ms !== null}<span class="text-muted-foreground">{clock(current.joined_ms)} to {clock(current.left_ms)}</span>{:else}<span class="text-muted-foreground">since {clock(current.joined_ms)}</span>{/if}
        {#if current.systems.length}<span>{current.systems.join(' → ')}</span>{/if}
        {#if current.disconnects}<span class="text-warning">{current.disconnects} disconnect{current.disconnects > 1 ? 's' : ''}</span>{/if}
      </div>
    {/if}
    {#if list?.error}<p class="text-warning" role="alert">{list.error}</p>{/if}
    {#if total === 0}
      <p class="text-muted-foreground">
        {#if reading}No shard joins yet. A row is added when you leave a shard (or the game closes); older logs in logbackups are read in too.{:else}No sessions saved. Turn on Shard history (under Read Game.log events) to keep a history of the shards you join.{/if}
      </p>
    {:else}
      {#if filters.channels.length > 1 || filters.regions.length > 1}
        <div class="flex flex-wrap items-center gap-2">
          {#if filters.channels.length > 1}<SelectField ariaLabel="Channel" options={opt('All channels', filters.channels)} value={channel} onchange={(v) => (channel = v)} />{/if}
          {#if filters.regions.length > 1}<SelectField ariaLabel="Region" options={opt('All regions', filters.regions)} value={region} onchange={(v) => (region = v)} />{/if}
          {#if rows.length !== total}<span class="text-muted-foreground">{rows.length} of {total}</span>{/if}
        </div>
      {/if}
      <ol class="grid max-h-96 gap-1 overflow-auto" aria-label="Shard sessions, newest first">
        {#each shown as r (r.key)}
          {@const s = r.stint}
          {@const why = s.left_reason ? LEFT_REASON[s.left_reason] : null}
          <li class="grid gap-0.5 rounded-md border px-2 py-1">
            <div class="flex flex-wrap items-center gap-2">
              <span class="mono text-muted-foreground shrink-0">{whenText(s.joined_ms)}</span>
              {#if s.channel}<span class="font-medium">{s.channel}</span>{/if}
              <span class="mono" title={s.shard}>{s.shard_short ?? s.shard}</span>
              <span class="text-muted-foreground">{regionOf(s)}</span>
              <span class="flex-1"></span>
              <span class="mono">{durationText(s.uptime_s)}</span>
              {#if why}<ToneBadge tone={why.tone} label={why.label} hint={why.hint} icon={s.dropped ? TriangleAlert : null} />{/if}
            </div>
            {#if s.systems.length || r.sameShard || s.disconnects > 1}
              <div class="text-muted-foreground flex flex-wrap items-center gap-2">
                {#if s.systems.length}<span>{s.systems.join(' → ')}</span>{/if}
                {#if r.sameShard}<span class="inline-flex items-center gap-1" title="The session before this one on this channel was on the same shard"><Repeat class="size-3" aria-hidden="true" /> same shard as before</span>{/if}
                {#if s.disconnects > 1}<span>{s.disconnects} disconnects</span>{/if}
              </div>
            {/if}
          </li>
        {/each}
      </ol>
      {#if rows.length > SHOWN}<p class="text-muted-foreground">The newest {SHOWN} are shown; a filter narrows the list.</p>{/if}
    {/if}
  </Card.Content>
</Card.Root>
