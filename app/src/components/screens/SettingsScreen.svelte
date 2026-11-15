<script lang="ts">
  /**
   * Settings (spec 9.4): Game, Appearance, Network (all off by default, spec 11), StarOrg Tools,
   * Launch hooks (Linux), Diagnostics, Advanced (Developer mode, addendum 7.8), Experimental
   * (local MCP server), Command line. A section index on top jumps by anchor.
   */
  import Wand from '@lucide/svelte/icons/wand-sparkles';
  import * as Card from '@/components/ui/card';
  import { jumpToSection } from '@/lib/scroll';
  import { Button } from '@/components/ui/button';
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import type { Accent, Settings, Theme, Palette } from '@/lib/contract/types';
  import { cn } from '@/lib/utils';
  import FolderSearch from '@lucide/svelte/icons/folder-search';
  import SelectField from '@/components/common/SelectField.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import StarOrgCard from '@/components/settings/StarOrgCard.svelte';
  import HooksCard from '@/components/settings/HooksCard.svelte';
  import DoctorCard from '@/components/settings/DoctorCard.svelte';
  import UpdatesSection from '@/components/settings/UpdatesSection.svelte';
  import McpCard from '@/components/settings/McpCard.svelte';
  import AssetCacheRow from '@/components/settings/AssetCacheRow.svelte';
  import MemoryLimitRow from '@/components/settings/MemoryLimitRow.svelte';
  import MemoryWarningDialog from '@/components/settings/MemoryWarningDialog.svelte';
  import { statusView } from '@/lib/format';

  const sections = [
    { id: 'game', label: 'Game' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'network', label: 'Network' },
    { id: 'starorg', label: 'StarOrg Tools' },
    { id: 'hooks', label: 'Launch hooks' },
    { id: 'diagnostics', label: 'Diagnostics' },
    { id: 'advanced', label: 'Advanced' },
    { id: 'experimental', label: 'Experimental' },
    { id: 'cli', label: 'Command line' },
  ];
  const jump = (id: string) => void jumpToSection(id);

  const accents: Accent[] = ['cyan', 'green', 'amber', 'orange', 'rose', 'violet'];
  const swatch: Record<Accent, string> = { cyan: '#3cc8ff', green: '#3ddc97', amber: '#ffd166', orange: '#ff9f43', rose: '#ff6b9d', violet: '#a78bfa' };

  /** Resolves once the save has landed (false if it failed); most callers do not wait. */
  function patch(f: (s: Settings) => void): Promise<boolean> {
    if (!app.settings) return Promise.resolve(false);
    const s = structuredClone($state.snapshot(app.settings)) as Settings;
    f(s);
    return app.saveSettings(s);
  }
  import Check from '@lucide/svelte/icons/check';
  /** Turning Developer mode on asks first: its workers can use several GB (addendum 7.18). */
  let devWarning = $state(false);
  async function devModeOn() {
    if (!(await patch((x) => (x.advanced.developer_mode = true)))) throw new Error('Developer mode was not turned on; see the message above.');
  }
  let rescanning = $state(false);
  /** Re-scan looks for installs again (discover_installs), then reloads the overview. */
  async function rescan() {
    rescanning = true;
    try {
      const found = await api.discoverInstalls(false);
      toast('success', `Found ${found.length} install${found.length === 1 ? '' : 's'}`, found.map((f) => f.found_by_label).join(', ') || null);
    } catch (e) {
      toast('error', 'Re-scan failed', toContractError(e).message);
    } finally {
      rescanning = false;
      await app.refresh();
    }
  }
  function accentKey(e: KeyboardEvent, i: number) {
    const n = accents.length;
    const j = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? (i + 1) % n : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? (i - 1 + n) % n : -1;
    if (j < 0) return;
    e.preventDefault();
    const a = accents[j]!;
    patch((x) => (x.appearance.accent = a));
    ((e.currentTarget as HTMLElement).parentElement?.children[j] as HTMLElement | undefined)?.focus();
  }
  import { api, toContractError } from '@/lib/contract/client';
  import { toast } from '@/lib/stores/toast.svelte';
</script>

{#if app.settings}
  {@const s = app.settings}
  <div class="grid max-w-3xl gap-4">
    <nav aria-label="Settings sections" class="flex flex-wrap gap-1.5">
      {#each sections as x (x.id)}
        <Button variant="ghost" size="xs" href="#{x.id}" onclick={(e: MouseEvent) => (e.preventDefault(), jump(x.id))}>{x.label}</Button>
      {/each}
    </nav>
    <Card.Root id="game">
      <Card.Header><Card.Title tabindex={-1} class="focus-visible:ring-ring/60 rounded-sm outline-none focus-visible:ring-2">Game</Card.Title><Card.Description>Where StarOrg Codex writes, and which channels are set up.</Card.Description></Card.Header>
      <Card.Content class="grid gap-3">
        {#if app.install}
          <div class="grid gap-0.5">
            <div class="mono truncate" title={app.install.root}>{app.install.root}</div>
            <div class="text-muted-foreground text-[12px]">Found by {app.install.found_by_label}{app.install.prefix ? ` · Wine prefix ${app.install.prefix}` : ''}</div>
          </div>
          <ul class="grid gap-1" aria-label="Channels">
            {#each app.channels as c (c.channel)}
              {@const v = statusView(c.status, c.set_up)}
              <li class="flex items-center gap-3"><span class="w-16 font-medium">{c.channel}</span><ToneBadge tone={c.set_up ? 'success' : 'muted'} label={c.set_up ? 'Set up' : 'Not set up'} /><span class="text-muted-foreground text-[12px]">{v.label}</span></li>
            {/each}
          </ul>
          <div class="flex flex-wrap items-center gap-3">
            <span id="def-ch">Default channel</span>
            <SelectField
              ariaLabel="Default channel"
              options={app.channels.filter((c) => c.set_up).map((c) => ({ id: c.channel, label: c.channel }))}
              value={s.default_channel}
              placeholder="First set-up channel"
              onchange={(v) => patch((x) => (x.default_channel = v))}
            />
          </div>
        {:else}
          <p class="text-muted-foreground">No install chosen.</p>
        {/if}
        <div class="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Run setup again</Button>
          <Button variant="ghost" size="sm" disabled={rescanning} onclick={rescan}><FolderSearch aria-hidden="true" /> {rescanning ? 'Looking…' : 'Re-scan'}</Button>
        </div>
      </Card.Content>
    </Card.Root>
    <Card.Root id="appearance">
      <Card.Header><Card.Title tabindex={-1} class="focus-visible:ring-ring/60 rounded-sm outline-none focus-visible:ring-2">Appearance</Card.Title></Card.Header>
      <Card.Content class="grid gap-3">
        <div class="flex flex-wrap items-center gap-3">
          <span class="w-24">Theme</span>
          <Segmented
            ariaLabel="Theme"
            options={[{ id: 'system', label: 'System' }, { id: 'dark', label: 'Dark' }, { id: 'light', label: 'Light' }] as { id: Theme; label: string }[]}
            value={s.appearance.theme}
            onchange={(v) => patch((x) => (x.appearance.theme = v))}
          />
        </div>
        <div class="flex flex-wrap items-center gap-3">
          <span class="w-24" id="accent-label">Accent</span>
          <div role="radiogroup" aria-labelledby="accent-label" class="flex gap-1.5">
            {#each accents as a, i (a)}
              {@const on = s.appearance.accent === a}
              <button
                type="button"
                role="radio"
                aria-checked={on}
                aria-label={a[0]!.toUpperCase() + a.slice(1)}
                tabindex={on ? 0 : -1}
                class={cn('focus-visible:ring-ring flex size-6 items-center justify-center rounded-full border-2 outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-background', on ? 'border-foreground' : 'border-transparent')}
                style="background: {swatch[a]}"
                onclick={() => patch((x) => (x.appearance.accent = a))}
                onkeydown={(e) => accentKey(e, i)}
              >{#if on}<Check class="size-3.5 text-black" strokeWidth={3} aria-hidden="true" />{/if}</button>
            {/each}
          </div>
        </div>
        <div class="flex flex-wrap items-center gap-3">
          <span class="w-24">Palette</span>
          <Segmented
            ariaLabel="Palette"
            options={[{ id: 'void', label: 'Void' }, { id: 'black', label: 'Black (OLED)' }] as { id: Palette; label: string }[]}
            value={s.appearance.palette}
            onchange={(v) => patch((x) => (x.appearance.palette = v))}
          />
        </div>
        <SwitchRow id="reduce-effects" label="Reduce effects" hint="Solid surfaces instead of see-through blur, for a smoother window. On by default on Linux (the Linux web view draws blur slowly on some graphics cards)." checked={s.appearance.reduce_effects} onchange={(v) => patch((x) => (x.appearance.reduce_effects = v))} />
      </Card.Content>
    </Card.Root>
    <Card.Root id="network">
      <Card.Header>
        <Card.Title tabindex={-1} class="focus-visible:ring-ring/60 rounded-sm outline-none focus-visible:ring-2">Network</Card.Title>
        <Card.Description>All off by default. With these off, StarOrg Codex' own code makes no network request.</Card.Description>
      </Card.Header>
      <Card.Content class="grid gap-1">
        <UpdatesSection checked={s.network.check_updates} onchange={(v) => patch((x) => (x.network.check_updates = v))} />
        <SwitchRow id="net-dumps" label="Use published game data" hint="Not in this build yet; game data is read from Data.p4k on this machine. When built: downloads the datasets for your exact build from github.com/voidput/sc-gamedata-dumps, checked by checksum and read as data only, for apply and as a Developer mode dataset source." checked={false} disabled onchange={() => {}} />
        <SwitchRow id="net-starorg" label="Connect StarOrg Tools" hint="Lets you sign in to StarOrg Tools (starorg.tools, or dev.starorg.tools below) in your browser. Nothing is sent until you sign in, and then only what you turn on under StarOrg Tools -> What to sync." checked={s.network.starorg} onchange={(v) => patch((x) => (x.network.starorg = v))} />
        <p class="text-muted-foreground mt-2 text-[12px]">No telemetry, crash upload or analytics exists in the app. On Windows the window is drawn by Microsoft's WebView2 runtime, which has its own update traffic that the app cannot fully switch off. On Linux the window loads only the app's own pages.</p>
      </Card.Content>
    </Card.Root>
    <StarOrgCard />
    <HooksCard />
    <DoctorCard />
    <Card.Root id="advanced">
      <Card.Header>
        <Card.Title tabindex={-1} class="focus-visible:ring-ring/60 rounded-sm outline-none focus-visible:ring-2">Advanced</Card.Title>
      </Card.Header>
      <Card.Content class="grid gap-1">
        <SwitchRow
          id="dev-mode"
          label="Developer mode"
          hint="Adds tools to browse the game's data records and compare datasets. Read only. Its workers can use several GB of RAM (about 0.55-1 GB open, model exports up to 11 GB), each held to a memory limit you can set here once it is on."
          checked={s.advanced.developer_mode || devWarning}
          onchange={(v) => (v ? (devWarning = true) : patch((x) => (x.advanced.developer_mode = false)))}
        />
        <MemoryWarningDialog bind:open={devWarning} what="dev" onconfirm={devModeOn} />
        {#if s.advanced.developer_mode}
          <p class="text-muted-foreground mt-1 text-[12px]">Explorer (Alt+9), Datasets, Diff and Files are in the sidebar under Dev; Settings moves to Alt+0. Ctrl+K has "Find record…".</p>
          <AssetCacheRow />
          <MemoryLimitRow />
        {/if}
      </Card.Content>
    </Card.Root>
    <McpCard />
    <Card.Root id="cli">
      <Card.Header>
        <Card.Title tabindex={-1} class="focus-visible:ring-ring/60 rounded-sm outline-none focus-visible:ring-2">Command line</Card.Title>
        <Card.Description>Everything the app does is also a command; scripts and launch hooks use it.</Card.Description>
      </Card.Header>
      <Card.Content class="grid gap-2">
        <pre class="mono bg-background/60 overflow-x-auto rounded-md border p-3 text-[12px] leading-relaxed">starorg-strings status --json
starorg-strings apply --channel LIVE
starorg-strings apply --all --if-stale --timeout 20
starorg-strings remove --channel LIVE
starorg-strings blueprints import owned-blueprints.json --character Pilot_A --dry-run
{#if s.advanced.developer_mode}starorg-strings dev types --channel LIVE
starorg-strings dev record &lt;guid|path|name&gt; --channel LIVE
starorg-strings dev extract "Data/Libs/Config/*.xml" --out DIR --convert
starorg-strings dev export-model AEGS_Javelin --preset print --out javelin.glb
starorg-strings dev export-sounds AEGS_Gladius --out DIR
{/if}starorg-strings sync status    # StarOrg environment and what to sync
starorg-strings sync set --import-game-log off
starorg-strings doctor
starorg-strings mcp            # experimental: MCP server on stdio</pre>
        <p class="text-muted-foreground text-[12px]">Exit codes: 0 done, 5 applied with warnings, other values an error named in the output. Before uninstalling, run <span class="mono">starorg-strings remove --all</span> to put stock text back.</p>
      </Card.Content>
    </Card.Root>
  </div>
{/if}
