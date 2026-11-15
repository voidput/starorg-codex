<script lang="ts">
  /**
   * Word diff of two game strings (spec 9.5). Removed words are struck through, added ones
   * underlined, each also marked for screen readers; text nodes only, no HTML from strings.
   */
  let { before, after }: { before: string; after: string } = $props();

  type Part = { t: string; op: 'same' | 'add' | 'del' };
  const tok = (s: string) => s.match(/\\n|\s+|[^\s\\]+|\\/g) ?? [];

  function diff(a: string[], b: string[]): Part[] {
    // LCS on tokens; long strings fall back to a whole replace to keep it O(n·m) small.
    if (a.length * b.length > 250_000) return [...a.map((t) => ({ t, op: 'del' as const })), ...b.map((t) => ({ t, op: 'add' as const }))];
    const m = a.length;
    const n = b.length;
    const dp: Uint16Array[] = Array.from({ length: m + 1 }, () => new Uint16Array(n + 1));
    for (let i = m - 1; i >= 0; i--) for (let j = n - 1; j >= 0; j--) dp[i]![j] = a[i] === b[j] ? dp[i + 1]![j + 1]! + 1 : Math.max(dp[i + 1]![j]!, dp[i]![j + 1]!);
    const out: Part[] = [];
    let i = 0;
    let j = 0;
    const push = (t: string, op: Part['op']) => {
      const last = out.at(-1);
      if (last && last.op === op) last.t += t;
      else out.push({ t, op });
    };
    while (i < m && j < n) {
      if (a[i] === b[j]) {
        push(a[i]!, 'same');
        i++;
        j++;
      } else if (dp[i + 1]![j]! >= dp[i]![j + 1]!) push(a[i++]!, 'del');
      else push(b[j++]!, 'add');
    }
    while (i < m) push(a[i++]!, 'del');
    while (j < n) push(b[j++]!, 'add');
    return out;
  }
  const parts = $derived(diff(tok(before), tok(after)));
  const changed = $derived(parts.some((p) => p.op !== 'same'));
</script>

<div class="bg-background/60 mono rounded-md border p-3 leading-relaxed break-words whitespace-pre-wrap">
  {#if !changed}<span class="text-muted-foreground">No change: the result is the stock text.</span>{:else}
    {#each parts as p, i (i)}
      {#if p.op === 'same'}{p.t}{:else if p.op === 'del'}<del class="mx-px bg-destructive/15 text-destructive decoration-destructive/70 rounded-sm"><span class="sr-only">removed: </span>{p.t}</del>{:else}<ins class="mx-px bg-success/15 text-success rounded-sm underline decoration-1 underline-offset-2"><span class="sr-only">added: </span>{p.t}</ins>{/if}
    {/each}
  {/if}
</div>
