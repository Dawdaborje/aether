<script lang="ts">
	import type { NavItem } from '$lib/theme/types';
	import NavLink from './NavLink.svelte';
	import { allLinks, itemKey } from './navModel';

	let { items, current }: { items: NavItem[]; current: NavItem | null } = $props();

	// Groups become columns with a heading; loose links share a first column without one.
	const columns = $derived.by(() => {
		const loose = items.filter((item) => !item.children?.length && item.href);
		const groups = items
			.filter((item) => item.children?.length)
			.map((item) => ({ heading: item.label, links: allLinks(item.children ?? []) }));
		return loose.length ? [{ heading: null, links: loose }, ...groups] : groups;
	});
</script>

<!-- `columns`: for a footer. Grouped links in columns, each under its heading. -->
<div class="grid w-full gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-4" data-part="nav.columns">
	{#each columns as column, i (column.heading ?? i)}
		<div class="flex min-w-0 flex-col gap-0.5">
			{#if column.heading}
				<p data-part="nav.heading" class="px-2.5 pb-1 type-caption text-region-muted-foreground">
					{column.heading}
				</p>
			{/if}
			{#each column.links as link (itemKey(link))}
				<NavLink item={link} current={link === current} />
			{/each}
		</div>
	{/each}
</div>
