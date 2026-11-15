<script lang="ts">
  /** Settings (spec 9.4): Game, Appearance, Network. All network switches are off by default (spec 11). */
  import Wand from '@lucide/svelte/icons/wand-sparkles';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import type { Accent, Settings, Theme, Palette } from '@/lib/contract/types';
  import { cn } from '@/lib/utils';

  const accents: Accent[] = ['cyan', 'green', 'amber', 'orange', 'rose', 'violet'];
  const swatch: Record<Accent, string> = { cyan: '#3cc8ff', green: '#3ddc97', amber: '#ffd166', orange: '#ff9f43', rose: '#ff6b9d', violet: '#a78bfa' };

  function patch(f: (s: Settings) => void) {
    if (!app.settings) return;
    const s = structuredClone($state.snapshot(app.settings)) as Settings;
    f(s);
    void app.saveSettings(s);
  }
</script>

{#if app.settings}
  {@const s = app.settings}
  <div class="grid max-w-3xl gap-4">
    <Card.Root>
      <Card.Header><Card.Title>Game</Card.Title><Card.Description>Where StarOrg Strings writes, and which channels are set up.</Card.Description></Card.Header>
      <Card.Content class="grid gap-2">
        <div class="mono truncate" title={app.install?.root}>{app.install?.root ?? 'No install chosen'}</div>
        <div><Button variant="outline" size="sm" onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Run setup again</Button></div>
      </Card.Content>
    </Card.Root>
    <Card.Root>
      <Card.Header><Card.Title>Appearance</Card.Title></Card.Header>
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
            {#each accents as a (a)}
              <button
                type="button"
                role="radio"
                aria-checked={s.appearance.accent === a}
                aria-label={a}
                class={cn('focus-visible:ring-ring size-6 rounded-full border-2 outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-background', s.appearance.accent === a ? 'border-foreground' : 'border-transparent')}
                style="background: {swatch[a]}"
                onclick={() => patch((x) => (x.appearance.accent = a))}
              ></button>
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
        <SwitchRow id="reduce-effects" label="Reduce effects" hint="Solid surfaces instead of blur. On by default on Linux, where WebKitGTK draws blur slowly on NVIDIA." checked={s.appearance.reduce_effects} onchange={(v) => patch((x) => (x.appearance.reduce_effects = v))} />
      </Card.Content>
    </Card.Root>
    <Card.Root>
      <Card.Header>
        <Card.Title>Network</Card.Title>
        <Card.Description>All off by default. With these off, StarOrg Strings' own code makes no network request.</Card.Description>
      </Card.Header>
      <Card.Content class="grid gap-1">
        <SwitchRow id="net-updates" label="Check for updates" hint="One request to the GitHub release endpoint of voidput/starorg-strings when the app starts." checked={s.network.check_updates} onchange={(v) => patch((x) => (x.network.check_updates = v))} />
        <SwitchRow id="net-dumps" label="Use published game data" hint="Downloads the datasets for your exact build from github.com/voidput/sc-gamedata-dumps, checked by checksum and read as data only. Without a published build, game data is read locally." checked={s.network.published_dumps} onchange={(v) => patch((x) => (x.network.published_dumps = v))} />
        <SwitchRow id="net-starorg" label="Connect StarOrg Tools" hint="Lets you sign in to starorg.tools in your browser to pull and push owned blueprints. Nothing is sent until you sign in." checked={s.network.starorg} onchange={(v) => patch((x) => (x.network.starorg = v))} />
        <p class="text-muted-foreground mt-2 text-[12px]">On Windows the window is drawn by Microsoft's WebView2 runtime, which has its own update traffic that the app cannot fully switch off.</p>
      </Card.Content>
    </Card.Root>
  </div>
{/if}
