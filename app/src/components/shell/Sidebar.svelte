<script lang="ts">
  /**
   * Sidebar (spec 9.3): grouped nav, collapses to icons, resizable 168-384 px by drag or arrow keys
   * on the handle. Arrow Up/Down move between items (roving focus); Alt+1..7 jump from anywhere (App).
   */
  import House from '@lucide/svelte/icons/house';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import Table2 from '@lucide/svelte/icons/table-2';
  import Wrench from '@lucide/svelte/icons/wrench';
  import Archive from '@lucide/svelte/icons/archive';
  import BookOpenCheck from '@lucide/svelte/icons/book-open-check';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import Settings from '@lucide/svelte/icons/settings';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open';
  import { Button } from '@/components/ui/button';
  import { NAV, NAV_ITEMS, type ScreenId } from '@/lib/nav';
  import { cn } from '@/lib/utils';

  let { current = $bindable<ScreenId>('home'), version = '' }: { current: ScreenId; version?: string } = $props();

  const icons = { home: House, enhancements: Sparkles, strings: Table2, patches: Wrench, blueprints: BookOpenCheck, backups: Archive, activity: ScrollText, settings: Settings } as const;
  const MIN = 168;
  const MAX = 384;
  const KEY = 'starorg:sidebar';

  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(KEY) ?? 'null') as { w: number; collapsed: boolean } | null;
    } catch {
      return null;
    }
  })();
  let width = $state(saved?.w ?? 208);
  let collapsed = $state(saved?.collapsed ?? false);
  $effect(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify({ w: width, collapsed }));
    } catch {
      /* fine */
    }
  });

  let nav = $state<HTMLElement | null>(null);
  function onNavKey(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Home' && e.key !== 'End') return;
    const items = [...(nav?.querySelectorAll<HTMLButtonElement>('[data-nav-item]') ?? [])];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    const n = items.length;
    const j = e.key === 'Home' ? 0 : e.key === 'End' ? n - 1 : (i + (e.key === 'ArrowDown' ? 1 : -1) + n) % n;
    items[j]?.focus();
    e.preventDefault();
  }

  let dragging = false;
  function startDrag(e: PointerEvent) {
    dragging = true;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }
  function drag(e: PointerEvent) {
    if (dragging) width = Math.min(MAX, Math.max(MIN, e.clientX));
  }
  function handleKey(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') width = Math.max(MIN, width - 16);
    else if (e.key === 'ArrowRight') width = Math.min(MAX, width + 16);
    else return;
    e.preventDefault();
  }
</script>

<aside
  class="bg-sidebar text-sidebar-foreground border-sidebar-border relative flex shrink-0 flex-col border-r"
  style="width: {collapsed ? 52 : width}px"
  aria-label="Main"
>
  <div class={cn('flex h-12 items-center gap-2 px-3', collapsed && 'justify-center px-0')}>
    {#if !collapsed}<span class="eyebrow text-primary flex-1 truncate">StarOrg Strings</span>{/if}
    <Button variant="ghost" size="icon-sm" aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'} aria-expanded={!collapsed} onclick={() => (collapsed = !collapsed)}>
      {#if collapsed}<PanelLeftOpen aria-hidden="true" />{:else}<PanelLeftClose aria-hidden="true" />{/if}
    </Button>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav bind:this={nav} class="flex flex-1 flex-col gap-3 overflow-y-auto px-2 pb-3" aria-label="Screens" onkeydown={onNavKey}>
    {#each NAV as group, gi (gi)}
      <div class="flex flex-col gap-0.5">
        {#if group.label && !collapsed}
          <div class="eyebrow text-muted-foreground px-2 pt-1 pb-1">{group.label}</div>
        {:else if group.label}
          <div class="bg-sidebar-border mx-2 my-1 h-px" aria-hidden="true"></div>
        {/if}
        <ul class="flex flex-col gap-0.5">
          {#each group.items as item (item.id)}
            {@const Icon = icons[item.id]}
            {@const active = current === item.id}
            <li>
              <button
                type="button"
                data-nav-item
                tabindex={active ? 0 : -1}
                class={cn(
                  'hover:bg-sidebar-accent focus-visible:ring-ring/60 flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-left outline-none focus-visible:ring-2',
                  active ? 'bg-sidebar-accent text-sidebar-primary font-medium' : 'text-sidebar-foreground/85',
                  collapsed && 'justify-center px-0',
                )}
                aria-current={active ? 'page' : undefined}
                aria-label={collapsed ? item.label : undefined}
                title={collapsed ? `${item.label} (Alt+${item.key})` : undefined}
                onclick={() => (current = item.id)}
              >
                <Icon class="size-4 shrink-0" aria-hidden="true" />
                {#if !collapsed}<span class="flex-1 truncate">{item.label}</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
  </nav>
  <div class={cn('border-sidebar-border flex items-center gap-2.5 border-t px-3 py-3', collapsed && 'justify-center px-0')}>
    <img src="/logo.png" alt="" class="size-7" width="28" height="28" />
    {#if !collapsed}
      <div class="min-w-0">
        <div class="truncate text-[12px] font-semibold">StarOrg Strings</div>
        <div class="text-muted-foreground mono text-[11px]">{version ? `v${version}` : ''}</div>
      </div>
    {/if}
  </div>
  {#if !collapsed}
    <!-- A focusable separator is a widget (WAI-ARIA window splitter); Svelte does not know it. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      aria-valuemin={MIN}
      aria-valuemax={MAX}
      aria-valuenow={width}
      tabindex="0"
      class="hover:bg-primary/40 focus-visible:bg-primary/60 absolute top-0 -right-[3px] z-10 h-full w-[6px] cursor-col-resize outline-none"
      onpointerdown={startDrag}
      onpointermove={drag}
      onpointerup={() => (dragging = false)}
      onkeydown={handleKey}
    ></div>
  {/if}
</aside>

<span class="sr-only" aria-live="polite">{NAV_ITEMS.find((i) => i.id === current)?.label}</span>
