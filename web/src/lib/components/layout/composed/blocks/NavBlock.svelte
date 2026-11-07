<script lang="ts">
	import { themeStore } from '$lib/theme';
	import type { LayoutBlock } from '$lib/theme/types';
	import NavMenu from '../nav/NavMenu.svelte';
	import { NAV_EXPANDS, NAV_VARIANTS, pick } from '../nav/navModel';
	import { getShellState } from '../shellContext.svelte';
	import { textProp } from '../visibility';

	let { block, vertical = false }: { block: LayoutBlock; vertical?: boolean } = $props();
	const shell = getShellState();

	/** Vertical in a sidebar, horizontal in a bar, unless the block says otherwise. */
	const column = $derived(
		textProp(block, 'orientation', vertical ? 'vertical' : 'horizontal') === 'vertical'
	);
</script>

<!-- The theme's navigation (or the desk's). `variant` defaults to `links`. -->
<NavMenu
	items={block.items ??
		(textProp(block, 'source') === 'footer'
			? (themeStore.theme.nav?.footer ?? [])
			: shell.nav.items)}
	variant={pick(NAV_VARIANTS, textProp(block, 'variant'), 'links')}
	expand={pick(NAV_EXPANDS, textProp(block, 'expand'), 'toggle')}
	vertical={column}
	overflow={textProp(block, 'overflow') === 'auto'
		? 'auto'
		: Number(textProp(block, 'overflow')) || 0}
	moreLabel={textProp(block, 'more_label', 'More')}
	label={textProp(block, 'source') === 'footer' ? 'Footer' : (shell.nav.header ?? 'Navigation')}
/>
