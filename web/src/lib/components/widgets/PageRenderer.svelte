<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { resolveWidget } from '$lib/dsl/registry';

	let { node }: { node: PageNode } = $props();

	const Widget = $derived(resolveWidget(node.type));
</script>

<Widget {node} />
