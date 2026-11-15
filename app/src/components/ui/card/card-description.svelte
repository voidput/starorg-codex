<script lang="ts">
	import { cn, type WithElementRef } from "@/lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";
	import { getCardCollapse } from "./collapse";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLParagraphElement>> = $props();
	const collapse = getCardCollapse();
</script>

<p
	bind:this={ref}
	data-slot="card-description"
	hidden={collapse ? !collapse.open : undefined}
	class={cn("text-muted-foreground text-sm", className)}
	{...restProps}
>
	{@render children?.()}
</p>
