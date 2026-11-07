<script lang="ts">
	import { Tabs as TabsPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: TabsPrimitive.TriggerProps = $props();
</script>

<TabsPrimitive.Trigger
	bind:ref
	data-slot="tabs-trigger"
	class={cn(
		"relative inline-flex h-full flex-1 items-center justify-start gap-2 px-4 text-sm font-normal tracking-[0.16px] whitespace-nowrap text-muted-foreground transition-colors group-data-vertical/tabs:w-full group-data-vertical/tabs:px-4 group-data-vertical/tabs:py-3 hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
		'group-data-[variant=default]/tabs-list:border-l group-data-[variant=default]/tabs-list:border-border-strong group-data-[variant=default]/tabs-list:bg-muted group-data-[variant=default]/tabs-list:first:border-l-0 group-data-[variant=default]/tabs-list:hover:bg-layer-hover',
		'data-active:font-semibold data-active:text-foreground group-data-[variant=default]/tabs-list:data-active:border-l-transparent group-data-[variant=default]/tabs-list:data-active:bg-background',
		'after:absolute after:bg-primary after:opacity-0 group-data-[variant=default]/tabs-list:after:top-0 group-data-[variant=line]/tabs-list:after:bottom-0 group-data-horizontal/tabs:after:inset-x-0 group-data-horizontal/tabs:after:h-0.5 group-data-vertical/tabs:after:inset-y-0 group-data-vertical/tabs:after:left-0 group-data-vertical/tabs:after:w-0.5 data-active:after:opacity-100',
		className
	)}
	{...restProps}
/>
