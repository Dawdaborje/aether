<script lang="ts">
	import { cn, type WithElementRef } from '$lib/utils.js';
	import type { HTMLAttributes } from 'svelte/elements';
	import { Dialog as DialogPrimitive } from 'bits-ui';
	import { Button } from '$lib/components/ui/button/index.js';

	let {
		ref = $bindable(null),
		class: className,
		children,
		showCloseButton = false,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		showCloseButton?: boolean;
	} = $props();
</script>

<div
	bind:this={ref}
	data-slot="dialog-footer"
	class={cn(
		'-mx-4 mt-4 flex flex-col-reverse *:data-[slot=button]:h-16 *:data-[slot=button]:flex-1 *:data-[slot=button]:items-start *:data-[slot=button]:justify-start *:data-[slot=button]:pt-4 *:data-[slot=button]:pr-16 sm:flex-row',
		className
	)}
	{...restProps}
>
	{@render children?.()}
	{#if showCloseButton}
		<DialogPrimitive.Close>
			{#snippet child({ props })}
				<Button variant="outline" {...props}>Close</Button>
			{/snippet}
		</DialogPrimitive.Close>
	{/if}
</div>
