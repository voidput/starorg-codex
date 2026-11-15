<script lang="ts">
  /**
   * Header (spec 9.3): title + description left; channel switcher, status pill and Apply right.
   * Why Apply is off is said in words beside the pill (game running); while a job runs its
   * progress and Stop sit next to the button.
   */
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import ChannelSwitcher from './ChannelSwitcher.svelte';
  import ApplyStatusPill from './ApplyStatusPill.svelte';
  import ApplyButton from './ApplyButton.svelte';
  import JobProgress from '@/components/common/JobProgress.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import type { NavItem } from '@/lib/nav';

  let { page }: { page: NavItem } = $props();
  const ch = $derived(app.channel);
  const why = $derived(ch?.game_running ? app.blocked : null);
</script>

<header class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b px-6 py-3">
  <div class="min-w-0">
    <div class="eyebrow text-primary">StarOrg Codex</div>
    <h1 id="page-title" class="text-h1 truncate">{page.label}</h1>
    <p class="text-muted-foreground truncate">{page.description}</p>
  </div>
  {#if ch && app.install}
    <div class="flex flex-wrap items-center justify-end gap-2">
      <ChannelSwitcher channels={app.channels} value={ch.channel} onchange={(c) => app.selectChannel(c)} />
      <ApplyStatusPill channel={ch} />
      {#if why}
        <span id="apply-blocked" class="text-warning flex items-center gap-1 text-[12px]"><TriangleAlert class="size-3.5" aria-hidden="true" /> {why}</span>
      {/if}
      {#if app.busy && app.job}
        <JobProgress job={app.job} compact />
      {/if}
      <ApplyButton
        label={`Apply to ${ch.channel}`}
        busy={app.busy}
        disabled={!ch.set_up || !!app.blocked}
        describedby={why ? 'apply-blocked' : undefined}
        title={!ch.set_up ? `${ch.channel} is not set up` : (app.blocked ?? undefined)}
        onclick={() => app.requestApply()}
      />
    </div>
  {/if}
</header>
