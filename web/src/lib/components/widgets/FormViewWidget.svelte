<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();
</script>

<div class="space-y-4">
	{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
		<PageRenderer node={child} />
	{/each}
</div>
