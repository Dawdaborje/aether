<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';

	let { node }: { node: PageNode } = $props();
</script>

<div class="rounded-md border border-border bg-card p-4 shadow-sm">
	<p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
		{String(node.label ?? node.title ?? 'Metric')}
	</p>
	<p class="mt-1 text-2xl font-semibold text-foreground">
		{String(node.value ?? '—')}
	</p>
</div>
