<script lang="ts">
  /**
   * Live telemetry -> Recordings: the sessions recorded with "Record sessions" (Settings ->
   * Experimental, off by default), newest first, with Replay and Delete. Files live in the
   * app's data folder (Settings -> Storage counts them) and never leave the computer. The one
   * being written and the one playing cannot be deleted.
   */
  import Circle from '@lucide/svelte/icons/circle';
  import History from '@lucide/svelte/icons/history';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Trash from '@lucide/svelte/icons/trash-2';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { recordingText } from '@/lib/telemetry';
  import { sizeText } from '@/lib/storage';
  import { telemetry } from '@/lib/stores/telemetry.svelte';
  import { app } from '@/lib/stores/app.svelte';

  const list = $derived(telemetry.recordings);
  const items = $derived(Array.isArray(list?.items) ? list.items : []);
  const st = $derived(telemetry.status);
  const recordOn = $derived(!!st?.settings.enabled && !!st?.settings.record);
  /** The list on mount, and again when a recording starts or ends (once, not per frame). */
  const writing = $derived(st?.recording ?? null);
  $effect(() => {
    void writing;
    void telemetry.loadRecordings();
  });
  const total = $derived(items.reduce((n, r) => n + r.bytes, 0));
</script>

<Card.Root aria-labelledby="tele-recordings">
  <Card.Header>
    <Card.Title id="tele-recordings" class="flex flex-wrap items-center gap-2">
      Recordings
      {#if writing}<ToneBadge tone="destructive" icon={Circle} label="Recording" hint={writing} />{/if}
    </Card.Title>
    <Card.Description>
      {#if recordOn}
        Record sessions is on: each stream is saved here from its first frame, {list?.record_hz ?? 4} frames a second.
      {:else}
        Record sessions is off. Turn it on under Settings → Experimental to save your sessions and replay them here.
      {/if}
      Saved on this computer only{list ? `: ${list.max_files} at most, ${sizeText(list.max_total_bytes)} in all, the oldest go first` : ''}.
    </Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-2">
    {#if st?.record_error}<p class="text-warning text-[12px]" role="status">{st.record_error}</p>{/if}
    {#if items.length === 0}
      <p class="text-muted-foreground text-[12px]">No recordings yet.</p>
    {:else}
      <ul class="grid gap-1" aria-label="Recordings">
        {#each items as r (r.name)}
          {@const busy = r.name === list?.recording || r.name === writing}
          {@const playing = r.name === telemetry.replay?.name}
          <li class="flex flex-wrap items-center gap-2 rounded-md border px-2 py-1.5 text-[12px]">
            <span class="min-w-0 flex-1 truncate" title={r.name}>{recordingText(r)}</span>
            <span class="mono text-muted-foreground">{sizeText(r.bytes)}</span>
            {#if playing}
              <ToneBadge tone="info" label="Playing" />
            {:else}
              <Button size="xs" variant="outline" disabled={busy} onclick={() => void telemetry.replayStart(r.name)}><History aria-hidden="true" /> Replay</Button>
            {/if}
            <ConfirmButton disabled={busy || playing} confirmLabel="Delete?" aria-label={`Delete ${r.name}`} onconfirm={() => void telemetry.deleteRecording(r.name)}><Trash aria-hidden="true" /></ConfirmButton>
          </li>
        {/each}
      </ul>
      <p class="text-muted-foreground text-[11.5px]">{items.length} {items.length === 1 ? 'recording' : 'recordings'}, {sizeText(total)}.</p>
    {/if}
    <div class="flex flex-wrap gap-1.5">
      <Button variant="ghost" size="xs" onclick={() => void telemetry.loadRecordings()}><RefreshCw aria-hidden="true" /> Refresh</Button>
      {#if !recordOn}
        <Button variant="ghost" size="xs" onclick={() => ((app.settingsSection = 'experimental'), app.go('settings'))}>Open Settings</Button>
      {/if}
    </div>
  </Card.Content>
</Card.Root>
