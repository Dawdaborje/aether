<script lang="ts">
	import * as Collapsible from '$lib/components/ui/collapsible';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { NavItem } from '$lib/theme/types';
	import NavBadgePill from './NavBadgePill.svelte';
	import NavLink from './NavLink.svelte';
	import { holds, itemKey, type NavExpand } from './navModel';

	let { items, current, expand }: { items: NavItem[]; current: NavItem | null; expand: NavExpand } =
		$props();

	/** `toggle`: the user's choice per group (a group holding the current page starts open). */
	let toggled = $state<Record<string, boolean>>({});
	/** `active`: the one open group under each parent (`undefined`: the one holding the page). */
	let single = $state<Record<string, string | null>>({});

	function isOpen(item: NavItem, parent: string): boolean {
		const key = itemKey(item);
		if (expand === 'all') return true;
		if (expand === 'active') {
			const chosen = single[parent];
			return chosen === undefined ? holds(item, current) : chosen === key;
		}
		return toggled[key] ?? holds(item, current);
	}

	function setOpen(item: NavItem, parent: string, open: boolean) {
		const key = itemKey(item);
		if (expand === 'active') single[parent] = open ? key : null;
		else toggled[key] = open;
	}

	const rowClass =
		'flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm whitespace-nowrap text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring data-[active]:text-region-foreground';
</script>

{#snippet level(list: NavItem[], depth: number, parent: string)}
	{#each list as item (itemKey(item))}
		{#if item.children?.length}
			{@const open = isOpen(item, parent)}
			{@const icon = iconRef(item.icon ?? (item.href ? undefined : 'folder'))}
			<Collapsible.Root
				{open}
				onOpenChange={(next) => setOpen(item, parent, next)}
				data-part="nav.group"
				class={depth === 0 ? 'pt-1' : ''}
			>
				{#if item.href}
					<!-- A group that is also a page: the label opens the page, the chevron the group. -->
					<div class="flex items-center">
						<NavLink {item} current={item === current} {depth} class="min-w-0 flex-1" />
						{#if expand !== 'all'}
							<Collapsible.Trigger
								aria-label="{open ? 'Collapse' : 'Expand'} {item.label}"
								class="rounded-md p-1.5 text-region-muted-foreground hover:bg-region-hover"
							>
								<AppIcon
									name="chevron-down"
									class="size-3.5 transition-transform {open ? 'rotate-180' : ''}"
								/>
							</Collapsible.Trigger>
						{/if}
					</div>
				{:else if expand === 'all'}
					<p
						data-part="nav.heading"
						style={depth ? `padding-inline-start: ${0.625 + depth * 1.5}rem` : undefined}
						class="px-2.5 pt-2 pb-1 type-caption text-region-muted-foreground"
					>
						{item.label}
					</p>
				{:else}
					<Collapsible.Trigger
						data-active={holds(item, current) || undefined}
						style={depth ? `padding-inline-start: ${0.625 + depth * 1.5}rem` : undefined}
						class={rowClass}
					>
						{#if icon}<AppIcon name={icon} class="size-4 shrink-0" />{/if}
						<span>{item.label}</span>
						<NavBadgePill badge={item.badge} />
						<AppIcon
							name="chevron-down"
							class="{item.badge ? 'ml-1' : 'ml-auto'} size-3.5 shrink-0 transition-transform {open
								? 'rotate-180'
								: ''}"
						/>
					</Collapsible.Trigger>
				{/if}
				<Collapsible.Content class="space-y-0.5 pt-0.5">
					{@render level(item.children, depth + 1, itemKey(item))}
				</Collapsible.Content>
			</Collapsible.Root>
		{:else if item.href}
			<NavLink {item} current={item === current} {depth} class="w-full" />
		{/if}
	{/each}
{/snippet}

<!-- `tree`: groups expand in place; `expand` decides which stay open. -->
{@render level(items, 0, '')}
