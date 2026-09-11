<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { Skeleton } from '$lib/components/ui/skeleton';

	let { node: _node }: { node: PageNode } = $props();
</script>

<div class="space-y-3">
	<Skeleton class="h-8 w-1/3" />
	<Skeleton class="h-24 w-full" />
	<div class="grid grid-cols-3 gap-3">
		<Skeleton class="h-10 w-full" />
		<Skeleton class="h-10 w-full" />
		<Skeleton class="h-10 w-full" />
	</div>
</div>
