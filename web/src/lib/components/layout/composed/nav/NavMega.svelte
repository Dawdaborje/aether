<script lang="ts">
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { NavItem } from '$lib/theme/types';
	import MenuEntry from './MenuEntry.svelte';
	import NavLink from './NavLink.svelte';
	import { holds, itemKey, megaColumns } from './navModel';

	let {
		items: all,
		current,
		vertical,
		overflow = 0 as number | 'auto',
		moreLabel = 'More'
	}: {
		items: NavItem[];
		current: NavItem | null;
		vertical: boolean;
		/**
		 * In a bar: a number keeps that many top-level entries and folds the rest into one "More" panel; `auto` keeps as
		 * many as fit the width and folds only the ones that would run into the next element.
		 */
		overflow?: number | 'auto';
		moreLabel?: string;
	} = $props();

	const auto = $derived(!vertical && overflow === 'auto');
	/** How many entries fit, when `auto`. */
	let fit = $state(Number.POSITIVE_INFINITY);
	let host = $state<HTMLElement | null>(null);
	let ghost = $state<HTMLElement | null>(null);
	/** Room kept for the "More" trigger. */
	const MORE_WIDTH = 96;

	function measure() {
		if (!host || !ghost) return;
		const room = host.clientWidth;
		const widths = Array.from(ghost.children).map((el) => el.getBoundingClientRect().width + 2);
		const total = widths.reduce((sum, width) => sum + width, 0);
		if (total <= room) {
			fit = Number.POSITIVE_INFINITY;
			return;
		}
		let used = MORE_WIDTH;
		let count = 0;
		for (const width of widths) {
			if (used + width > room) break;
			used += width;
			count += 1;
		}
		fit = Math.max(1, count);
	}

	$effect(() => {
		if (!auto || !host || !ghost) return;
		void all;
		const watcher = new ResizeObserver(measure);
		watcher.observe(host);
		watcher.observe(ghost);
		measure();
		return () => watcher.disconnect();
	});

	const items = $derived.by((): NavItem[] => {
		if (vertical) return all;
		const keep = auto ? fit : typeof overflow === 'number' && overflow >= 2 ? overflow : 0;
		if (!keep || all.length <= keep) return all;
		// With a fixed count, a single leftover entry is shown as itself rather than behind "More".
		if (!auto && all.length <= keep + 1) return all;
		return [
			...all.slice(0, keep),
			{ label: moreLabel, icon: 'more-horizontal', children: all.slice(keep) }
		];
	});

	const columnsClass = [
		'',
		'grid-cols-1',
		'grid-cols-2',
		'grid-cols-3',
		'grid-cols-4',
		'grid-cols-5',
		'grid-cols-6'
	];

	/** Where each trigger starts and how far it is above the bar's lower edge, noted as it is pressed. */
	let anchor = $state<Record<string, { left: number; drop: number }>>({});
	function noteAnchor(key: string, event: Event) {
		const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const bar = (event.currentTarget as HTMLElement)
			.closest('[data-part="shell.topbar"]')
			?.getBoundingClientRect();
		anchor[key] = { left: rect.left, drop: bar ? Math.max(0, bar.bottom - rect.bottom) : 0 };
	}
</script>

<!-- `mega`: a group opens a wide panel; nested groups become columns, descriptions show. -->
{#snippet entry(item: NavItem)}
	{#if item.children?.length}
		{@const icon = iconRef(item.icon)}
		{@const panel = megaColumns(item)}
		{@const key = itemKey(item)}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger
				data-part="nav.group"
				onpointerdown={(event) => noteAnchor(key, event)}
				onkeydown={(event) => noteAnchor(key, event)}
				data-active={holds(item, current) || undefined}
				class="group flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1.5 text-sm text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground data-[active]:font-medium data-[active]:text-region-foreground data-[state=open]:bg-region-hover {vertical
					? 'w-full'
					: ''}"
			>
				{#if icon}<AppIcon name={icon} class="size-4 shrink-0" />{/if}
				{item.label}
				<AppIcon
					name={vertical ? 'chevron-right' : 'chevron-down'}
					class="ml-auto size-3.5 opacity-60"
				/>
			</DropdownMenu.Trigger>
			<DropdownMenu.Content
				side={vertical ? 'right' : 'bottom'}
				align="start"
				alignOffset={vertical ? 0 : -(anchor[key]?.left ?? 0)}
				sideOffset={vertical ? 4 : (anchor[key]?.drop ?? 0)}
				avoidCollisions={vertical}
				data-part="nav.panel"
				class={vertical
					? 'w-[min(52rem,92vw)] p-3'
					: 'w-screen max-w-none rounded-none border-x-0 border-t border-b border-region-border px-6 py-4'}
			>
				{#if panel.columns.length}
					<div class="grid gap-4 {columnsClass[Math.min(panel.columns.length, 6)]}">
						{#each panel.columns as column, i (column.heading ?? i)}
							<DropdownMenu.Group>
								{#if column.heading}
									<DropdownMenu.Label class="type-caption text-muted-foreground">
										{column.heading}
									</DropdownMenu.Label>
								{/if}
								{#each column.links as link (itemKey(link))}
									<MenuEntry item={link} current={link.href === current?.href} />
								{/each}
							</DropdownMenu.Group>
						{/each}
					</div>
				{:else}
					<div
						class="grid gap-1 {panel.cells.length > 8
							? 'sm:grid-cols-4'
							: panel.cells.length > 4
								? 'sm:grid-cols-3'
								: 'sm:grid-cols-2'}"
					>
						{#each panel.cells as cell (itemKey(cell))}
							<MenuEntry item={cell} current={cell === current} class="p-2.5" />
						{/each}
					</div>
				{/if}
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	{:else if item.href}
		<NavLink {item} current={item === current} class={vertical ? 'w-full' : ''} />
	{/if}
{/snippet}

{#if auto}
	<div bind:this={host} class="relative flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
		{#each items as item (itemKey(item))}
			{@render entry(item)}
		{/each}
		<!-- The whole menu, unseen, so each entry's width is known without it being on screen. -->
		<div
			bind:this={ghost}
			aria-hidden="true"
			inert
			class="pointer-events-none invisible absolute flex gap-0.5 whitespace-nowrap"
		>
			{#each all as item (itemKey(item))}
				{@render entry(item)}
			{/each}
		</div>
	</div>
{:else}
	{#each items as item (itemKey(item))}
		{@render entry(item)}
	{/each}
{/if}
