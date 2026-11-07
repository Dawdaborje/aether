<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Select as SelectPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils.js';

	/**
	 * A select that looks like the rest of the app (shadcn's Select): a field-styled trigger and
	 * a floating, scrollable list with a check on the chosen row. Use it in place of the
	 * browser's `<select>`, whose popup cannot be styled.
	 */
	let {
		value = $bindable(''),
		options,
		id,
		placeholder = 'Select…',
		disabled = false,
		required = false,
		/** Offer an empty choice at the top ("None"), so a chosen value can be cleared. */
		clearLabel,
		class: className,
		onchange
	}: {
		value?: string;
		options: { value: string; label: string; disabled?: boolean }[];
		id?: string;
		placeholder?: string;
		disabled?: boolean;
		required?: boolean;
		clearLabel?: string;
		class?: string;
		onchange?: (value: string) => void;
	} = $props();

	// bits-ui does not take an empty string as a row value, so "nothing" is a marker.
	const NONE = '__none__';

	const shown = $derived(options.find((option) => option.value === value)?.label ?? '');
	const canClear = $derived(clearLabel !== undefined && !required);

	function change(next: string) {
		const real = next === NONE ? '' : next;
		value = real;
		onchange?.(real);
	}
</script>

<SelectPrimitive.Root
	type="single"
	value={value === '' ? undefined : value}
	{disabled}
	onValueChange={(next) => change(next ?? '')}
>
	<SelectPrimitive.Trigger
		{id}
		data-slot="select-trigger"
		data-placeholder={shown === '' ? '' : undefined}
		class={cn(
			'flex h-10 w-full min-w-0 items-center justify-between gap-2 rounded-field border-0 border-b border-border-strong bg-field px-4 text-left text-sm text-foreground transition-[background-color,border-color] hover:bg-field-hover disabled:cursor-not-allowed disabled:border-transparent disabled:opacity-50 aria-invalid:outline-2 aria-invalid:-outline-offset-2 aria-invalid:outline-destructive',
			'data-[placeholder]:text-muted-foreground data-[state=open]:outline-2 data-[state=open]:-outline-offset-2 data-[state=open]:outline-ring',
			className
		)}
	>
		<span class="truncate">{shown === '' ? placeholder : shown}</span>
		<AppIcon
			name="chevron-down"
			class="size-4 shrink-0 text-foreground transition-transform in-data-[state=open]:rotate-180"
		/>
	</SelectPrimitive.Trigger>
	<SelectPrimitive.Portal>
		<SelectPrimitive.Content
			data-slot="select-content"
			sideOffset={4}
			class="z-50 max-h-72 w-(--bits-select-anchor-width) min-w-36 origin-(--bits-select-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-menu bg-popover p-0 text-popover-foreground shadow-md ring-0 duration-short outline-none data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95"
		>
			<SelectPrimitive.Viewport>
				{#if canClear}
					<SelectPrimitive.Item
						value={NONE}
						label={clearLabel}
						class="relative flex h-10 w-full cursor-default items-center pr-10 pl-4 text-sm text-muted-foreground outline-none select-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground"
					>
						{clearLabel}
					</SelectPrimitive.Item>
				{/if}
				{#each options as option (option.value)}
					<SelectPrimitive.Item
						value={option.value}
						label={option.label}
						disabled={option.disabled}
						class="relative flex h-10 w-full cursor-default items-center gap-2 pr-10 pl-4 text-sm outline-none select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground data-[selected]:bg-layer-hover"
					>
						{#snippet children({ selected })}
							<span class="truncate">{option.label}</span>
							{#if selected}
								<span class="absolute right-4 flex size-4 items-center justify-center">
									<AppIcon name="check" class="size-4" />
								</span>
							{/if}
						{/snippet}
					</SelectPrimitive.Item>
				{/each}
			</SelectPrimitive.Viewport>
		</SelectPrimitive.Content>
	</SelectPrimitive.Portal>
</SelectPrimitive.Root>
