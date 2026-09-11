<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import { Button } from '$lib/components/ui/button';

	let { node }: { node: PageNode } = $props();

	let active = $state(0);
	const tabs = $derived((node.children ?? []).filter((c) => c.type === 'page'));
</script>

<div class="space-y-4">
	<div class="flex gap-1 border-b border-border">
		{#each tabs as tab, i (tab.title ?? i)}
			<Button
				variant={active === i ? 'secondary' : 'ghost'}
				size="sm"
				class="rounded-b-none"
				onclick={() => (active = i)}
			>
				{tab.title ?? `Tab ${i + 1}`}
			</Button>
		{/each}
	</div>
	{#if tabs[active]}
		<PageRenderer node={tabs[active]} />
	{/if}
</div>
