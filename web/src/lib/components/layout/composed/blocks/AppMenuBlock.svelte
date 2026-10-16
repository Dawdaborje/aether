<script lang="ts">
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { page } from '$app/state';
	import { appIcon } from '$lib/components/apps/icons';
	import * as Collapsible from '$lib/components/ui/collapsible';
	import type { NavItem } from '$lib/theme/types';
	import { isActive, isMenuCurrent, navHref } from '../../nav';
	import { getShellState } from '../shellContext.svelte';

	const shell = getShellState();

	/**
	 * Top-level links and collapsible sections. Singleton groups are already flattened
	 * by `groupAppMenus`; this still flattens a leftover one-child section from theme nav.
	 */
	const sections = $derived.by(() => {
		const items: Array<
			| { kind: 'link'; item: NavItem }
			| { kind: 'group'; label: string; icon?: string; entries: NavItem[] }
		> = [];
		for (const item of shell.appMenus) {
			if (item.children?.length) {
				if (item.children.length === 1 && item.children[0]) {
					items.push({ kind: 'link', item: item.children[0] });
				} else {
					items.push({
						kind: 'group',
						label: item.label,
						icon: item.icon,
						entries: item.children
					});
				}
				continue;
			}
			if (item.href) items.push({ kind: 'link', item });
		}
		return items;
	});

	const allLinks = $derived(
		sections.flatMap((section) => (section.kind === 'link' ? [section.item] : section.entries))
	);

	/** User toggles; sections with the current page stay open. */
	let openByGroup = $state<Record<string, boolean>>({});

	function isGroupOpen(label: string, entries: NavItem[]): boolean {
		if (entries.some((entry) => isActive(entry, page.url.pathname))) return true;
		return openByGroup[label] ?? false;
	}

	const linkClass =
		'flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm whitespace-nowrap text-region-muted-foreground outline-none transition hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring aria-[current=page]:bg-region-active aria-[current=page]:font-medium aria-[current=page]:text-region-foreground';
</script>

{#snippet link(item: NavItem, nested = false)}
	{@const Icon = appIcon(item.icon)}
	{#if item.href}
		<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
		<a
			href={navHref(item.href)}
			aria-current={isMenuCurrent(item, page.url.pathname, allLinks) ? 'page' : undefined}
			class="{linkClass} {nested ? 'pl-8' : ''}"
		>
			{#if Icon}<Icon class="size-4 shrink-0" />{/if}
			<span>{item.label}</span>
		</a>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	{/if}
{/snippet}

<!-- The current app's pages. Renders nothing when the app has none (the region then hides). -->
{#if sections.length > 0}
	<nav aria-label="App menu" class="min-w-full w-max space-y-0.5">
		{#each sections as section (section.kind === 'link' ? section.item.label + (section.item.href ?? '') : section.label)}
			{#if section.kind === 'link'}
				{@render link(section.item)}
			{:else}
				{@const open = isGroupOpen(section.label, section.entries)}
				{@const GroupIcon = appIcon(section.icon ?? 'folder')}
				<Collapsible.Root
					{open}
					onOpenChange={(next) => {
						openByGroup[section.label] = next;
					}}
					class="pt-2"
				>
					<Collapsible.Trigger
						class="flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm whitespace-nowrap text-region-muted-foreground transition hover:bg-region-hover hover:text-region-hover-foreground"
					>
						{#if GroupIcon}<GroupIcon class="size-4 shrink-0" />{/if}
						<span>{section.label}</span>
						<ChevronDownIcon
							class="ml-auto size-3.5 shrink-0 transition-transform {open
								? 'rotate-180'
								: ''}"
						/>
					</Collapsible.Trigger>
					<Collapsible.Content class="space-y-0.5 pt-0.5">
						{#each section.entries as entry (entry.label + (entry.href ?? ''))}
							{@render link(entry, true)}
						{/each}
					</Collapsible.Content>
				</Collapsible.Root>
			{/if}
		{/each}
	</nav>
{/if}
