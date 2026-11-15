<script lang="ts">
  /**
   * Renders raw game text like mobiGlas does: `\n` escapes become line breaks, <EM3> underlined,
   * <EM4> in the game's highlight blue (both seen in game). <EM1>, <EM2> and <EM5> exist in stock
   * but their look is not verified in game: they get a neutral dashed highlight and a note.
   * Built from text nodes and spans in code; never sets HTML from a string (spec 9.5).
   */
  import { cn } from '@/lib/utils';

  let { text, class: className = '', note = true }: { text: string; class?: string; /** Say which tags are guesses. */ note?: boolean } = $props();

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
  /** Tag numbers shown whose in-game look is a guess. */
  const unverified = $derived([...new Set(lines.flat().flatMap((p) => (p.em && p.em !== 3 && p.em !== 4 ? [p.em] : [])))].sort());
</script>

<div class={cn('bg-background/60 min-w-0 rounded-md border p-3 font-mono text-[12px] leading-relaxed [overflow-wrap:anywhere]', className)}>
  {#each lines as line, i (i)}
    <div class="min-h-[1.4em] whitespace-pre-wrap">
      {#each line as p, j (j)}
        {#if p.em === 3}<span class="underline underline-offset-2">{p.t}</span>{:else if p.em === 4}<span class="text-em4">{p.t}</span>{:else if p.em}<span class="bg-muted rounded-sm outline-1 outline-muted-foreground/60 outline-dashed" title="EM{p.em}: colour not verified in game">{p.t}</span>{:else}{p.t}{/if}
      {/each}
    </div>
  {/each}
  {#if note && unverified.length}
    <p class="text-muted-foreground mt-1.5 border-t pt-1.5 font-sans text-[11px]">
      Dashed: {unverified.map((n) => `EM${n}`).join(', ')}, colour not verified in game. The Colour test under General shows them in the game.
    </p>
  {/if}
</div>
