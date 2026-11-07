<script lang="ts" module>
	import { cn, type WithElementRef } from '$lib/utils.js';
	import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';
	import { type VariantProps, tv } from 'tailwind-variants';

	/*
	 * Carbon buttons: square, flat, 48/40/32px tall, label at the start and an icon at the
	 * end. `outline` is Carbon's tertiary button, `destructive` its danger button.
	 */
	export const buttonVariants = tv({
		base: "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring focus-visible:shadow-[inset_0_0_0_3px_var(--background)] aria-invalid:outline-2 aria-invalid:outline-destructive rounded-button border border-transparent type-button [&_svg:not([class*='size-'])]:size-4 group/button inline-flex shrink-0 items-center justify-center whitespace-nowrap transition-[background-color,border-color,color,box-shadow] duration-short select-none disabled:pointer-events-none disabled:bg-muted disabled:text-muted-foreground disabled:border-transparent [&_svg]:pointer-events-none [&_svg]:shrink-0",
		variants: {
			variant: {
				default: 'bg-primary text-primary-foreground hover:bg-primary-hover',
				outline:
					'border-primary text-primary hover:bg-primary-hover hover:text-primary-foreground aria-expanded:bg-primary aria-expanded:text-primary-foreground bg-transparent dark:border-foreground dark:text-foreground dark:hover:border-transparent',
				secondary:
					'bg-secondary text-secondary-foreground hover:bg-secondary-hover aria-expanded:bg-secondary-hover',
				ghost:
					'text-foreground hover:bg-layer-hover aria-expanded:bg-layer-hover aria-expanded:text-foreground disabled:bg-transparent',
				destructive:
					'bg-destructive text-white hover:bg-[color-mix(in_oklab,var(--destructive)_85%,black)]',
				link: 'text-primary underline-offset-4 hover:underline disabled:bg-transparent'
			},
			size: {
				default:
					'h-10 gap-2 px-4 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3',
				xs: "h-6 gap-1 px-2 text-xs has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
				sm: 'h-8 gap-2 px-3 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2',
				lg: 'h-12 gap-2 pr-16 pl-4 justify-between has-data-[icon=inline-end]:pr-4 has-data-[icon=inline-start]:pl-4',
				icon: 'size-10',
				'icon-xs': "size-6 [&_svg:not([class*='size-'])]:size-3",
				'icon-sm': 'size-8',
				'icon-lg': 'size-12'
			}
		},
		defaultVariants: {
			variant: 'default',
			size: 'default'
		}
	});

	export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
	export type ButtonSize = VariantProps<typeof buttonVariants>['size'];

	export type ButtonProps = WithElementRef<HTMLButtonAttributes> &
		WithElementRef<HTMLAnchorAttributes> & {
			variant?: ButtonVariant;
			size?: ButtonSize;
		};
</script>

<script lang="ts">
	let {
		class: className,
		variant = 'default',
		size = 'default',
		ref = $bindable(null),
		href = undefined,
		type = 'button',
		disabled,
		children,
		...restProps
	}: ButtonProps = $props();
</script>

{#if href}
	<a
		bind:this={ref}
		data-slot="button"
		class={cn(buttonVariants({ variant, size }), className)}
		href={disabled ? undefined : href}
		aria-disabled={disabled}
		role={disabled ? 'link' : undefined}
		tabindex={disabled ? -1 : undefined}
		{...restProps}
	>
		{@render children?.()}
	</a>
{:else}
	<button
		bind:this={ref}
		data-slot="button"
		class={cn(buttonVariants({ variant, size }), className)}
		{type}
		{disabled}
		{...restProps}
	>
		{@render children?.()}
	</button>
{/if}
