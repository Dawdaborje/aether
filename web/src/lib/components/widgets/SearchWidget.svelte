<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { Input } from '$lib/components/ui/input';

	let { node }: { node: PageNode } = $props();
	let q = $state('');
</script>

<Input
	class="max-w-sm"
	placeholder={String(node.placeholder ?? node.label ?? 'Search…')}
	bind:value={q}
/>
