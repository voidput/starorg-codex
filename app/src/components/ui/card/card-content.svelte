<script lang="ts">
	import { cn, type WithElementRef } from "@/lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";
	import { getCardCollapse } from "./collapse";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
	const collapse = getCardCollapse();
</script>

<div
	bind:this={ref}
	data-slot="card-content"
	id={collapse?.contentId}
	hidden={collapse ? !collapse.open : undefined}
	class={cn("px-(--card-spacing)", className)}
	{...restProps}
>
	{@render children?.()}
</div>
