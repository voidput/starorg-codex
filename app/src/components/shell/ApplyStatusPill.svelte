<script lang="ts">
  /** Status of the current channel as icon + text (spec 9.6). */
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Info from '@lucide/svelte/icons/info';
  import CircleDashed from '@lucide/svelte/icons/circle-dashed';
  import type { ChannelSummary } from '@/lib/contract/types';
  import { statusView } from '@/lib/format';
  import { cn } from '@/lib/utils';

  let { channel, class: className = '' }: { channel: ChannelSummary; class?: string } = $props();
  const s = $derived(statusView(channel.status, channel.set_up));
  const Icon = $derived(s.tone === 'success' ? CircleCheck : s.tone === 'warning' ? RefreshCw : s.tone === 'info' ? Info : CircleDashed);
  const tone = $derived(
    {
      success: 'border-success/40 text-success',
      warning: 'border-warning/40 text-warning',
      info: 'border-info/40 text-info',
      muted: 'border-border text-muted-foreground',
      destructive: 'border-destructive/40 text-destructive',
    }[s.tone],
  );
</script>

<span class={cn('inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-[12px] font-medium whitespace-nowrap', tone, className)} title={s.hint}>
  <Icon class="size-3.5" aria-hidden="true" />
  {s.label}
</span>
