<script lang="ts">
  /** A number with its label (RSI+ Stat). */
  let { label, value, hint = null }: { label: string; value: string; hint?: string | null } = $props();
</script>

<div class="bg-card rounded-lg border px-3.5 py-2.5">
  <div class="text-muted-foreground text-[12px]">{label}</div>
  <div class="mono mt-0.5 text-[18px] font-semibold">{value}</div>
  {#if hint}<div class="text-muted-foreground mt-0.5 text-[11.5px]">{hint}</div>{/if}
</div>
