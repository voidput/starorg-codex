<script lang="ts">
  /**
   * Renders raw game text like mobiGlas does: `\n` escapes become line breaks, <EM3> underlined,
   * <EM4> coloured. Built from text nodes and spans in code; never sets HTML from a string (spec 9.5).
   */
  import { cn } from '@/lib/utils';

  let { text, class: className = '' }: { text: string; class?: string } = $props();

  type Piece = { t: string; em: number | null };
  function parse(s: string): Piece[][] {
    return s.split('\\n').map((line) => {
      const out: Piece[] = [];
      const re = /<EM(\d)>(.*?)<\/EM\1>/g;
      let last = 0;
      for (const m of line.matchAll(re)) {
        if (m.index! > last) out.push({ t: line.slice(last, m.index), em: null });
        out.push({ t: m[2]!, em: Number(m[1]) });
        last = m.index! + m[0].length;
      }
      if (last < line.length) out.push({ t: line.slice(last), em: null });
      return out;
    });
  }
  const lines = $derived(parse(text));
</script>

<div class={cn('bg-background/60 rounded-md border p-3 font-mono text-[12px] leading-relaxed', className)}>
  {#each lines as line, i (i)}
    <div class="min-h-[1.4em] whitespace-pre-wrap">
      {#each line as p, j (j)}
        {#if p.em === 3}<span class="underline">{p.t}</span>{:else if p.em === 4}<span class="text-accent-2">{p.t}</span>{:else if p.em}<span class="font-semibold">{p.t}</span>{:else}{p.t}{/if}
      {/each}
    </div>
  {/each}
</div>
