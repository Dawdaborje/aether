<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();
</script>

{#if node.route && node.title}
	<ControlPanel crumbs={[{ label: String(node.title) }]} />
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
