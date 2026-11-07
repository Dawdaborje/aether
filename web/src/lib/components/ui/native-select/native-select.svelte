<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { cn, type WithElementRef } from '$lib/utils.js';
	import type { HTMLSelectAttributes } from 'svelte/elements';

	type NativeSelectProps = Omit<WithElementRef<HTMLSelectAttributes>, 'size'> & {
		size?: 'sm' | 'default';
	};

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		size = 'default',
		children,
		...restProps
	}: NativeSelectProps = $props();
</script>

<div
	class={cn(
		'cn-native-select-wrapper group/native-select relative w-fit has-[select:disabled]:opacity-50',
		className
	)}
	data-slot="native-select-wrapper"
	data-size={size}
>
	<select
		bind:value
		bind:this={ref}
		data-slot="native-select"
		data-size={size}
		class="h-10 w-full min-w-0 appearance-none rounded-field border-0 border-b border-border-strong bg-field pr-10 pl-4 text-sm text-foreground transition-[background-color,border-color] select-none placeholder:text-muted-foreground hover:bg-field-hover disabled:pointer-events-none disabled:cursor-not-allowed disabled:border-transparent disabled:opacity-50 aria-invalid:outline-2 aria-invalid:-outline-offset-2 aria-invalid:outline-destructive data-[size=sm]:h-8"
		{...restProps}
	>
		{@render children?.()}
	</select>
	<AppIcon
		name="chevron-down"
		class="pointer-events-none absolute top-1/2 right-4 size-4 -translate-y-1/2 text-foreground select-none"
		aria-hidden
		data-slot="native-select-icon"
	/>
</div>
