<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Command as CommandPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils.js';
	import * as InputGroup from '$lib/components/ui/input-group/index.js';

	let {
		ref = $bindable(null),
		class: className,
		value = $bindable(''),
		...restProps
	}: CommandPrimitive.InputProps = $props();
</script>

<div data-slot="command-input-wrapper" class="p-1">
	<InputGroup.Root class="border-transparent border-b-input bg-transparent px-3">
		<CommandPrimitive.Input
			{value}
			data-slot="command-input"
			class={cn(
				'w-full px-2 text-sm outline-hidden disabled:cursor-not-allowed disabled:opacity-50',
				className
			)}
			{...restProps}
		>
			{#snippet child({ props })}
				<InputGroup.Input {...props} bind:value bind:ref />
			{/snippet}
		</CommandPrimitive.Input>
		<InputGroup.Addon>
			<AppIcon name="search" class="size-3.5 shrink-0 opacity-50" />
		</InputGroup.Addon>
	</InputGroup.Root>
</div>
