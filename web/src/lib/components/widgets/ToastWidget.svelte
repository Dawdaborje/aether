<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { onMount } from 'svelte';

	let { node }: { node: PageNode } = $props();
	let visible = $state(true);

	onMount(() => {
		const t = setTimeout(() => (visible = false), 3200);
		return () => clearTimeout(t);
	});
</script>

{#if visible}
	<div
		class="fixed right-4 bottom-4 z-50 max-w-sm rounded-md border border-border bg-card px-4 py-3 shadow-md"
		role="status"
	>
		<p class="text-sm font-medium text-foreground">{String(node.title ?? 'Notification')}</p>
		{#if node.description}
			<p class="mt-0.5 text-sm text-muted-foreground">{String(node.description)}</p>
		{/if}
	</div>
{/if}
