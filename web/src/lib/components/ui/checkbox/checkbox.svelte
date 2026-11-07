<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Checkbox as CheckboxPrimitive } from 'bits-ui';
	import { cn, type WithoutChildrenOrChild } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		checked = $bindable(false),
		indeterminate = $bindable(false),
		class: className,
		...restProps
	}: WithoutChildrenOrChild<CheckboxPrimitive.RootProps> = $props();
</script>

<CheckboxPrimitive.Root
	bind:ref
	data-slot="checkbox"
	class={cn(
		'peer relative flex size-4 shrink-0 items-center justify-center rounded-[2px] border border-foreground bg-transparent transition-colors outline-none group-has-disabled/field:opacity-50 group-has-[:focus-visible]/field-label:ring-0 group-has-[:focus-visible]/field-label:not-data-checked:border-input after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:outline-2 aria-invalid:outline-destructive aria-invalid:aria-checked:border-foreground dark:aria-invalid:border-destructive/50 data-checked:border-foreground data-checked:bg-foreground data-checked:text-background group-has-[:focus-visible]/field-label:data-checked:border-primary',
		className
	)}
	bind:checked
	bind:indeterminate
	{...restProps}
>
	{#snippet children({ checked, indeterminate })}
		<div
			data-slot="checkbox-indicator"
			class="grid place-content-center text-current transition-none [&>svg]:size-3.5"
		>
			{#if checked}
				<AppIcon name="check" />
			{:else if indeterminate}
				<AppIcon name="minus" />
			{/if}
		</div>
	{/snippet}
</CheckboxPrimitive.Root>
