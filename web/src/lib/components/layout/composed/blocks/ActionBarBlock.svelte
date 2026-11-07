<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { pageState } from '../pageState.svelte';
</script>

<!-- Shows while rows are selected in the open list. -->
{#if pageState.selection && pageState.selection.count > 0}
	<div data-part="shell.action-bar" class="flex items-center gap-2 px-1.5 text-sm" role="status">
		<span>{pageState.selection.count} selected</span>
		<Button size="sm" variant="ghost" onclick={() => pageState.selection?.clear()}>Clear</Button>
	</div>
{/if}
