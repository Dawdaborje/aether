<script lang="ts">
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { NavItem } from '$lib/theme/types';
	import NavBadgePill from './NavBadgePill.svelte';
	import { openItem } from './openItem';

	let {
		item,
		current = false,
		class: className = ''
	}: { item: NavItem; current?: boolean; class?: string } = $props();
	const icon = $derived(iconRef(item.icon));
</script>

<!-- One link in a dropdown or mega panel; shows the item's description under its label. -->
<DropdownMenu.Item
	onclick={() => openItem(item)}
	aria-current={current ? 'page' : undefined}
	data-part="nav.link"
	class="cursor-pointer items-start gap-2.5 aria-[current=page]:font-medium {className}"
>
	{#if icon}<AppIcon name={icon} class="mt-0.5 size-4 shrink-0" />{/if}
	<span class="flex min-w-0 flex-col">
		<span>{item.label}</span>
		{#if item.description}
			<span class="text-xs font-normal whitespace-normal text-muted-foreground">
				{item.description}
			</span>
		{/if}
	</span>
	<NavBadgePill badge={item.badge} />
</DropdownMenu.Item>
