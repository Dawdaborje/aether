<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();
</script>

<section class="flex h-full min-h-0 flex-col gap-4 p-4 md:p-6">
	{#if node.route && node.title}
		<div class="flex items-center justify-between gap-3 border-b border-border pb-3">
			<h1 class="text-xl font-semibold tracking-tight text-foreground">{node.title}</h1>
		</div>
	{/if}
	<div class="min-h-0 flex-1 space-y-4 overflow-auto">
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
