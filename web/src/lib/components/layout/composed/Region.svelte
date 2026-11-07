<script lang="ts">
	import { authSession } from '$lib/auth/session.svelte';
	import type { LayoutRegion, RegionName } from '$lib/theme/types';
	import { blockRegistry } from './registry';
	import { getShellState } from './shellContext.svelte';
	import { blockVisible, hideBelowClass, showBelowClass } from './visibility';

	let { name, region }: { name: RegionName; region: LayoutRegion } = $props();
	const shell = getShellState();

	const vertical = $derived(name === 'sidebar' || name === 'aside');

	// Sidebar modes: `rail` is always a narrow strip of icons; `collapsible` folds when the
	// person asks, to the same strip if the layout gives a `collapsed_width`, else away.
	const folded = $derived(
		name === 'sidebar' && region.mode === 'collapsible' && shell.sidebarCollapsed
	);
	const rail = $derived(
		name === 'sidebar' && (region.mode === 'rail' || (folded && Boolean(region.collapsed_width)))
	);
	const gone = $derived(folded && !region.collapsed_width);
	const overlay = $derived(name === 'sidebar' && Boolean(region.overlay));

	const blocks = $derived(
		region.blocks
			.filter((block) => blockVisible(block, authSession))
			.map((block) => ({ block, spec: blockRegistry[block.type] }))
			.filter((entry) => entry.spec !== undefined)
	);

	/** A region whose blocks would all draw nothing takes no room (e.g. an app with no menus). */
	const shown = $derived(
		blocks.some(
			({ spec, block }) => spec && !spec.filler && !(spec.isEmpty?.(shell, block) ?? false)
		)
	);

	const tone = $derived(`tone-${region.tone ?? 'default'}`);

	const frame: Record<RegionName, string> = {
		banner:
			'flex min-h-9 shrink-0 items-center gap-2 border-b border-region-border px-4 py-1.5 text-sm',
		topbar: 'z-30 flex h-12 shrink-0 items-center gap-1 border-b border-region-border px-2',
		// A second row under the top bar: tabs, a subnav, breadcrumbs.
		subbar: 'z-20 flex h-11 shrink-0 items-center gap-1 border-b border-region-border px-2',
		// Grows when a collapsible menu group is open so indented labels are not clipped.
		sidebar:
			'flex w-[var(--side-min)] max-w-80 shrink-0 flex-col gap-0.5 overflow-y-auto p-2 transition-[width] duration-medium ease-out max-md:hidden has-[[data-slot=collapsible][data-state=open]]:w-max',
		aside: 'flex shrink-0 flex-col gap-2 overflow-y-auto p-3 max-lg:hidden',
		// A bar along the bottom edge: with `show_below` it is the phone's tab bar.
		bottombar:
			'z-30 flex min-h-12 shrink-0 items-center justify-around gap-1 border-t border-region-border px-2 pb-[env(safe-area-inset-bottom)]',
		footer:
			'flex shrink-0 flex-wrap items-center gap-2 border-t border-region-border px-4 py-2 text-sm'
	};

	const side = $derived(
		vertical
			? region.position === 'right'
				? 'border-l border-region-border'
				: 'border-r border-region-border'
			: ''
	);
	const sticky = $derived(region.sticky && name === 'topbar' ? 'sticky top-0' : '');
	const width = $derived(
		vertical
			? name === 'sidebar'
				? `--side-min: ${rail ? (region.collapsed_width ?? '4.5rem') : (region.width ?? '14rem')}`
				: `width: ${region.width ?? '18rem'}`
			: undefined
	);

	const Tag = $derived(
		name === 'topbar'
			? 'header'
			: name === 'footer'
				? 'footer'
				: name === 'sidebar' || name === 'aside'
					? 'aside'
					: 'div'
	);
</script>

{#if shown && !gone}
	<svelte:element
		this={Tag}
		data-part="shell.{name}"
		data-rail={rail ? 'true' : undefined}
		class="{tone} bg-region text-region-foreground {frame[name]} {side} {sticky} {showBelowClass(
			region
		)} {rail ? 'has-[[data-slot=collapsible][data-state=open]]:w-[var(--side-min)]' : ''} {overlay
			? shell.sidebarOpen
				? 'max-md:fixed! max-md:inset-y-12 max-md:left-0 max-md:z-40 max-md:flex! max-md:shadow-lg'
				: ''
			: ''}"
		style={width}
	>
		{#each blocks as { block, spec }, index (block.id ?? `${block.type}-${index}`)}
			{#if spec}
				{@const Block = spec.component}
				{@const hide = `${hideBelowClass(block)} ${showBelowClass(block)}`.trim()}
				{#if hide}
					<div class="contents {hide}"><Block {block} {vertical} /></div>
				{:else}
					<Block {block} {vertical} />
				{/if}
			{/if}
		{/each}
	</svelte:element>
{/if}
