<script lang="ts">
  /**
   * Live telemetry -> Raw frame: the whole latest frame as JSON, for fields the panels do not
   * show. Closed by default; open, it follows the stream once a second (not at 10 Hz), and
   * Pause holds the frame on screen for reading or copying.
   */
  import { untrack } from 'svelte';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Copy from '@lucide/svelte/icons/copy';
  import Pause from '@lucide/svelte/icons/pause';
  import Play from '@lucide/svelte/icons/play';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import type { TelemetryFrame } from '@/lib/contract/telemetry';
  import { gate, inspectorText } from '@/lib/telemetry';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  let { frame }: { frame: TelemetryFrame | null } = $props();

  let open = $state(false);
  let paused = $state(false);
  let shown = $state.raw<TelemetryFrame | null>(null);
  const tick = gate(1000);

  $effect(() => {
    const f = frame;
    if (!open || paused || !f) return;
    if (!untrack(() => shown) || tick()) shown = f;
  });

  const view = $derived(open && shown ? inspectorText(shown.raw) : null);

  async function copy() {
    if (!shown) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(shown.raw, null, 2));
      toast('success', 'Copied the frame', 'It holds your in-game state (health, ship, position): share it only if you mean to.');
    } catch {
      toast('error', 'Copy failed', 'The clipboard is not available.');
    }
  }
</script>

<Card.Root class="lg:col-span-2">
  <Card.Header>
    <Card.Title>
      <button type="button" class="flex items-center gap-1.5 text-left" aria-expanded={open} aria-controls="tele-raw" onclick={() => (open = !open)}>
        <ChevronRight class={cn('size-4 transition-transform', open && 'rotate-90')} aria-hidden="true" />
        Raw frame
      </button>
    </Card.Title>
    <Card.Description>Everything the game sent in the latest frame, field names as the game names them.</Card.Description>
  </Card.Header>
  {#if open}
    <Card.Content id="tele-raw" class="grid gap-2">
      <div class="flex flex-wrap items-center gap-2 text-[12px]">
        <Button variant="ghost" size="xs" aria-pressed={paused} onclick={() => (paused = !paused)}>
          {#if paused}<Play aria-hidden="true" /> Follow{:else}<Pause aria-hidden="true" /> Pause{/if}
        </Button>
        <Button variant="ghost" size="xs" disabled={!shown} onclick={copy}><Copy aria-hidden="true" /> Copy JSON</Button>
        <span class="text-muted-foreground">{shown ? `Frame ${shown.seq}${paused ? ' (paused)' : ', once a second'}` : 'No frame yet.'}</span>
      </div>
      {#if view}
        <pre class="mono bg-background/60 max-h-[32rem] overflow-auto rounded-md border p-3 text-[12px] leading-relaxed">{view.text}</pre>
        {#if view.cut}<p class="text-muted-foreground text-[12px]">Cut after 400 lines; Copy JSON has all of it.</p>{/if}
      {/if}
    </Card.Content>
  {/if}
</Card.Root>
