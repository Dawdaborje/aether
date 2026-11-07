<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import * as Alert from '$lib/components/ui/alert';

	let { node }: { node: PageNode } = $props();
</script>

<Alert.Root>
	<Alert.Title>{String(node.title ?? node.label ?? 'Notice')}</Alert.Title>
	{#if node.description}
		<Alert.Description>{String(node.description)}</Alert.Description>
	{/if}
</Alert.Root>
