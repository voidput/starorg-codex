<script lang="ts">
  /**
   * Live telemetry -> the replay's controls: play/pause, a seek slider, the speed, and back to
   * the game. The position follows the replay's events; while the slider is dragged it shows
   * the drag, and the seek goes out once, on release.
   */
  import Pause from '@lucide/svelte/icons/pause';
  import Play from '@lucide/svelte/icons/play';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Square from '@lucide/svelte/icons/square';
  import { Button } from '@/components/ui/button';
  import SelectField from '@/components/common/SelectField.svelte';
  import type { TelemetryReplayState } from '@/lib/contract/telemetry';
  import { SPEEDS, clockText, speedText } from '@/lib/telemetry';
  import { telemetry } from '@/lib/stores/telemetry.svelte';

  let { replay, onlive }: { replay: TelemetryReplayState; onlive: () => void } = $props();

  let drag = $state<number | null>(null);
  const pos = $derived(drag ?? replay.position_ms);
  const speeds = SPEEDS.map((s) => ({ id: String(s), label: speedText(s) }));

  function seek(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    drag = null;
    if (Number.isFinite(v)) void telemetry.replayControl({ seek_ms: v });
  }
</script>

<div class="bg-card grid gap-2 rounded-lg border px-3 py-2" role="group" aria-label="Replay controls">
  <div class="flex flex-wrap items-center gap-2 text-[12px]">
    <Button size="sm" variant="outline" aria-label={replay.playing ? 'Pause' : replay.ended ? 'Play again' : 'Play'} onclick={() => void telemetry.replayControl({ playing: !replay.playing })}>
      {#if replay.playing}<Pause aria-hidden="true" /> Pause{:else if replay.ended}<RotateCcw aria-hidden="true" /> Again{:else}<Play aria-hidden="true" /> Play{/if}
    </Button>
    <span class="mono" aria-live="off">{clockText(pos)} / {clockText(replay.duration_ms)}</span>
    <span class="text-muted-foreground min-w-0 flex-1 truncate" title={replay.name}>{new Date(replay.start_ms + pos).toLocaleTimeString()} · {replay.name}</span>
    <SelectField ariaLabel="Replay speed" options={speeds} value={String(replay.speed)} onchange={(v) => void telemetry.replayControl({ speed: Number(v) })} />
    <Button size="sm" variant="ghost" onclick={onlive}><Square aria-hidden="true" /> Back to live</Button>
  </div>
  <input
    type="range"
    class="accent-primary w-full"
    min="0"
    max={replay.duration_ms}
    step="250"
    value={pos}
    aria-label="Replay position"
    aria-valuetext={`${clockText(pos)} of ${clockText(replay.duration_ms)}`}
    oninput={(e) => (drag = Number((e.currentTarget as HTMLInputElement).value))}
    onchange={seek}
  />
</div>
