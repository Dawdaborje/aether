<script lang="ts" module>
	import { type VariantProps, tv } from 'tailwind-variants';

	export const badgeVariants = tv({
		base: 'gap-1 rounded-chip border-0 h-6 px-2 text-xs font-normal tracking-[0.32px] transition-colors has-data-[icon=inline-end]:pr-0 has-data-[icon=inline-start]:pl-0 [&>svg]:size-3! focus-visible:outline-2 focus-visible:outline-ring group/badge inline-flex w-fit shrink-0 items-center justify-center overflow-hidden whitespace-nowrap transition-colors [&>svg]:pointer-events-none',
		variants: {
			variant: {
				default: 'bg-primary/15 text-primary dark:text-foreground [a]:hover:bg-primary/25',
				secondary: 'bg-muted text-foreground [a]:hover:bg-layer-hover',
				destructive: 'bg-destructive/15 text-destructive [a]:hover:bg-destructive/25',
				outline: 'border border-border text-foreground [a]:hover:text-foreground/70',
				ghost: 'text-muted-foreground hover:text-foreground',
				link: 'text-foreground underline-offset-4 hover:underline'
			}
		},
		defaultVariants: {
			variant: 'default'
		}
	});

	export type BadgeVariant = VariantProps<typeof badgeVariants>['variant'];
</script>

<script lang="ts">
	import type { HTMLAnchorAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils.js';

	let {
		ref = $bindable(null),
		href,
		class: className,
		variant = 'default',
		children,
		...restProps
	}: WithElementRef<HTMLAnchorAttributes> & {
		variant?: BadgeVariant;
	} = $props();
</script>

<svelte:element
	this={href ? 'a' : 'span'}
	bind:this={ref}
	data-slot="badge"
	{href}
	class={cn(badgeVariants({ variant }), className)}
	{...restProps}
>
	{@render children?.()}
</svelte:element>
