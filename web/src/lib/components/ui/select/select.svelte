<script lang="ts">
	import { Select as SelectPrimitive } from 'bits-ui';
	import CheckIcon from '@lucide/svelte/icons/check';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
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
			'border-input bg-card shadow-xs focus-visible:border-ring focus-visible:ring-ring/20 aria-invalid:border-destructive flex h-9 w-full min-w-0 items-center justify-between gap-2 rounded-md border px-3 py-1 text-left text-base transition-[color,border-color] outline-none focus-visible:ring-[3px] disabled:cursor-not-allowed disabled:opacity-50 md:text-sm',
			'data-[placeholder]:text-muted-foreground data-[state=open]:border-ring',
			className
		)}
	>
		<span class="truncate">{shown === '' ? placeholder : shown}</span>
		<ChevronDownIcon class="text-muted-foreground size-4 shrink-0" />
	</SelectPrimitive.Trigger>
	<SelectPrimitive.Portal>
		<SelectPrimitive.Content
			data-slot="select-content"
			sideOffset={4}
			class="bg-popover text-popover-foreground ring-foreground/10 data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 data-closed:zoom-out-95 data-open:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2 z-50 max-h-72 w-(--bits-select-anchor-width) min-w-36 origin-(--bits-select-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-md p-1.5 shadow-md ring-1 duration-100 outline-none"
		>
			<SelectPrimitive.Viewport>
				{#if canClear}
					<SelectPrimitive.Item
						value={NONE}
						label={clearLabel}
						class="data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground text-muted-foreground relative flex w-full cursor-default items-center rounded-sm py-1.5 pr-8 pl-2 text-sm outline-none select-none"
					>
						{clearLabel}
					</SelectPrimitive.Item>
				{/if}
				{#each options as option (option.value)}
					<SelectPrimitive.Item
						value={option.value}
						label={option.label}
						disabled={option.disabled}
						class="data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground relative flex w-full cursor-default items-center gap-2 rounded-sm py-1.5 pr-8 pl-2 text-sm outline-none select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50"
					>
						{#snippet children({ selected })}
							<span class="truncate">{option.label}</span>
							{#if selected}
								<span class="absolute right-2 flex size-4 items-center justify-center">
									<CheckIcon class="size-4" />
								</span>
							{/if}
						{/snippet}
					</SelectPrimitive.Item>
				{/each}
			</SelectPrimitive.Viewport>
		</SelectPrimitive.Content>
	</SelectPrimitive.Portal>
</SelectPrimitive.Root>
