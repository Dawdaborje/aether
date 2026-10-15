<script lang="ts">
	import { Button } from '$lib/components/ui/button/index.js';

	let {
		count,
		onClear
	}: {
		count: number;
		onClear: () => void;
	} = $props();
</script>

{#if count > 0}
	<div
		class="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-muted/40 px-3 py-2 text-sm"
		role="status"
	>
		<span class="text-muted-foreground">{count} selected</span>
		<Button size="sm" variant="ghost" onclick={onClear}>Clear</Button>
	</div>
{/if}
