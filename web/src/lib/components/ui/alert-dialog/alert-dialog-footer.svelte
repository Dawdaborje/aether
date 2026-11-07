<script lang="ts">
	import { cn, type WithElementRef } from '$lib/utils.js';
	import type { HTMLAttributes } from 'svelte/elements';

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
	bind:this={ref}
	data-slot="alert-dialog-footer"
	class={cn(
		'cn-alert-dialog-footer -mx-4 mt-4 flex flex-col-reverse group-data-[size=sm]/alert-dialog-content:grid group-data-[size=sm]/alert-dialog-content:grid-cols-2 *:data-[slot=button]:h-16 *:data-[slot=button]:flex-1 *:data-[slot=button]:items-start *:data-[slot=button]:justify-start *:data-[slot=button]:pt-4 *:data-[slot=button]:pr-16 sm:flex-row',
		className
	)}
	{...restProps}
>
	{@render children?.()}
</div>
