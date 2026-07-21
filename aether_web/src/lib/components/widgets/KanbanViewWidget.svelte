<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();

	const columns = $derived((node.children ?? []).filter((c) => c.type === 'group'));
</script>

<div class="grid gap-4 md:grid-cols-3">
	{#each columns as col, i (col.title ?? i)}
		<div class="min-h-40 space-y-2 rounded-md border border-border bg-muted/40 p-3">
			<h4 class="text-sm font-semibold text-foreground">{col.title}</h4>
			{#each col.children ?? [] as child, j (child.name ?? j)}
				<PageRenderer node={child} />
			{/each}
		</div>
	{/each}
</div>
