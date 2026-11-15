<script lang="ts">
  /**
   * Channel label in the channel's colour (lib/channels.ts). `variant`: `badge` (name in a tinted
   * pill), `swatch` (a short bar, decorative: put the name next to it), `dot` (small square, for
   * dense rows, with the name for screen readers).
   */
  import { channelStyle } from '@/lib/channels';
  import { cn } from '@/lib/utils';

  let { channel, variant = 'badge', title, class: className }: { channel: string; variant?: 'badge' | 'swatch' | 'dot'; title?: string; class?: string } = $props();
</script>

{#if variant === 'swatch'}
  <span class={cn('chan chan-swatch', className)} style={channelStyle(channel)} aria-hidden="true"></span>
{:else if variant === 'dot'}
  <span class={cn('chan chan-dot', className)} style={channelStyle(channel)} title={title ?? channel}><span class="sr-only">{channel}</span></span>
{:else}
  <span class={cn('chan chan-badge', className)} style={channelStyle(channel)} title={title}>{channel}</span>
{/if}

<style>
  .chan {
    --chan: var(--chan-d);
  }
  :global([data-theme='light']) .chan {
    --chan: var(--chan-l);
  }
  .chan-badge {
    display: inline-flex;
    align-items: center;
    height: 1.125rem;
    padding: 0 0.375rem;
    border-radius: 4px;
    border: 1px solid color-mix(in oklab, var(--chan) 55%, transparent);
    background: color-mix(in oklab, var(--chan) 14%, transparent);
    color: var(--chan);
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.04em;
    line-height: 1;
    white-space: nowrap;
  }
  .chan-swatch {
    display: inline-block;
    width: 3px;
    height: 0.875rem;
    border-radius: 2px;
    background: var(--chan);
    flex-shrink: 0;
  }
  .chan-dot {
    display: inline-block;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 2px;
    background: var(--chan);
    flex-shrink: 0;
  }
</style>
