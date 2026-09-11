<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { browser } from '$app/environment';

	let { node }: { node: PageNode } = $props();

	const show = $derived(browser && import.meta.env.DEV);
</script>

{#if show}
	<div class="rounded border border-dashed border-amber-500/50 bg-amber-500/5 p-3 text-xs">
		Unknown / unhandled widget: <code class="font-mono">{node.type}</code>
		{#if node.name}
			({String(node.name)})
		{/if}
	</div>
{/if}
