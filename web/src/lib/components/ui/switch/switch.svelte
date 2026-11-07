<script lang="ts">
	import { Switch as SwitchPrimitive } from 'bits-ui';
	import { cn, type WithoutChildrenOrChild } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		checked = $bindable(false),
		size = 'default',
		...restProps
	}: WithoutChildrenOrChild<SwitchPrimitive.RootProps> & {
		size?: 'sm' | 'default';
	} = $props();
</script>

<SwitchPrimitive.Root
	bind:ref
	bind:checked
	data-slot="switch"
	data-size={size}
	class={cn(
		'peer group/switch relative inline-flex shrink-0 items-center rounded-full transition-all outline-none after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring aria-invalid:outline-2 aria-invalid:outline-destructive data-[size=default]:h-6 data-[size=default]:w-12 data-[size=sm]:h-4 data-[size=sm]:w-8 data-checked:bg-success data-unchecked:bg-border-strong data-disabled:cursor-not-allowed data-disabled:opacity-50',
		className
	)}
	{...restProps}
>
	<SwitchPrimitive.Thumb
		data-slot="switch-thumb"
		class={cn(
			'pointer-events-none block rounded-full bg-white ring-0 transition-transform group-data-[size=default]/switch:size-[18px] group-data-[size=sm]/switch:size-3 group-data-[size=default]/switch:data-checked:translate-x-[27px] group-data-[size=sm]/switch:data-checked:translate-x-[18px] data-unchecked:translate-x-[3px]',
			'rtl:data-[state=checked]:translate-x-[calc(-100%)]'
		)}
	/>
</SwitchPrimitive.Root>
