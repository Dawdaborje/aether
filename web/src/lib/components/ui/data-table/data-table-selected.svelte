<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';

	let {
		count,
		onClear,
		children
	}: {
		count: number;
		onClear: () => void;
		/** Actions on the selected rows, shown before Clear. */
		children?: Snippet;
	} = $props();
</script>

{#if count > 0}
	<div
		class="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-muted/40 px-3 py-2 text-sm"
		role="status"
	>
		<span class="text-muted-foreground">{count} selected</span>
		<div class="flex flex-wrap items-center gap-2">
			{@render children?.()}
			<Button size="sm" variant="ghost" onclick={onClear}>Clear</Button>
		</div>
	</div>
{/if}
