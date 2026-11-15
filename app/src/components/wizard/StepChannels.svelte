<script lang="ts">
  /** The consent step (spec 9.4 step 2): nothing writes into a channel that is not ticked here. */
  import Languages from '@lucide/svelte/icons/languages';
  import Info from '@lucide/svelte/icons/info';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { Checkbox } from '@/components/ui/checkbox';
  import * as Alert from '@/components/ui/alert';
  import ApplyStatusPill from '@/components/shell/ApplyStatusPill.svelte';
  import type { InstallSummary } from '@/lib/contract/types';
  import type { Draft } from './draft.svelte';
  import { cn } from '@/lib/utils';

  let { install, draft }: { install: InstallSummary; draft: Draft } = $props();
</script>

<p class="text-muted-foreground">
  Setting up a channel lets StarOrg Codex write <span class="mono">Data/Localization/english/global.ini</span> there and, only when needed, one line in
  <span class="mono">user.cfg</span>. Launch hooks and the command line never touch a channel you do not tick here.
</p>

<fieldset class="grid gap-2">
  <legend class="eyebrow text-muted-foreground mb-1">Channels in this install</legend>
  {#each install.channels as c (c.channel)}
    {@const choice = draft.channels[c.channel]!}
    {@const id = `ch-${c.channel}`}
    <div class={cn('bg-card/80 grid gap-2 rounded-lg border p-3', choice.consent && 'border-primary/60')}>
      <div class="flex flex-wrap items-center gap-3">
        <Checkbox {id} checked={choice.consent} disabled={!!c.build_error} onCheckedChange={(v) => (choice.consent = v === true)} aria-describedby="{id}-desc" />
        <label for={id} class="w-20 cursor-pointer font-semibold">{c.channel}</label>
        <span id="{id}-desc" class="mono text-muted-foreground flex-1">
          {c.build_error ? `No build info: ${c.build_error}` : `${c.build.branch} · ${c.build.version}`}
        </span>
        {#if c.language === 'other'}
          <span class="text-warning flex items-center gap-1 text-[12px]"><Languages class="size-3.5" aria-hidden="true" /> Language: {c.language_value ?? 'not English'}</span>
        {/if}
        <ApplyStatusPill channel={c} />
      </div>
      {#if choice.consent && c.foreign_tool}
        <Alert.Root>
          <Info aria-hidden="true" />
          <Alert.Title>{c.foreign_tool}'s text file is in place</Alert.Title>
          <Alert.Description>It is backed up first (kept for good, never rotated out) and then replaced. Do not run two tools on one channel.</Alert.Description>
        </Alert.Root>
      {/if}
      {#if choice.consent && c.language === 'other'}
        <fieldset class="border-warning/40 bg-warning/10 grid gap-1.5 rounded-md border p-3">
          <legend class="flex items-center gap-1.5 px-1 font-semibold"><Languages class="text-warning size-4" aria-hidden="true" /> {c.channel} is set to another language</legend>
          <p class="text-muted-foreground">
            Its <span class="mono">user.cfg</span> says <span class="mono">g_language = {c.language_value}</span>. StarOrg Codex writes English text, which only shows when the game language is English.
          </p>
          <label class="flex cursor-pointer items-center gap-2">
            <input type="radio" name="lang-{c.channel}" class="accent-primary size-4" checked={choice.switch_language === true} onchange={() => (choice.switch_language = true)} />
            Switch {c.channel} to English (adds <span class="mono">g_language = english</span>)
          </label>
          <label class="flex cursor-pointer items-center gap-2">
            <input type="radio" name="lang-{c.channel}" class="accent-primary size-4" checked={false} onchange={() => { choice.consent = false; choice.switch_language = null; }} />
            Leave its language and skip {c.channel}
          </label>
          {#if choice.switch_language === null}
            <p class="text-warning flex items-center gap-1.5 text-[12px]"><TriangleAlert class="size-3.5" aria-hidden="true" /> Choose one to continue.</p>
          {/if}
        </fieldset>
      {/if}
    </div>
  {/each}
</fieldset>
