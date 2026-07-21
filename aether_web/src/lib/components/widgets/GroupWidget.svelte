<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();
</script>

<section class="space-y-3 rounded-md border border-border bg-card p-4 shadow-sm">
	{#if node.title}
		<h3 class="text-sm font-semibold tracking-wide text-muted-foreground uppercase">
			{node.title}
		</h3>
	{/if}
	<div class="grid gap-3 md:grid-cols-2">
		{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
			<PageRenderer node={child} />
		{/each}
	</div>
</section>
