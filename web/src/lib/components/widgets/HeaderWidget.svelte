<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();
</script>

<header class="flex flex-wrap items-center justify-between gap-3">
	{#if node.title}
		<h2 class="text-lg font-medium text-foreground">{node.title}</h2>
	{/if}
	<div class="flex flex-wrap items-center gap-2">
		{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
			<PageRenderer node={child} />
		{/each}
	</div>
</header>
