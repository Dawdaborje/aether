<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { NavItem } from '$lib/theme/types';
	import { navHref } from '../../nav';
	import NavBadgePill from './NavBadgePill.svelte';
	import { allLinks, itemKey } from './navModel';

	let { items, current }: { items: NavItem[]; current: NavItem | null } = $props();

	// Groups become sections of tiles; loose links share a first section without a heading.
	const sections = $derived.by(() => {
		const loose = items.filter((item) => !item.children?.length && item.href);
		const groups = items
			.filter((item) => item.children?.length)
			.map((item) => ({ heading: item.label, links: allLinks(item.children ?? []) }));
		return loose.length ? [{ heading: null, links: loose }, ...groups] : groups;
	});
</script>

<!-- `tiles`: a launchpad. Each link is a tile with its icon, label, description and badge. -->
<div class="flex w-full flex-col gap-5" data-part="nav.tiles">
	{#each sections as section, i (section.heading ?? i)}
		<section class="flex flex-col gap-2">
			{#if section.heading}
				<h2 data-part="nav.heading" class="type-caption text-region-muted-foreground">
					{section.heading}
				</h2>
			{/if}
			<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
				{#each section.links as link (itemKey(link))}
					{@const icon = iconRef(link.icon)}
					<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
					<a
						href={navHref(link.href ?? '/')}
						aria-current={link === current ? 'page' : undefined}
						data-part="nav.tile"
						class="flex min-h-24 flex-col gap-2 rounded-tile border border-region-border bg-region-hover/30 p-3 text-region-foreground transition outline-none hover:bg-region-hover focus-visible:ring-2 focus-visible:ring-region-ring aria-[current=page]:border-region-primary"
					>
						<span class="flex items-start gap-2">
							{#if icon}<AppIcon name={icon} class="size-5 shrink-0 text-region-primary" />{/if}
							<span class="min-w-0 flex-1 truncate text-sm font-medium">{link.label}</span>
							<NavBadgePill badge={link.badge} />
						</span>
						{#if link.description}
							<span class="line-clamp-2 text-xs text-region-muted-foreground"
								>{link.description}</span
							>
						{/if}
					</a>
					<!-- eslint-enable svelte/no-navigation-without-resolve -->
				{/each}
			</div>
		</section>
	{/each}
</div>
