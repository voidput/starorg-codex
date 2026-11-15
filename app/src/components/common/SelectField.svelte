<script lang="ts" generics="V extends string">
  /** A labelled single select over a few string options (shadcn Select). */
  import * as Select from '@/components/ui/select';
  import { cn } from '@/lib/utils';

  let {
    options,
    value,
    onchange,
    ariaLabel,
    id = undefined,
    placeholder = 'Choose…',
    disabled = false,
    size = 'sm',
    class: className = '',
  }: {
    options: { id: V; label: string }[];
    value: V | null;
    onchange: (v: V) => void;
    ariaLabel?: string;
    id?: string;
    placeholder?: string;
    disabled?: boolean;
    size?: 'sm' | 'default';
    class?: string;
  } = $props();
  const current = $derived(options.find((o) => o.id === value));
</script>

<!-- Controlled: the shown value is always `value`, so a pick the parent turns down (or asks
     about first) can be picked again. -->
<Select.Root type="single" bind:value={() => value ?? '', (v) => { if (v && v !== value) onchange(v as V); }} {disabled}>
  <Select.Trigger {id} {size} aria-label={ariaLabel} class={cn('min-w-28', className)}>
    <span class={cn('truncate', !current && 'text-muted-foreground')}>{current?.label ?? placeholder}</span>
  </Select.Trigger>
  <Select.Content>
    {#each options as o (o.id)}
      <Select.Item value={o.id} label={o.label}>{o.label}</Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
