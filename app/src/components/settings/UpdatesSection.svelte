<script lang="ts">
  /**
   * Settings -> Network -> Check for updates (docs/updates.md). The switch is the consent
   * (spec 11); with it on: pre-releases, the repo (a public mirror works), the last result and
   * "Check now". Requests and the 24 h throttle are decided in Rust. The Microsoft Store build
   * shows only a line saying the Store updates it: no switch, no check, nothing to install.
   */
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import { updates } from '@/lib/stores/update.svelte';
  import { applyUpdateSwitch, checkLine, requestHint, storeManaged } from '@/lib/update';
  import { fmtDateTime } from '@/lib/format';
  import { cn } from '@/lib/utils';

  /** `onchange` saves the switch and resolves once the save has landed (false on failure). */
  let { checked, onchange }: { checked: boolean; onchange: (v: boolean) => Promise<boolean> } = $props();

  const st = $derived(updates.status);
  const line = $derived(st ? checkLine({ ...st, enabled: checked }) : null);
  let repo = $state('');
  let editing = $state(false);
  $effect(() => {
    if (!editing) repo = st?.prefs.repo ?? 'voidput/starorg-strings';
  });
  const tone = { muted: 'text-muted-foreground', success: 'text-success', info: 'text-foreground', warning: 'text-foreground' } as const;

  function toggle(v: boolean) {
    void applyUpdateSwitch(v, { save: onchange, check: () => updates.enabled(), refresh: () => updates.refresh() });
  }
  async function saveRepo() {
    if (!st) return;
    if (await updates.setPrefs({ repo, include_prereleases: st.prefs.include_prereleases })) editing = false;
  }
</script>

{#if storeManaged(st)}
  <div class="flex items-start gap-2 py-1.5">
    <ToneBadge tone="muted" label="Microsoft Store" />
    <p class="text-[13px]" role="status">{st?.install_note || 'Updates come from the Microsoft Store.'}</p>
  </div>
{:else}
<SwitchRow id="net-updates" label="Check for updates" hint={requestHint(st?.prefs.repo ?? 'voidput/starorg-strings')} {checked} onchange={toggle} />
{#if checked && st}
  <div class="mb-1 ml-11.5 grid gap-2.5 border-l pl-3">
    <div class="flex flex-wrap items-center gap-2">
      {#if line}<p class={cn('text-[13px]', tone[line.tone])} role="status">{line.text}</p>{/if}
      <Button variant="outline" size="xs" disabled={updates.checking} onclick={() => updates.check(true)}><RefreshCw class={cn(updates.checking && 'animate-spin')} aria-hidden="true" /> {updates.checking ? 'Checking…' : 'Check now'}</Button>
    </div>
    <p class="text-muted-foreground text-[12px]">Last checked {st.last_checked ? fmtDateTime(st.last_checked) : 'never'}{#if st.skipped_version} · skipping {st.skipped_version} <Button variant="link" size="xs" class="h-auto p-0 text-[12px]" onclick={() => updates.skip('')}>Undo</Button>{/if}</p>
    <SwitchRow id="net-prerelease" size="sm" label="Include pre-releases" hint="Also offer beta and release-candidate versions." checked={st.prefs.include_prereleases} onchange={(v) => updates.setPrefs({ repo: st.prefs.repo, include_prereleases: v })} />
    <div class="grid gap-1">
      <label for="net-repo" class="text-[13px]">Release repository</label>
      <div class="flex max-w-md gap-1.5">
        <Input id="net-repo" class="mono h-7 text-[12px]" bind:value={repo} oninput={() => (editing = true)} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && saveRepo()} spellcheck={false} autocomplete="off" aria-describedby="net-repo-hint" />
        <Button variant="outline" size="sm" disabled={!editing || repo === st.prefs.repo} onclick={saveRepo}>Save</Button>
      </div>
      <p id="net-repo-hint" class="text-muted-foreground text-[12px]">owner/name on github.com. A private repo shows no releases (no sign-in is ever sent); use a public mirror of the releases instead.</p>
    </div>
    <div class="flex items-start gap-2">
      <ToneBadge tone={st.install === 'in-app' ? 'success' : 'muted'} label={st.install === 'in-app' ? 'Installs in the app' : 'Download only'} />
      <p class="text-muted-foreground text-[12px]">{st.install_note}</p>
    </div>
  </div>
{/if}
{/if}
