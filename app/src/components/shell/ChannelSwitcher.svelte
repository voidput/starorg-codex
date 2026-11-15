<script lang="ts">
  /**
   * Channel switcher (spec 9.3): segmented radio group up to four channels, otherwise a select.
   * Each option has a status mark whose shape differs per state (filled = applied, half = needs a
   * re-apply, diamond = another tool's file, ring = not applied / not set up), a tooltip with the
   * status in words, and the status in text for screen readers (never colour alone, spec 9.6).
   */
  import * as Select from '@/components/ui/select';
  import type { ChannelSummary } from '@/lib/contract/types';
  import { statusView, type StatusTone } from '@/lib/format';
  import { cn } from '@/lib/utils';

  let { channels, value, onchange }: { channels: ChannelSummary[]; value: string | null; onchange: (ch: string) => void } = $props();

  const dot: Record<StatusTone, string> = {
    success: 'bg-success rounded-full',
    warning: 'rounded-full border-[1.5px] border-warning bg-[linear-gradient(90deg,var(--warning)_50%,transparent_50%)]',
    info: 'bg-info rotate-45 rounded-[1px] scale-90',
    muted: 'rounded-full border-[1.5px] border-muted-foreground',
    destructive: 'bg-destructive rounded-[1px]',
  };
  let group = $state<HTMLElement | null>(null);

  function key(e: KeyboardEvent, i: number) {
    const n = channels.length;
    let j = -1;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') j = (i + 1) % n;
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') j = (i - 1 + n) % n;
    if (j < 0) return;
    e.preventDefault();
    onchange(channels[j]!.channel);
    group?.querySelectorAll<HTMLButtonElement>('[role=radio]')[j]?.focus();
  }
</script>

{#if channels.length <= 4}
  <div bind:this={group} role="radiogroup" aria-label="Channel" class="border-border bg-muted/40 flex gap-0.5 rounded-[10px] border p-[3px]">
    {#each channels as c, i (c.channel)}
      {@const s = statusView(c.status, c.set_up)}
      {@const on = c.channel === value}
      <button
        type="button"
        role="radio"
        aria-checked={on}
        tabindex={on ? 0 : -1}
        class={cn(
          'focus-visible:ring-ring/60 inline-flex h-7 items-center gap-1.5 rounded-[7px] px-2.5 text-[12.5px] font-semibold outline-none focus-visible:ring-2',
          on ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
        title={`${c.channel}: ${s.label}`}
        onclick={() => onchange(c.channel)}
        onkeydown={(e) => key(e, i)}
      >
        <span class={cn('flex size-3.5 items-center justify-center rounded-full', on && 'bg-background')} aria-hidden="true"><span class={cn('size-2', dot[s.tone])}></span></span>
        {c.channel}
        <span class="sr-only">, {s.label}</span>
      </button>
    {/each}
  </div>
{:else}
  <Select.Root type="single" value={value ?? undefined} onValueChange={(v) => v && onchange(v)}>
    <Select.Trigger size="sm" aria-label="Channel" class="min-w-32">{value ?? 'Channel'}</Select.Trigger>
    <Select.Content>
      {#each channels as c (c.channel)}
        {@const s = statusView(c.status, c.set_up)}
        <Select.Item value={c.channel} label={c.channel}>
          <span class={cn('size-2', dot[s.tone])} aria-hidden="true"></span>
          {c.channel}
          <span class="text-muted-foreground ml-auto text-xs">{s.label}</span>
        </Select.Item>
      {/each}
    </Select.Content>
  </Select.Root>
{/if}
