<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';

	let { node }: { node: PageNode } = $props();
</script>

<div
	class="flex h-48 items-center justify-center rounded-md border border-border bg-muted/40 text-sm text-muted-foreground"
>
	Chart: {String(node.title ?? node.name ?? 'untitled')}
</div>
