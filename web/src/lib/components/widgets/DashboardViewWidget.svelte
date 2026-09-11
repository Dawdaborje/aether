<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();
</script>

<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
	{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
		<PageRenderer node={child} />
	{/each}
</div>
