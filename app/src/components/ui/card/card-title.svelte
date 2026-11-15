<script lang="ts">
	import ChevronRight from "@lucide/svelte/icons/chevron-right";
	import { cn, type WithElementRef } from "@/lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";
	import { arrowToggle } from "@/lib/collapse";
	import { getCardCollapse } from "./collapse";

	let {
		ref = $bindable(null),
		class: className,
		level = 2,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLElement>> & {
		/** Heading level: the app header renders the page <h1>, so a card title defaults to <h2>; 0 = plain div. */
		level?: 0 | 2 | 3 | 4;
	} = $props();
	// In a CollapsibleCard the title is the disclosure button (Enter/Space toggle, Left/Right
	// collapse/expand), inside the heading as in the ARIA accordion pattern.
	const collapse = getCardCollapse();
	function onkeydown(e: KeyboardEvent) {
		const to = collapse ? arrowToggle(e.key, collapse.open) : null;
		if (to === null || !collapse) return;
		e.preventDefault();
		collapse.set(to);
	}
</script>

<svelte:element
	this={level ? `h${level}` : "div"}
	bind:this={ref}
	data-slot="card-title"
	class={cn("m-0 text-lg leading-snug font-semibold group-data-[size=sm]/card:text-sm", className)}
	{...restProps}
>
	{#if collapse}
		<button
			type="button"
			data-slot="card-toggle"
			class="focus-visible:ring-ring/60 -mx-1 inline-flex max-w-full items-center gap-2 rounded-md px-1 text-left outline-none focus-visible:ring-2"
			aria-expanded={collapse.open}
			aria-controls={collapse.contentId}
			title={collapse.open ? "Collapse" : "Expand"}
			onclick={() => collapse.set(!collapse.open)}
			{onkeydown}
		>
			<ChevronRight class={cn("text-muted-foreground size-4 shrink-0 transition-transform motion-reduce:transition-none", collapse.open && "rotate-90")} aria-hidden="true" />
			{@render children?.()}
		</button>
	{:else}
		{@render children?.()}
	{/if}
</svelte:element>
