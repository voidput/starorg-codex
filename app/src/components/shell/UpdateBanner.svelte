<script lang="ts">
  /**
   * "StarOrg Strings X.Y.Z is available" (docs/updates.md): non-modal, above the screen, only
   * when "Check for updates" is on and the version is not skipped. Release notes arrive as
   * plain text blocks (sanitised in Rust) and are shown as text, never as HTML.
   */
  import CircleArrowUp from '@lucide/svelte/icons/circle-arrow-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Download from '@lucide/svelte/icons/download';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '@/components/ui/button';
  import { updates } from '@/lib/stores/update.svelte';
  import { bannerRelease, bannerTitle } from '@/lib/update';
  import { fmtDate } from '@/lib/format';
  import { cn } from '@/lib/utils';

  const params = new URLSearchParams(window.location.search);
  const release = $derived(bannerRelease(updates.status, updates.dismissed));
  const inApp = $derived(updates.status?.install === 'in-app');
  let open = $state(params.get('notes') === '1');
</script>

{#if release}
  <section aria-labelledby="update-title" class="border-primary/40 bg-primary/8 mb-4 rounded-lg border px-3 py-2.5 text-sm">
    <div class="flex items-start gap-2.5">
      <CircleArrowUp class="text-primary mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <div class="grid min-w-0 flex-1 gap-0.5">
        <h2 id="update-title" class="font-medium">{bannerTitle(release)}</h2>
        <p class="text-muted-foreground text-[12px]">
          You have {updates.status?.current_version}{release.published ? ` · released ${fmtDate(release.published)}` : ''}{inApp ? '' : ' · download it from the release page'}
        </p>
      </div>
      <Button variant="ghost" size="icon-xs" aria-label="Hide until next start" title="Hide until next start" onclick={() => (updates.dismissed = release.version)}><X aria-hidden="true" /></Button>
    </div>
    <div class="mt-2 flex flex-wrap items-center gap-1.5 pl-6.5">
      {#if inApp}
        <Button size="sm" disabled={updates.installing} onclick={() => updates.install()}><Download aria-hidden="true" /> {updates.installing ? 'Installing…' : 'Install and restart'}</Button>
        <Button variant="outline" size="sm" onclick={() => updates.openPage()}><ExternalLink aria-hidden="true" /> Release page</Button>
      {:else}
        <Button size="sm" onclick={() => updates.openPage()}><ExternalLink aria-hidden="true" /> Download from the release page</Button>
      {/if}
      {#if release.notes.length}
        <Button variant="ghost" size="sm" aria-expanded={open} aria-controls="update-notes" onclick={() => (open = !open)}>
          What's new <ChevronDown class={cn('transition-transform', open && 'rotate-180')} aria-hidden="true" />
        </Button>
      {/if}
      <Button variant="ghost" size="sm" class="text-muted-foreground" onclick={() => updates.skip(release.version)}>Skip this version</Button>
    </div>
    {#if open && release.notes.length}
      <div id="update-notes" class="bg-background/50 mt-2 ml-6.5 max-h-56 overflow-y-auto rounded-md border px-3 py-2 text-[13px] leading-relaxed">
        {#each release.notes as n, i (i)}
          {#if n.kind === 'heading'}
            <h3 class="mt-1.5 font-medium first:mt-0">{n.text}</h3>
          {:else if n.kind === 'item'}
            <p class="text-muted-foreground relative pl-3.5 before:absolute before:left-0.5 before:content-['•']">{n.text}</p>
          {:else}
            <p class="text-muted-foreground">{n.text}</p>
          {/if}
        {/each}
        {#if release.notes_truncated}<p class="text-muted-foreground mt-1.5 text-[12px] italic">More on the release page.</p>{/if}
      </div>
    {/if}
  </section>
{/if}
