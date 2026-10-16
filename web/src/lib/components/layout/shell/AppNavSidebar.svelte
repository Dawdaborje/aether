<script lang="ts">
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { page } from '$app/state';
	import { appIcon } from '$lib/components/apps/icons';
	import * as Collapsible from '$lib/components/ui/collapsible';
	import type { NavItem } from '$lib/theme/types';
	import { isActive, isMenuCurrent, navHref } from '../nav';

	let { menus }: { menus: NavItem[] } = $props();

	/**
	 * Top-level links and collapsible sections. Singleton groups are already
	 * flattened by `groupAppMenus`; this still flattens any leftover one-child
	 * section so theme nav cannot reproduce the old "ATTENDANCE ▾ / Attendance" look.
	 */
	const sections = $derived.by(() => {
		const items: Array<
			| { kind: 'link'; item: NavItem }
			| { kind: 'group'; label: string; icon?: string; entries: NavItem[] }
		> = [];
		for (const item of menus) {
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
		sections.flatMap((section) =>
			section.kind === 'link' ? [section.item] : section.entries
		)
	);

	const showSidebar = $derived(menus.length > 0);

	/** User toggles; sections with the current page stay open. */
	let openByGroup = $state<Record<string, boolean>>({});

	function groupHasActive(entries: NavItem[]): boolean {
		return entries.some((entry) => isActive(entry, page.url.pathname));
	}

	function isGroupOpen(label: string, entries: NavItem[]): boolean {
		if (groupHasActive(entries)) return true;
		return openByGroup[label] ?? false;
	}
</script>

{#if showSidebar}
	<aside
		class="flex w-56 max-w-80 shrink-0 flex-col border-r border-border bg-card transition-[width] duration-200 ease-out has-[[data-slot=collapsible][data-state=open]]:w-max"
		aria-label="App menu"
	>
		<nav class="min-h-0 min-w-full w-max flex-1 space-y-0.5 overflow-y-auto p-2">
			{#each sections as section (section.kind === 'link' ? section.item.label + (section.item.href ?? '') : section.label)}
				{#if section.kind === 'link'}
					{@const Icon = appIcon(section.item.icon)}
					{#if section.item.href}
						<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
						<a
							href={navHref(section.item.href)}
							aria-current={isMenuCurrent(section.item, page.url.pathname, allLinks)
								? 'page'
								: undefined}
							class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-[13px] whitespace-nowrap text-muted-foreground transition hover:bg-accent hover:text-foreground aria-[current=page]:bg-accent aria-[current=page]:font-medium aria-[current=page]:text-foreground"
						>
							{#if Icon}<Icon class="size-4 shrink-0 opacity-70" />{/if}
							<span>{section.item.label}</span>
						</a>
						<!-- eslint-enable svelte/no-navigation-without-resolve -->
					{/if}
				{:else}
					{@const open = isGroupOpen(section.label, section.entries)}
					{@const GroupIcon = appIcon(section.icon ?? 'folder')}
					<Collapsible.Root
						{open}
						onOpenChange={(next) => {
							openByGroup[section.label] = next;
						}}
						class="pt-1"
					>
						<Collapsible.Trigger
							class="flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-[13px] whitespace-nowrap text-muted-foreground transition hover:bg-accent hover:text-foreground"
						>
							{#if GroupIcon}<GroupIcon class="size-4 shrink-0 opacity-70" />{/if}
							<span>{section.label}</span>
							<ChevronDownIcon
								class="ml-auto size-3.5 shrink-0 opacity-60 transition-transform {open
									? 'rotate-180'
									: ''}"
							/>
						</Collapsible.Trigger>
						<Collapsible.Content class="space-y-0.5 pt-0.5">
							{#each section.entries as entry (entry.label + (entry.href ?? ''))}
								{@const Icon = appIcon(entry.icon)}
								{#if entry.href}
									<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
									<a
										href={navHref(entry.href)}
										aria-current={isMenuCurrent(entry, page.url.pathname, allLinks)
											? 'page'
											: undefined}
										class="flex items-center gap-2.5 rounded-md py-2 pr-2.5 pl-8 text-[13px] whitespace-nowrap text-muted-foreground transition hover:bg-accent hover:text-foreground aria-[current=page]:bg-accent aria-[current=page]:font-medium aria-[current=page]:text-foreground"
									>
										{#if Icon}<Icon class="size-4 shrink-0 opacity-70" />{/if}
										<span>{entry.label}</span>
									</a>
									<!-- eslint-enable svelte/no-navigation-without-resolve -->
								{/if}
							{/each}
						</Collapsible.Content>
					</Collapsible.Root>
				{/if}
			{/each}
		</nav>
	</aside>
{/if}
