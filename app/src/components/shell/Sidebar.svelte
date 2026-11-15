<script lang="ts">
  import { House, Sparkles, Table2, Wrench, Archive, ScrollText, Settings } from '@lucide/svelte';
  import { NAV, type ScreenId } from '@/lib/nav';

  let { current = $bindable<ScreenId>('home'), version = '' }: { current: ScreenId; version?: string } = $props();

  const icons = {
    home: House,
    enhancements: Sparkles,
    strings: Table2,
    patches: Wrench,
    backups: Archive,
    activity: ScrollText,
    settings: Settings,
  } as const;
</script>

<aside class="bg-sidebar text-sidebar-foreground border-sidebar-border flex w-[200px] shrink-0 flex-col border-r">
  <nav aria-label="Main" class="flex-1 overflow-y-auto px-2 py-3">
    {#each NAV as group, gi (gi)}
      {#if group.label}
        <div class="eyebrow text-muted-foreground px-2 pt-4 pb-1.5">{group.label}</div>
      {:else if gi > 0}
        <div class="border-sidebar-border mx-2 my-3 border-t"></div>
      {/if}
      <ul class="flex flex-col gap-0.5">
        {#each group.items as item (item.id)}
          {@const Icon = icons[item.id]}
          <li>
            <button
              type="button"
              class="hover:bg-sidebar-accent focus-visible:ring-ring/50 flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-left outline-none focus-visible:ring-2
                {current === item.id ? 'bg-sidebar-accent text-sidebar-primary font-medium' : 'text-sidebar-foreground/85'}"
              aria-current={current === item.id ? 'page' : undefined}
              onclick={() => (current = item.id)}
            >
              <Icon class="size-4 shrink-0" aria-hidden="true" />
              <span class="flex-1 truncate">{item.label}</span>
              {#if item.milestone}
                <span class="text-muted-foreground mono text-[10px]" title="Arrives in {item.milestone}">{item.milestone}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/each}
  </nav>
  <div class="border-sidebar-border flex items-center gap-2.5 border-t px-3 py-3">
    <img src="/logo.png" alt="" class="size-8" width="32" height="32" />
    <div class="min-w-0">
      <div class="truncate text-[12px] font-semibold">StarOrg Strings</div>
      <div class="text-muted-foreground mono text-[11px]">{version ? `v${version}` : ''}</div>
    </div>
  </div>
</aside>
