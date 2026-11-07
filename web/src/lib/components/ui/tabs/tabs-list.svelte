<script lang="ts" module>
	import { tv, type VariantProps } from 'tailwind-variants';

	export const tabsListVariants = tv({
		base: 'group-data-horizontal/tabs:h-12 group/tabs-list flex w-full items-center justify-start group-data-vertical/tabs:w-fit text-muted-foreground group-data-vertical/tabs:h-fit group-data-vertical/tabs:flex-col',
		variants: {
			variant: {
				default: 'bg-transparent',
				line: 'bg-transparent shadow-[inset_0_-2px_0_var(--border)]'
			}
		},
		defaultVariants: {
			variant: 'default'
		}
	});

	export type TabsListVariant = VariantProps<typeof tabsListVariants>['variant'];
</script>

<script lang="ts">
	import { Tabs as TabsPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		variant = 'default',
		class: className,
		...restProps
	}: TabsPrimitive.ListProps & {
		variant?: TabsListVariant;
	} = $props();
</script>

<TabsPrimitive.List
	bind:ref
	data-slot="tabs-list"
	data-variant={variant}
	class={cn(tabsListVariants({ variant }), className)}
	{...restProps}
/>
