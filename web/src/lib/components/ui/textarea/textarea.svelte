<script lang="ts">
	import { cn, type WithElementRef, type WithoutChildren } from '$lib/utils.js';
	import type { HTMLTextareaAttributes } from 'svelte/elements';

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		'data-slot': dataSlot = 'textarea',
		...restProps
	}: WithoutChildren<WithElementRef<HTMLTextareaAttributes>> = $props();
</script>

<textarea
	bind:this={ref}
	data-slot={dataSlot}
	class={cn(
		'flex field-sizing-content min-h-16 w-full min-w-0 resize-y rounded-field border-0 border-b border-border-strong bg-field px-4 py-[11px] text-sm text-foreground transition-[background-color,border-color] placeholder:text-muted-foreground hover:bg-field-hover disabled:pointer-events-none disabled:cursor-not-allowed disabled:border-transparent disabled:opacity-50 aria-invalid:outline-2 aria-invalid:-outline-offset-2 aria-invalid:outline-destructive',
		className
	)}
	bind:value
	{...restProps}
></textarea>
