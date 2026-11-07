<script lang="ts">
	import type { HTMLInputAttributes, HTMLInputTypeAttribute } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils.js';

	type InputType = Exclude<HTMLInputTypeAttribute, 'file'>;

	type Props = WithElementRef<
		Omit<HTMLInputAttributes, 'type'> &
			({ type: 'file'; files?: FileList } | { type?: InputType; files?: undefined })
	>;

	let {
		ref = $bindable(null),
		value = $bindable(),
		type,
		files = $bindable(),
		class: className,
		'data-slot': dataSlot = 'input',
		...restProps
	}: Props = $props();
</script>

{#if type === 'file'}
	<input
		bind:this={ref}
		data-slot={dataSlot}
		class={cn(
			'h-10 w-full min-w-0 rounded-field border-0 border-b border-border-strong bg-field px-4 py-0 text-sm text-foreground transition-[background-color,border-color] file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground hover:bg-field-hover disabled:pointer-events-none disabled:cursor-not-allowed disabled:border-transparent disabled:opacity-50 aria-invalid:outline-2 aria-invalid:-outline-offset-2 aria-invalid:outline-destructive',
			className
		)}
		type="file"
		bind:files
		bind:value
		{...restProps}
	/>
{:else}
	<input
		bind:this={ref}
		data-slot={dataSlot}
		class={cn(
			'h-10 w-full min-w-0 rounded-field border-0 border-b border-border-strong bg-field px-4 py-0 text-sm text-foreground transition-[background-color,border-color] file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground hover:bg-field-hover disabled:pointer-events-none disabled:cursor-not-allowed disabled:border-transparent disabled:opacity-50 aria-invalid:outline-2 aria-invalid:-outline-offset-2 aria-invalid:outline-destructive',
			className
		)}
		{type}
		bind:value
		{...restProps}
	/>
{/if}
