<script lang="ts">
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { NavItem } from '$lib/theme/types';
	import MenuEntry from './MenuEntry.svelte';
	import NavLink from './NavLink.svelte';
	import { allLinks, fitCount, holds, itemKey } from './navModel';

	let {
		items,
		current,
		vertical
	}: { items: NavItem[]; current: NavItem | null; vertical: boolean } = $props();

	/** Room for the "More" tab, in pixels. */
	const MORE = 88;

	let available = $state(0);
	let measured: HTMLElement[] = $state([]);
	let widths = $state<number[]>([]);

	// Every tab is also drawn in an invisible row; its widths decide how many fit.
	$effect(() => {
		void available;
		widths = items.map((_, i) => measured[i]?.offsetWidth ?? 0);
	});

	const shown = $derived(
		widths.length === items.length ? fitCount(widths, available, MORE) : items.length
	);
	const overflow = $derived(items.slice(shown));

	const tabClass =
		'group flex h-full items-center gap-1.5 border-b-2 border-transparent px-3 py-2 text-sm whitespace-nowrap text-region-muted-foreground transition outline-none hover:border-region-border hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring data-[active]:border-region-primary data-[active]:font-medium data-[active]:text-region-foreground';
</script>

{#snippet groupMenu(item: NavItem)}
	{@const icon = iconRef(item.icon)}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger
			data-part="nav.group"
			data-active={holds(item, current) || undefined}
			class={tabClass}
		>
			{#if icon}<AppIcon name={icon} class="size-4 shrink-0" />{/if}
			{item.label}
			<AppIcon name="chevron-down" class="size-3.5 opacity-60" />
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="start" class="min-w-48">
			{#each allLinks(item.children ?? []) as child (itemKey(child))}
				<MenuEntry item={child} current={child === current} />
			{/each}
		</DropdownMenu.Content>
	</DropdownMenu.Root>
{/snippet}

{#if vertical}
	<!-- Vertical tabs: a bar on the leading edge marks the current page. -->
	{#each items as item (itemKey(item))}
		{#if item.children?.length}
			<p data-part="nav.heading" class="px-3 pt-3 pb-1 type-caption text-region-muted-foreground">
				{item.label}
			</p>
			{#each allLinks(item.children) as child (itemKey(child))}
				<NavLink item={child} current={child === current} look="side-tab" class="w-full" />
			{/each}
		{:else if item.href}
			<NavLink {item} current={item === current} look="side-tab" class="w-full" />
		{/if}
	{/each}
{:else}
	<div class="relative flex h-full min-w-0 flex-1 items-stretch" bind:clientWidth={available}>
		<div aria-hidden="true" class="pointer-events-none invisible absolute flex">
			{#each items as item, i (itemKey(item))}
				<span bind:this={measured[i]} class={tabClass}>
					{#if iconRef(item.icon)}<span class="size-4"></span>{/if}
					{item.label}
					<!-- Room for a count, so a tab with one still fits. -->
					{#if item.badge}<span class="ml-1.5 h-5 min-w-5 px-1.5 text-[11px]">00</span>{/if}
					{#if item.children?.length}<span class="size-3.5"></span>{/if}
				</span>
			{/each}
		</div>
		{#each items.slice(0, shown) as item (itemKey(item))}
			{#if item.children?.length}
				{@render groupMenu(item)}
			{:else if item.href}
				<NavLink {item} current={item === current} look="tab" />
			{/if}
		{/each}
		{#if overflow.length}
			<DropdownMenu.Root>
				<DropdownMenu.Trigger
					data-part="nav.more"
					data-active={overflow.some((item) => holds(item, current)) || undefined}
					class={tabClass}
				>
					More
					<AppIcon name="chevron-down" class="size-3.5 opacity-60" />
				</DropdownMenu.Trigger>
				<DropdownMenu.Content align="end" class="min-w-48">
					{#each overflow as item (itemKey(item))}
						{#if item.children?.length}
							<DropdownMenu.Label>{item.label}</DropdownMenu.Label>
							{#each allLinks(item.children) as child (itemKey(child))}
								<MenuEntry item={child} current={child === current} />
							{/each}
						{:else if item.href}
							<MenuEntry {item} current={item === current} />
						{/if}
					{/each}
				</DropdownMenu.Content>
			</DropdownMenu.Root>
		{/if}
	</div>
{/if}
