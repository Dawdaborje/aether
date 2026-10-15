<script lang="ts">
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import type { LayoutBlock, NavItem } from '$lib/theme/types';
	import { isActive, navHref } from '../../nav';
	import { getShellState } from '../shellContext.svelte';
	import { textProp } from '../visibility';
	import LinkBlock from './LinkBlock.svelte';

	let { block, vertical = false }: { block: LayoutBlock; vertical?: boolean } = $props();
	const shell = getShellState();

	/** Vertical in a sidebar, horizontal in a bar, unless the block says otherwise. */
	const column = $derived(
		textProp(block, 'orientation', vertical ? 'vertical' : 'horizontal') === 'vertical'
	);

	function asLink(item: NavItem): LayoutBlock {
		return {
			type: 'link',
			props: { label: item.label, href: item.href ?? '/', icon: item.icon ?? '' }
		};
	}

	function open(item: NavItem) {
		// `navHref` applies the base path to app paths; URLs leave the app.
		// eslint-disable-next-line svelte/no-navigation-without-resolve
		if (item.href?.startsWith('/')) void goto(navHref(item.href));
		else if (item.href) window.open(item.href, '_blank', 'noopener,noreferrer');
	}

	const groupHasActive = (item: NavItem) =>
		(item.children ?? []).some((child) => isActive(child, page.url.pathname));
</script>

<!-- The theme's navigation (or the desk's): links, and groups as dropdowns in a bar. -->
<nav
	aria-label={shell.nav.header ?? 'Navigation'}
	class="flex min-w-0 gap-0.5 {column ? 'w-full flex-col' : 'items-center'}"
>
	{#each shell.nav.items as item (item.label + (item.href ?? ''))}
		{#if item.children?.length && !column}
			<DropdownMenu.Root>
				<DropdownMenu.Trigger
					data-active={groupHasActive(item) || undefined}
					class="flex items-center gap-1 rounded-md px-2.5 py-1.5 text-sm text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground data-[active]:font-medium data-[active]:text-region-foreground data-[state=open]:bg-region-hover"
				>
					{item.label}
					<ChevronDownIcon class="size-3.5 opacity-60" />
				</DropdownMenu.Trigger>
				<DropdownMenu.Content align="start" class="min-w-48">
					{#each item.children as child (child.label + (child.href ?? ''))}
						<DropdownMenu.Item onclick={() => open(child)}>{child.label}</DropdownMenu.Item>
					{/each}
				</DropdownMenu.Content>
			</DropdownMenu.Root>
		{:else if item.children?.length}
			<p class="px-2.5 pt-3 pb-1 text-xs font-medium text-region-muted-foreground">{item.label}</p>
			{#each item.children as child (child.label + (child.href ?? ''))}
				<LinkBlock block={asLink(child)} vertical />
			{/each}
		{:else if item.href}
			<LinkBlock block={asLink(item)} vertical={column} />
		{/if}
	{/each}
</nav>
