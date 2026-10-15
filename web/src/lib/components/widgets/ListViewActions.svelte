<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';

	let {
		actions,
		row,
		onRun
	}: {
		actions: PageNode[];
		row: Record<string, unknown>;
		onRun: (action: PageNode, row: Record<string, unknown>) => void;
	} = $props();
</script>

<div class="flex justify-end gap-1">
	{#each actions as action (action.name ?? action.function)}
		<Button
			size="sm"
			variant={action.danger ? 'destructive' : 'outline'}
			onclick={(event: MouseEvent) => {
				event.stopPropagation();
				onRun(action, row);
			}}
		>
			{String(action.label ?? action.name ?? 'Action')}
		</Button>
	{/each}
</div>
