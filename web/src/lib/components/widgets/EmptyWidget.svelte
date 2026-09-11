<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import { InboxIcon } from '@lucide/svelte';

	let { node }: { node: PageNode } = $props();
</script>

<div
	class="flex flex-col items-center justify-center gap-3 rounded-md border border-dashed border-border bg-muted/30 px-6 py-12 text-center"
>
	<InboxIcon class="size-8 text-muted-foreground" />
	<div class="space-y-1">
		<p class="font-medium text-foreground">{String(node.title ?? 'Nothing here')}</p>
		{#if node.description}
			<p class="max-w-sm text-sm text-muted-foreground">{String(node.description)}</p>
		{/if}
	</div>
	{#if node.actionLabel}
		<Button size="sm">{String(node.actionLabel)}</Button>
	{/if}
</div>
