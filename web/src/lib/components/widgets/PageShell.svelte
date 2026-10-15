<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import { navHref } from '$lib/components/layout/nav';
	import { subscribeDataChanged } from '$lib/pages/liveRefresh';
	import { PageChrome, setPageChrome } from '$lib/pages/pageChrome.svelte';
	import { getPageState } from '$lib/pages/pageContext.svelte';
	import FacetSearchBar from '$lib/search/FacetSearchBar.svelte';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();

	const chrome = new PageChrome();
	setPageChrome(chrome);
	const page = getPageState();

	// Forms on this page use the title bar for Save and buttons when there is one.
	$effect.pre(() => {
		chrome.hasBar = Boolean(node.route && node.title);
		return () => {
			chrome.hasBar = false;
		};
	});

	// Kernel db::create/update/delete → SSE `aether.data.changed` → reload views.
	$effect(() => {
		if (!page) return;
		const watched = new Set<string>();
		const pageModel = page.model.trim();
		if (pageModel) watched.add(pageModel);
		const listModel = (chrome.model ?? '').trim();
		if (listModel) watched.add(listModel);
		for (const model of chrome.refreshModels) {
			const name = model.trim();
			if (name) watched.add(name);
		}
		if (watched.size === 0) return;
		return subscribeDataChanged(
			(payload) => watched.has(payload.model),
			() => page.refresh()
		);
	});
</script>

{#if node.route && node.title}
	<ControlPanel crumbs={[{ label: String(node.title) + (chrome.dirty ? ' *' : '') }]}>
		{#snippet actions()}
			{#if chrome.createHref}
				<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies base -->
				<Button size="sm" href={navHref(chrome.createHref)}>{chrome.createLabel}</Button>
			{/if}
		{/snippet}
		{#if chrome.searchable}
			<FacetSearchBar {chrome} />
		{/if}
		<!-- A form on this page puts its buttons and Save here, then its state steps. -->
		<div class="flex flex-wrap items-center justify-end gap-2" bind:this={chrome.barActions}></div>
		<div class="flex flex-wrap items-center justify-end gap-2" bind:this={chrome.barStatus}></div>
	</ControlPanel>
{/if}
<section class="flex min-h-0 w-full flex-1 flex-col gap-3 overflow-hidden p-3 md:px-6 md:py-5">
	<!-- One sheet per page; what is inside (form groups, lists) draws no frame of its own. -->
	<div
		data-part="page.sheet"
		class="min-h-0 flex-1 space-y-6 overflow-y-auto overscroll-contain rounded-lg border border-border bg-card p-4 contain-paint md:p-6"
	>
		{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
			{#if child.type !== 'spinner' && child.type !== 'toast' && child.type !== 'modal' && child.type !== 'confirm' && child.type !== 'dialog'}
				<PageRenderer node={child} />
			{/if}
		{/each}
	</div>
	{#each node.children ?? [] as child, i (child.name ?? `overlay-${child.type}-${i}`)}
		{#if child.type === 'modal' || child.type === 'dialog' || child.type === 'confirm' || child.type === 'toast'}
			<PageRenderer node={child} />
		{/if}
	{/each}
</section>
