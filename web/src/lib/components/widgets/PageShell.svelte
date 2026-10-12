<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';
	import type { PageNode } from '$lib/dsl/types';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { navHref } from '$lib/components/layout/nav';
	import { PageChrome, setPageChrome } from '$lib/pages/pageChrome.svelte';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();

	const chrome = new PageChrome();
	setPageChrome(chrome);
</script>

{#if node.route && node.title}
	<ControlPanel crumbs={[{ label: String(node.title) }]}>
		{#snippet actions()}
			{#if chrome.createHref}
				<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies base -->
				<Button size="sm" href={navHref(chrome.createHref)}>{chrome.createLabel}</Button>
			{/if}
		{/snippet}
		{#if chrome.searchable}
			<div class="relative w-full max-w-xs">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					bind:value={chrome.query}
					type="search"
					placeholder="Search…"
					aria-label="Search"
					class="pl-9"
				/>
			</div>
		{/if}
	</ControlPanel>
{/if}
<section class="flex h-full w-full min-h-0 flex-1 flex-col gap-3 p-3 md:px-6 md:py-4">
	<div class="min-h-0 flex-1 space-y-3 overflow-auto rounded-lg border border-border bg-card p-4 shadow-sm">
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
