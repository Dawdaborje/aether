<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { NavItem } from '$lib/theme/types';
	import { navHref } from '../../nav';
	import NavBadgePill from './NavBadgePill.svelte';

	let {
		item,
		current = false,
		look = 'pill',
		depth = 0,
		class: className = ''
	}: {
		item: NavItem;
		current?: boolean;
		/** `pill`: a filled row; `tab`: underlined; `side-tab`: a bar on the leading edge. */
		look?: 'pill' | 'tab' | 'side-tab';
		/** Nesting level in a tree; indents the row. */
		depth?: number;
		class?: string;
	} = $props();

	const href = $derived(item.href ?? '/');
	const external = $derived(!href.startsWith('/'));
	const icon = $derived(iconRef(item.icon));

	const looks = {
		pill: 'rounded-md px-2.5 py-1.5 hover:bg-region-hover hover:text-region-hover-foreground aria-[current=page]:bg-region-active aria-[current=page]:font-medium aria-[current=page]:text-region-foreground',
		tab: 'h-full border-b-2 border-transparent px-3 py-2 hover:text-region-hover-foreground hover:border-region-border aria-[current=page]:border-region-primary aria-[current=page]:font-medium aria-[current=page]:text-region-foreground',
		'side-tab':
			'border-l-2 border-transparent px-3 py-1.5 hover:bg-region-hover hover:text-region-hover-foreground aria-[current=page]:border-region-primary aria-[current=page]:font-medium aria-[current=page]:text-region-foreground'
	};
</script>

<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
<a
	href={navHref(href)}
	target={external ? '_blank' : undefined}
	rel={external ? 'noopener noreferrer' : undefined}
	aria-current={current ? 'page' : undefined}
	data-part="nav.link"
	style={depth ? `padding-inline-start: ${0.625 + depth * 1.5}rem` : undefined}
	class="flex items-center gap-2.5 text-sm whitespace-nowrap text-region-muted-foreground transition outline-none focus-visible:ring-2 focus-visible:ring-region-ring {looks[
		look
	]} {className}"
>
	{#if icon}<AppIcon name={icon} class="size-4 shrink-0" />{/if}
	<span class="truncate">{item.label}</span>
	<NavBadgePill badge={item.badge} class={look === 'pill' ? '' : 'ml-1.5'} />
</a>
<!-- eslint-enable svelte/no-navigation-without-resolve -->
