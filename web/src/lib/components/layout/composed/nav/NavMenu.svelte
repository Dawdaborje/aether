<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import type { NavItem } from '$lib/theme/types';
	import NavColumns from './NavColumns.svelte';
	import NavLinks from './NavLinks.svelte';
	import NavMega from './NavMega.svelte';
	import NavTiles from './NavTiles.svelte';
	import NavTabs from './NavTabs.svelte';
	import NavTree from './NavTree.svelte';
	import { navBadges } from './navBadges.svelte';
	import { currentLink, type NavExpand, type NavVariant } from './navModel';

	let {
		items,
		variant,
		expand = 'toggle',
		vertical,
		label,
		overflow = 0,
		moreLabel = 'More'
	}: {
		items: NavItem[];
		variant: NavVariant;
		expand?: NavExpand;
		vertical: boolean;
		label: string;
		overflow?: number | 'auto';
		moreLabel?: string;
	} = $props();

	const current = $derived(currentLink(items, page.url.pathname));

	// Counts often change with what was just done on the last page.
	afterNavigate(() => navBadges.refresh());
</script>

<!-- A theme's or app's navigation, drawn as the layout's `variant` asks. A tree in a bar
     draws as `links`. -->
<nav
	aria-label={label}
	data-part="nav"
	data-variant={variant}
	class="flex min-w-0 gap-0.5 {vertical
		? 'w-max min-w-full flex-col'
		: variant === 'mega' && overflow === 'auto'
			? 'w-full flex-[1000_1_0%]'
			: variant === 'columns' || variant === 'tiles'
				? 'w-full'
				: variant === 'tabs'
					? 'h-full flex-[1000_1_0%] items-stretch self-stretch'
					: 'items-center'}"
>
	{#if variant === 'columns'}
		<NavColumns {items} {current} />
	{:else if variant === 'tiles'}
		<NavTiles {items} {current} />
	{:else if variant === 'tree' && vertical}
		<NavTree {items} {current} {expand} />
	{:else if variant === 'tabs'}
		<NavTabs {items} {current} {vertical} />
	{:else if variant === 'mega'}
		<NavMega {items} {current} {vertical} {overflow} {moreLabel} />
	{:else}
		<NavLinks {items} {current} {vertical} />
	{/if}
</nav>
