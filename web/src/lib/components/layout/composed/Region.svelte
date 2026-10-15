<script lang="ts">
	import { authSession } from '$lib/auth/session.svelte';
	import type { LayoutRegion, RegionName } from '$lib/theme/types';
	import { blockRegistry } from './registry';
	import { getShellState } from './shellContext.svelte';
	import { blockVisible, hideBelowClass } from './visibility';

	let { name, region }: { name: RegionName; region: LayoutRegion } = $props();
	const shell = getShellState();

	const vertical = $derived(name === 'sidebar' || name === 'aside');

	const blocks = $derived(
		region.blocks
			.filter((block) => blockVisible(block, authSession))
			.map((block) => ({ block, spec: blockRegistry[block.type] }))
			.filter((entry) => entry.spec !== undefined)
	);

	/** A region whose blocks would all draw nothing takes no room (e.g. an app with no menus). */
	const shown = $derived(
		blocks.some(({ spec }) => spec && !spec.filler && !(spec.isEmpty?.(shell) ?? false))
	);

	const tone = $derived(`tone-${region.tone ?? 'default'}`);

	const frame: Record<RegionName, string> = {
		banner:
			'flex min-h-9 shrink-0 items-center gap-2 border-b border-region-border px-4 py-1.5 text-sm',
		topbar: 'z-30 flex h-12 shrink-0 items-center gap-1 border-b border-region-border px-2',
		sidebar: 'flex shrink-0 flex-col gap-0.5 overflow-y-auto p-2 max-md:hidden',
		aside: 'flex shrink-0 flex-col gap-2 overflow-y-auto p-3 max-lg:hidden',
		footer: 'flex shrink-0 items-center gap-2 border-t border-region-border px-4 py-2 text-sm'
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
		vertical ? `width: ${region.width ?? (name === 'aside' ? '18rem' : '14rem')}` : undefined
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

{#if shown}
	<svelte:element
		this={Tag}
		data-part="shell.{name}"
		class="{tone} bg-region text-region-foreground {frame[name]} {side} {sticky}"
		style={width}
	>
		{#each blocks as { block, spec }, index (block.id ?? `${block.type}-${index}`)}
			{#if spec}
				{@const Block = spec.component}
				{@const hide = hideBelowClass(block)}
				{#if hide}
					<div class="contents {hide}"><Block {block} {vertical} /></div>
				{:else}
					<Block {block} {vertical} />
				{/if}
			{/if}
		{/each}
	</svelte:element>
{/if}
