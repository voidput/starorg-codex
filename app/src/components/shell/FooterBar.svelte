<script lang="ts">
  /** Footer bar (spec 9.3): 24 px mono — build, Data.p4k date, cache state, running job. */
  import { app } from '@/lib/stores/app.svelte';
  import { fmtDate } from '@/lib/format';

  const ch = $derived(app.channel);
  const cache = $derived(ch ? { fresh: 'cache ready', missing: 'no cache', stale: 'cache stale' }[ch.cache] : '');
</script>

<footer class="bg-sidebar text-muted-foreground mono flex h-6 shrink-0 items-center gap-4 overflow-hidden border-t px-3 text-[11px] whitespace-nowrap">
  {#if ch}
    <span>{ch.channel} {ch.build.version ?? 'unknown build'}</span>
    <span>Data.p4k {fmtDate(ch.p4k_time)}</span>
    <span>{cache}</span>
  {:else}
    <span>{app.overview ? `v${app.overview.app_version}` : 'StarOrg Strings'}</span>
  {/if}
  <span class="flex-1"></span>
  <span role="status" aria-live="polite">
    {#if app.job}{app.job.label} ({app.job.step}/{app.job.steps}){:else}No job running{/if}
  </span>
</footer>
