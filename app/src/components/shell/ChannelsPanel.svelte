<script lang="ts">
  import { onMount } from 'svelte';
  import { RefreshCw, CircleCheck, CircleAlert, FolderSearch } from '@lucide/svelte';
  import { Button } from '@/components/ui/button';
  import { Badge } from '@/components/ui/badge';
  import { Skeleton } from '@/components/ui/skeleton';
  import * as Alert from '@/components/ui/alert';
  import { listChannels, type ChannelsView } from '@/lib/api';

  let data = $state<ChannelsView | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);

  async function load() {
    loading = true;
    error = null;
    try {
      data = await listChannels();
    } catch (e) {
      error = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);
</script>

<section aria-labelledby="channels-title" class="flex flex-col gap-3">
  <div class="flex items-center justify-between gap-3">
    <div>
      <div class="eyebrow text-muted-foreground">Game</div>
      <h2 id="channels-title" class="text-[16px] font-semibold">Channels</h2>
    </div>
    <Button variant="outline" size="sm" onclick={load} disabled={loading} aria-label="Scan again">
      <RefreshCw class={loading ? 'animate-spin' : ''} aria-hidden="true" />
      Scan again
    </Button>
  </div>

  <div aria-live="polite" class="sr-only">{loading ? 'Scanning for Star Citizen installs' : ''}</div>

  {#if loading && !data}
    <div class="flex flex-col gap-2">
      <Skeleton class="h-24 w-full" />
    </div>
  {:else if error}
    <Alert.Root variant="destructive" role="alert">
      <CircleAlert aria-hidden="true" />
      <Alert.Title>Could not scan for installs</Alert.Title>
      <Alert.Description>{error}</Alert.Description>
    </Alert.Root>
  {:else if data && Array.isArray(data.installs) && data.installs.length === 0}
    <div class="bg-card flex flex-col items-center gap-2 rounded-lg border px-6 py-10 text-center">
      <FolderSearch class="text-muted-foreground size-8" aria-hidden="true" />
      <div class="font-semibold">No Star Citizen install found</div>
      <p class="text-muted-foreground max-w-md">
        Looked in the default Wine prefixes, the LUG Helper config and the RSI Launcher logs. Choosing a folder arrives with the setup wizard.
      </p>
    </div>
  {:else if data && Array.isArray(data.installs)}
    {#each data.installs as install, i (install.root)}
      <article class="bg-card chamfer-md relative border" aria-label="Install {i + 1}">
        <header class="flex flex-wrap items-baseline gap-x-3 gap-y-1 border-b px-4 py-3">
          {#if i === 0}<Badge variant="info">Default</Badge>{/if}
          <span class="mono min-w-0 flex-1 truncate" title={install.root}>{install.root}</span>
          <span class="text-muted-foreground text-[12px]">
            found by {install.found_by_label}{install.prefix ? ` · Wine prefix ${install.prefix}` : ''}
          </span>
        </header>
        <table class="w-full text-left">
          <thead class="text-muted-foreground">
            <tr class="border-b">
              <th scope="col" class="eyebrow px-4 py-2 font-bold">Channel</th>
              <th scope="col" class="eyebrow px-4 py-2 font-bold">Branch</th>
              <th scope="col" class="eyebrow px-4 py-2 font-bold">Game</th>
              <th scope="col" class="eyebrow px-4 py-2 font-bold">Change</th>
              <th scope="col" class="eyebrow px-4 py-2 font-bold">Version</th>
            </tr>
          </thead>
          <tbody>
            {#each install.channels as ch (ch.channel)}
              <tr class="hover:bg-accent/50 border-b last:border-b-0">
                <th scope="row" class="px-4 py-2 font-semibold">
                  <span class="inline-flex items-center gap-1.5">
                    {#if ch.build_error}
                      <CircleAlert class="text-warning size-3.5" aria-hidden="true" />
                      <span class="sr-only">No build info:</span>
                    {:else}
                      <CircleCheck class="text-success size-3.5" aria-hidden="true" />
                      <span class="sr-only">Found:</span>
                    {/if}
                    {ch.channel}
                  </span>
                </th>
                {#if ch.build_error}
                  <td colspan="4" class="text-warning px-4 py-2">No build info: {ch.build_error}</td>
                {:else}
                  <td class="mono px-4 py-2">{ch.branch}</td>
                  <td class="mono px-4 py-2">{ch.game_version ?? 'unknown'}</td>
                  <td class="mono px-4 py-2">{ch.change ?? 'unknown'}</td>
                  <td class="mono text-muted-foreground px-4 py-2">{ch.version}</td>
                {/if}
              </tr>
            {/each}
          </tbody>
        </table>
      </article>
    {/each}
  {/if}
</section>
