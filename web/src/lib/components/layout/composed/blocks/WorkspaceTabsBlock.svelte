<script lang="ts">
	import { page } from '$app/state';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { uiPrefs } from '$lib/theme/uiPrefs.svelte';
	import type { LayoutBlock } from '$lib/theme/types';
	import { navHref } from '../../nav';
	import { getShellState } from '../shellContext.svelte';
	import { textProp } from '../visibility';

	type Tab = { href: string; title: string };
	let { block }: { block: LayoutBlock } = $props();
	const shell = getShellState();
	const limit = $derived(Math.min(20, Math.max(2, Number(textProp(block, 'limit', '8')) || 8)));

	const tabs = $derived(uiPrefs.get<Tab[]>('workspace.tabs', []));
	const base = $derived(navHref('/'));
	const here = $derived.by(() => {
		const path = page.url.pathname + page.url.search;
		return path.startsWith(base) ? `/${path.slice(base.length)}`.replace('//', '/') : path;
	});

	// Every page the person opens becomes a tab (the oldest drops off past the limit).
	$effect(() => {
		const href = here;
		if (shell.onApps || href === '/') return;
		const title = document.title.split(' · ')[0] || href;
		const known = tabs.find((tab) => tab.href === href);
		if (known && known.title === title) return;
		const next = known
			? tabs.map((tab) => (tab.href === href ? { href, title } : tab))
			: [...tabs, { href, title }];
		uiPrefs.set('workspace.tabs', next.slice(-limit));
	});

	function close(href: string) {
		uiPrefs.set(
			'workspace.tabs',
			tabs.filter((tab) => tab.href !== href)
		);
	}
</script>

<!-- The pages the person has open, one tab each. -->
<nav
	aria-label="Open pages"
	data-part="shell.workspace-tabs"
	class="flex min-w-0 items-center gap-0.5 overflow-x-auto"
>
	{#each tabs as tab (tab.href)}
		<span
			class="flex items-center gap-1 rounded-md pr-1 text-sm {tab.href === here
				? 'bg-region-active font-medium text-region-foreground'
				: 'text-region-muted-foreground hover:bg-region-hover'}"
		>
			<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
			<a
				href={navHref(tab.href)}
				aria-current={tab.href === here ? 'page' : undefined}
				class="max-w-40 truncate py-1.5 pl-2.5">{tab.title}</a
			>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
			<button
				type="button"
				aria-label="Close {tab.title}"
				onclick={() => close(tab.href)}
				class="grid size-5 place-items-center rounded-sm hover:bg-region-hover"
			>
				<AppIcon name="close" class="size-3" />
			</button>
		</span>
	{/each}
</nav>
