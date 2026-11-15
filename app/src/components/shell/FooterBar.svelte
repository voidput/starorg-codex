<script lang="ts">
  /** Footer bar (spec 9.3): 24 px mono — build, Data.p4k date, cache state, running job, Privacy. */
  import { app } from '@/lib/stores/app.svelte';
  import { fmtDate } from '@/lib/format';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import PrivacyDialog from './PrivacyDialog.svelte';

  let privacy = $state(false);

  const ch = $derived(app.channel);
  const cache = $derived(ch ? { fresh: 'cache ready', missing: 'no cache', stale: 'cache stale' }[ch.cache] : '');
</script>

<footer class="bg-sidebar text-muted-foreground mono flex h-6 shrink-0 items-center gap-4 overflow-hidden border-t px-3 text-[11px] whitespace-nowrap">
  {#if ch}
    <span>{ch.channel} {ch.build.version ?? 'unknown build'}</span>
    <span>Data.p4k {fmtDate(ch.p4k_time)}</span>
    <span>{cache}</span>
  {:else}
    <span>{app.overview ? `v${app.overview.app_version}` : 'StarOrg Codex'}</span>
  {/if}
  <span class="flex-1"></span>
  <span role="status" aria-live="polite">
    {#if app.job}{app.job.label} ({app.job.step}/{app.job.steps}){:else}No job running{/if}
  </span>
  <button
    type="button"
    class="hover:text-foreground focus-visible:ring-ring/60 flex items-center gap-1 rounded-sm outline-none focus-visible:ring-2"
    onclick={() => (privacy = true)}
  >
    <ShieldCheck class="size-3" aria-hidden="true" /> Privacy
  </button>
</footer>
<PrivacyDialog bind:open={privacy} />
