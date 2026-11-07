<script lang="ts">
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { NavItem } from '$lib/theme/types';
	import MenuEntry from './MenuEntry.svelte';
	import NavLink from './NavLink.svelte';
	import { allLinks, holds, itemKey } from './navModel';

	let {
		items,
		current,
		vertical
	}: { items: NavItem[]; current: NavItem | null; vertical: boolean } = $props();
</script>

<!-- `links`: links in a row or a column; in a row a group is a dropdown, in a column a heading. -->
{#each items as item (itemKey(item))}
	{#if item.children?.length && !vertical}
		{@const icon = iconRef(item.icon)}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger
				data-part="nav.group"
				data-active={holds(item, current) || undefined}
				class="group flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-sm text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground data-[active]:font-medium data-[active]:text-region-foreground data-[state=open]:bg-region-hover"
			>
				{#if icon}<AppIcon name={icon} class="size-4 shrink-0" />{/if}
				{item.label}
				<AppIcon
					name="chevron-down"
					class="size-3.5 opacity-60 transition-transform group-data-[state=open]:rotate-180"
				/>
			</DropdownMenu.Trigger>
			<DropdownMenu.Content align="start" class="min-w-48">
				{#each allLinks(item.children) as child (itemKey(child))}
					<MenuEntry item={child} current={child === current} />
				{/each}
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	{:else if item.children?.length}
		<p data-part="nav.heading" class="px-2.5 pt-3 pb-1 type-caption text-region-muted-foreground">
			{item.label}
		</p>
		{#each allLinks(item.children) as child (itemKey(child))}
			<NavLink item={child} current={child === current} class="w-full" />
		{/each}
	{:else if item.href}
		<NavLink {item} current={item === current} class={vertical ? 'w-full' : ''} />
	{/if}
{/each}
