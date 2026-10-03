<script lang="ts">
	import { page } from '$app/state';
	import type { NavItem } from '$lib/theme/types';
	import { isActive, navHref } from './nav';
	import NavMenu from './NavMenu.svelte';

	let { items, depth = 0 }: { items: NavItem[]; depth?: number } = $props();
</script>

<ul class={depth === 0 ? 'flex items-center gap-4' : 'ml-3 mt-1 space-y-1 text-sm'}>
	{#each items as item (item.label)}
		<li>
			{#if item.href}
				<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the app's base path -->
				<a
					href={navHref(item.href)}
					aria-current={isActive(item, page.url.pathname) ? 'page' : undefined}
					class="hover:underline aria-[current=page]:font-semibold"
				>
					{item.label}
				</a>
				<!-- eslint-enable svelte/no-navigation-without-resolve -->
			{:else}
				<span class="font-medium">{item.label}</span>
			{/if}
			{#if item.children?.length}
				<NavMenu items={item.children} depth={depth + 1} />
			{/if}
		</li>
	{/each}
</ul>
