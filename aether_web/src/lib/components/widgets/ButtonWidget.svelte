<script lang="ts">
	import type { ButtonVariant, PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';

	let { node }: { node: PageNode } = $props();

	const variant = $derived(
		(node.danger
			? 'destructive'
			: typeof node.variant === 'string'
				? node.variant
				: 'default') as ButtonVariant
	);
</script>

<Button size="sm" {variant}>
	{String(node.label ?? node.name ?? 'Action')}
</Button>
