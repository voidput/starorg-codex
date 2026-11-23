<script lang="ts">
  /**
   * One key's detail (spec 9.4 Strings): rendered preview, word diff, provenance (winner and the
   * layers it beat), and the override editor with live rule checks (spec 7.3) and Reset.
   * Save stays off while a live check fails; the core's own refusal names the rule in words.
   * Closing with unsaved text asks first.
   */
  import { untrack } from 'svelte';
  import Crown from '@lucide/svelte/icons/crown';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Sheet from '@/components/common/Sheet.svelte';
  import ToneBadge from '@/components/common/ToneBadge.svelte';
  import InGamePreview from '@/components/common/InGamePreview.svelte';
  import StringDiff from '@/components/common/StringDiff.svelte';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import * as Alert from '@/components/ui/alert';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, Layer, StringDetail, StringRow } from '@/lib/contract/types';
  import { CATEGORY_LABEL, SOURCE_LABEL, SOURCE_TONE } from '@/lib/format';
  import { checkOverride } from '@/lib/validate';
  import { ownerContext, ownerLabel } from '@/lib/strowner';
  import { ruleText } from '@/lib/messages';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import { type ContractError } from '@/lib/contract/client';
  import { toast } from '@/lib/stores/toast.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  let {
    open = $bindable(false),
    target,
    rowKey,
    onchanged,
  }: { open: boolean; target: ChannelRef | null; rowKey: string | null; onchanged: (row: StringRow) => void } = $props();

  let detail = $state<StringDetail | null>(null);
  let value = $state('');
  let rejected = $state<{ rule: string | null; message: string | null } | null>(null);
  let busy = $state(false);
  let loadError = $state<ContractError | null>(null);
  let retry = $state(0);
  let askClose = $state(false);
  /** The detail lists this many owners of a shared key until "Show all". */
  const OWNERS_SHOWN = 8;
  let allOwners = $state(false);

  // Keyed on primitives only (the channel key, the row key): `target` is a new object after every
  // app.refresh(), which must not reset an open sheet or refetch it.
  $effect(() => {
    const k = rowKey;
    const tk = app.targetKey;
    void retry;
    if (!open || !k || !tk) return;
    const t = untrack(() => (target ? $state.snapshot(target) : null));
    if (!t) return;
    let stale = false;
    detail = null;
    allOwners = false;
    rejected = null;
    loadError = null;
    askClose = false;
    api
      .stringDetail({ target: t, key: k })
      .then((d) => {
        if (stale) return;
        detail = d;
        value = d.override_value ?? d.row.result;
      })
      .catch((e) => !stale && (loadError = toContractError(e)));
    return () => (stale = true);
  });

  /** Unsaved text: the first close attempt asks instead of dropping it. */
  function beforeClose(): boolean {
    if (!changed || askClose) return true;
    askClose = true;
    return false;
  }

  const stock = $derived(detail?.provenance.find((p) => p.layer === 'stock')?.text ?? detail?.row.original ?? '');
  const checks = $derived(detail ? checkOverride(stock, value) : []);
  const failing = $derived(checks.filter((c) => !c.ok));
  const changed = $derived(detail !== null && value !== (detail.override_value ?? detail.row.result));
  const layerLabel: Record<Layer, string> = { stock: 'Stock', generated: 'Enhancement', patch: 'Patch', override: 'Your edit' };

  async function save() {
    if (!detail || !target) return;
    busy = true;
    try {
      const r = await api.setOverride({ target, key: detail.row.key, value });
      if (!r.accepted) {
        const t = ruleText(r.rule);
        rejected = { rule: r.rule, message: r.message ?? t.message };
        return;
      }
      rejected = null;
      if (r.detail) {
        detail = r.detail;
        value = r.detail.override_value ?? r.detail.row.result;
        onchanged(r.detail.row);
      }
      toast('success', 'Edit saved', 'It is written on the next apply.');
    } catch (e) {
      toast('error', 'Edit not saved', toContractError(e).message);
    } finally {
      busy = false;
    }
  }

  async function reset() {
    if (!detail || !target) return;
    try {
      const d = await api.resetOverride({ target, key: detail.row.key });
      detail = d;
      value = d.row.result;
      rejected = null;
      onchanged(d.row);
      toast('success', 'Edit removed', 'The key goes back to what StarOrg Codex builds.');
    } catch (e) {
      toast('error', 'Reset failed', toContractError(e).message);
    }
  }
  /** "Ship components › Coolers · Aegis · Size 2" (group, then the facets the key has). */
  function groupLine(d: StringDetail): string {
    const parts = [d.row.group_label || (CATEGORY_LABEL[d.row.category] ?? d.row.category)];
    if (d.row.manufacturer) parts.push(d.row.manufacturer);
    if (d.row.size !== null) parts.push(`Size ${d.row.size}`);
    return parts.join(' · ');
  }
</script>

<Sheet bind:open title={rowKey ?? ''} description={detail ? groupLine(detail) : null} {beforeClose}>
  {#if loadError}
    <ErrorState title={`Could not open ${rowKey ?? 'this key'}`} error={loadError} onretry={() => retry++} />
  {:else if !detail}
    <div class="grid gap-3"><Skeleton class="h-20 w-full" /><Skeleton class="h-32 w-full" /></div>
  {:else}
    {@const d = detail}
    <div class="grid gap-5">
      <section aria-labelledby="sd-owner" class="grid gap-1.5">
        <h3 id="sd-owner" class="eyebrow text-muted-foreground">{d.owners.length > 1 ? `Used by ${d.owners.length} records` : 'What this is'}</h3>
        <ul class="grid gap-1">
          {#each d.owners.length ? (allOwners ? d.owners : d.owners.slice(0, OWNERS_SHOWN)) : [d.row.owner] as o, i (i)}
            <li class="leading-snug">
              <span class={cn(o?.kind === 'key' && 'text-muted-foreground')}>{ownerLabel(o, d.row.key)}</span>
              {#if ownerContext(o)}<span class="text-muted-foreground text-[12px]"> — {ownerContext(o)}</span>{/if}
            </li>
          {/each}
        </ul>
        {#if d.owners.length > OWNERS_SHOWN && !allOwners}
          <Button variant="link" size="xs" class="h-auto w-fit p-0" onclick={() => (allOwners = true)}>Show all {d.owners.length}</Button>
        {/if}
        {#if !d.owners.length}<p class="text-muted-foreground text-[12px]">No record uses this key; this is the key made readable.</p>{/if}
      </section>
      <section aria-labelledby="sd-result" class="grid gap-1.5">
        <div class="flex items-center gap-2">
          <h3 id="sd-result" class="eyebrow text-muted-foreground">In game</h3>
          <ToneBadge tone={SOURCE_TONE[d.row.source]} label={SOURCE_LABEL[d.row.source]} />
        </div>
        <InGamePreview text={d.row.result} />
        {#if d.also_used_by.length}
          <p class="text-muted-foreground text-[12px]">Also used by: {d.also_used_by.join(', ')}</p>
        {/if}
      </section>
      <section aria-labelledby="sd-diff" class="grid gap-1.5">
        <h3 id="sd-diff" class="eyebrow text-muted-foreground">Changes from stock</h3>
        <StringDiff before={stock} after={d.row.result} />
      </section>
      <section aria-labelledby="sd-prov" class="grid gap-1.5">
        <h3 id="sd-prov" class="eyebrow text-muted-foreground">Where the text comes from</h3>
        <ol class="grid gap-1.5">
          {#each [...d.provenance].reverse() as p, i (i)}
            <li class={cn('rounded-md border px-3 py-2', p.winner && 'border-primary/50 bg-primary/5')}>
              <div class="flex flex-wrap items-center gap-2">
                <span class="font-semibold">{layerLabel[p.layer]}</span>
                <span class="mono text-muted-foreground text-[11.5px]">{p.by}</span>
                {#if p.winner}<ToneBadge tone="info" icon={Crown} label="Shown in game" />{:else}<ToneBadge tone="muted" label="Overridden" />{/if}
              </div>
              <p class="mono mt-1 line-clamp-3 text-[12px] break-words">{p.text}</p>
              {#if p.note}<p class="text-muted-foreground mt-1 text-[12px]">{p.note}</p>{/if}
            </li>
          {/each}
        </ol>
      </section>
      <section aria-labelledby="sd-edit" class="grid gap-2">
        <div class="flex items-center justify-between gap-2">
          <h3 id="sd-edit" class="eyebrow text-muted-foreground">Your edit</h3>
          {#if d.override_value !== null}<ToneBadge tone="info" label="Edited" />{/if}
        </div>
        <p class="text-muted-foreground text-[12px]" id="sd-pin">
          Your edit replaces this key's text as it is. Stats StarOrg Codex builds into it stop updating after a game patch.
          {#if value !== stock}<button type="button" class="text-primary underline-offset-2 hover:underline" onclick={() => { value = stock; rejected = null; }}>Start from stock text</button>{/if}
        </p>
        <label for="sd-value" class="text-muted-foreground text-[12px]">Raw text as in global.ini. Write a line break as \n; tags as &lt;EM4&gt;…&lt;/EM4&gt;.</label>
        <textarea
          id="sd-value"
          bind:value
          oninput={() => (rejected = null)}
          rows="5"
          spellcheck="false"
          aria-invalid={failing.length > 0 || rejected !== null}
          aria-describedby="sd-pin sd-checks{rejected ? ' sd-rejected' : ''}"
          class="mono border-input bg-background focus-visible:ring-ring/50 aria-invalid:border-destructive w-full resize-y rounded-md border px-3 py-2 outline-none focus-visible:ring-3"
        ></textarea>
        <ul id="sd-checks" class="grid gap-1 sm:grid-cols-2" aria-label="Rule checks">
          {#each checks as c (c.rule)}
            <li class={cn('flex items-start gap-1.5 text-[12px]', c.ok ? 'text-muted-foreground' : 'text-destructive')}>
              {#if c.ok}<CircleCheck class="text-success mt-px size-3.5 shrink-0" aria-hidden="true" />{:else}<CircleX class="mt-px size-3.5 shrink-0" aria-hidden="true" />{/if}
              <span>{#if c.ok}{c.check}<span class="sr-only">: ok</span>{:else}<span class="font-medium">{c.label}.</span>{' '}{c.message}{/if}</span>
            </li>
          {/each}
        </ul>
        {#if rejected}
          <Alert.Root variant="destructive" role="alert" id="sd-rejected">
            <CircleAlert aria-hidden="true" />
            <Alert.Title>Not saved: {ruleText(rejected.rule).label}</Alert.Title>
            <Alert.Description>
              {rejected.message}
              {#if rejected.rule}<span class="mono text-muted-foreground mt-0.5 block text-[11.5px]">rule {rejected.rule}</span>{/if}
            </Alert.Description>
          </Alert.Root>
        {/if}
        {#if askClose && changed}
          <Alert.Root variant="warning" role="alert">
            <CircleAlert aria-hidden="true" />
            <Alert.Title>Your edit is not saved</Alert.Title>
            <Alert.Description>
              <div class="mt-1 flex flex-wrap gap-2">
                <Button size="xs" variant="outline" onclick={() => { askClose = false; }}>Keep editing</Button>
                <Button size="xs" variant="destructive" onclick={() => { value = d.override_value ?? d.row.result; open = false; }}>Discard and close</Button>
              </div>
            </Alert.Description>
          </Alert.Root>
        {/if}
        <div class="flex flex-wrap items-center gap-2">
          <Button size="sm" disabled={!changed || busy || failing.length > 0} aria-describedby={failing.length ? 'sd-save-why' : undefined} onclick={save}>Save edit</Button>
          {#if changed && failing.length}<span id="sd-save-why" class="text-destructive text-[12px]">Fix {failing.length === 1 ? 'the rule' : `the ${failing.length} rules`} marked above to save.</span>{/if}
          <Button variant="ghost" size="sm" disabled={!changed} onclick={() => (value = d.override_value ?? d.row.result)}>Discard</Button>
          <span class="flex-1"></span>
          {#if d.override_value !== null}
            <ConfirmButton variant="outline" size="sm" confirmLabel="Remove edit? Click again" onconfirm={reset}>Reset to built text</ConfirmButton>
          {/if}
        </div>
      </section>
    </div>
  {/if}
</Sheet>
