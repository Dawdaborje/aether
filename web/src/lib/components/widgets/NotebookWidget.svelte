<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import PluginScope from './PluginScope.svelte';
	import * as Tabs from '$lib/components/ui/tabs';

	let { node }: { node: PageNode } = $props();

	const tabs = $derived((node.children ?? []).filter((c) => c.type === 'page'));
	/** Unique per tab: two plugins may add a tab with the same title. */
	const keyOf = (tab: PageNode, i: number) => `${i}:${tab.plugin ?? ''}:${tab.title ?? ''}`;
	let active = $state('');

	$effect(() => {
		if (!tabs.length) {
			active = '';
			return;
		}
		if (!tabs.some((tab, i) => keyOf(tab, i) === active)) {
			active = tabs[0] ? keyOf(tabs[0], 0) : '';
		}
	});
</script>

{#snippet body(tab: PageNode)}
	<!-- The tab's own content, full width, one block under another. A tab is not a page of
	     its own, so it gets no sheet or padding of its own either. -->
	<div class="flex w-full min-w-0 flex-col gap-4">
		{#each tab.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
			<PageRenderer node={child} />
		{/each}
	</div>
{/snippet}

{#if tabs.length > 0}
	<Tabs.Root bind:value={active} class="w-full gap-4">
		<Tabs.List
			variant="line"
			data-part="notebook.tabs"
			class="h-auto w-full flex-wrap justify-start gap-1 rounded-none border-b border-border p-0"
		>
			{#each tabs as tab, i (keyOf(tab, i))}
				{@const key = keyOf(tab, i)}
				<Tabs.Trigger
					value={key}
					class="h-9 flex-none rounded-none border-0 bg-transparent px-3 text-sm font-medium normal-case tracking-normal shadow-none transition-colors after:bottom-0 after:h-0.5 after:bg-primary focus-visible:ring-0 focus-visible:outline-none focus-visible:after:opacity-50 data-[state=active]:bg-transparent data-[state=active]:text-foreground data-[state=active]:shadow-none data-[state=active]:after:opacity-100 dark:data-[state=active]:border-0 dark:data-[state=active]:bg-transparent"
				>
					{tab.title ?? `Tab ${i + 1}`}
				</Tabs.Trigger>
			{/each}
		</Tabs.List>
		{#each tabs as tab, i (keyOf(tab, i))}
			{@const key = keyOf(tab, i)}
			<Tabs.Content value={key} class="mt-0 space-y-4 outline-none">
				<!-- A tab's widgets call their functions only once it is opened. -->
				{#if key === active}
					{#if typeof tab.plugin === 'string' && tab.plugin}
						<PluginScope plugin={tab.plugin} params={String(tab.params ?? '')}>
							{@render body(tab)}
						</PluginScope>
					{:else}
						{@render body(tab)}
					{/if}
				{/if}
			</Tabs.Content>
		{/each}
	</Tabs.Root>
{/if}
