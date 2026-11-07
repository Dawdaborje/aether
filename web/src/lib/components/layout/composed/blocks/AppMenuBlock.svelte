<script lang="ts">
	import type { LayoutBlock, NavItem } from '$lib/theme/types';
	import NavMenu from '../nav/NavMenu.svelte';
	import { NAV_EXPANDS, NAV_VARIANTS, pick } from '../nav/navModel';
	import { getShellState } from '../shellContext.svelte';
	import { textProp } from '../visibility';

	let { block, vertical = false }: { block: LayoutBlock; vertical?: boolean } = $props();
	const shell = getShellState();

	/**
	 * Singleton groups are already flattened by `groupAppMenus`; this still flattens a
	 * leftover one-child section from theme nav.
	 */
	const items = $derived(
		(block.items ?? shell.appMenus).flatMap((item): NavItem[] => {
			if (item.children?.length === 1 && item.children[0]) return [item.children[0]];
			if (item.children?.length || item.href) return [item];
			return [];
		})
	);
</script>

<!-- The current app's pages. Renders nothing when the app has none (the region then hides).
     `variant` defaults to `tree`. -->
{#if items.length > 0}
	<NavMenu
		{items}
		variant={pick(NAV_VARIANTS, textProp(block, 'variant'), 'tree')}
		expand={pick(NAV_EXPANDS, textProp(block, 'expand'), 'toggle')}
		{vertical}
		overflow={textProp(block, 'overflow') === 'auto'
			? 'auto'
			: Number(textProp(block, 'overflow')) || 0}
		moreLabel={textProp(block, 'more_label', 'More')}
		label="App menu"
	/>
{/if}
