<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import PageRenderer from './PageRenderer.svelte';

	let { node }: { node: PageNode } = $props();
</script>

<div
	class="flex min-h-64 flex-1 flex-col items-center justify-center gap-3 rounded-md border border-dashed border-border bg-muted/30 px-6 py-12 text-center"
>
	<AppIcon name="mail" class="size-8 text-muted-foreground" />
	<div class="space-y-1">
		<p class="font-medium text-foreground">{String(node.title ?? 'Nothing here')}</p>
		{#if node.description}
			<p class="max-w-2xl text-sm text-muted-foreground">{String(node.description)}</p>
		{/if}
	</div>
	{#if node.actionLabel}
		<Button size="sm">{String(node.actionLabel)}</Button>
	{/if}
	{#if (node.children ?? []).length > 0}
		<div class="flex flex-wrap items-center justify-center gap-2">
			{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
				<PageRenderer node={child} />
			{/each}
		</div>
	{/if}
</div>
