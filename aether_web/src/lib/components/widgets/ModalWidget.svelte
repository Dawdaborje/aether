<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';

	let { node }: { node: PageNode } = $props();
	let open = $state(false);
</script>

<Button size="sm" variant="outline" onclick={() => (open = true)}>
	{String(node.title ?? node.label ?? 'Open')}
</Button>

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>{String(node.title ?? 'Dialog')}</Dialog.Title>
			{#if node.description}
				<Dialog.Description>{String(node.description)}</Dialog.Description>
			{/if}
		</Dialog.Header>
		<div class="space-y-3 py-2">
			{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
				<PageRenderer node={child} />
			{/each}
		</div>
	</Dialog.Content>
</Dialog.Root>
