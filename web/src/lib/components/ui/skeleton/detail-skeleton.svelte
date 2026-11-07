<script lang="ts">
	import { cn } from '$lib/utils.js';
	import Skeleton from './skeleton.svelte';

	let {
		fields = 4,
		label = 'Loading…',
		class: className
	}: {
		/** How many label-and-value pairs to suggest below the heading. */
		fields?: number;
		/** Read out to screen readers; nothing is shown as text. */
		label?: string;
		class?: string;
	} = $props();
</script>

<div role="status" aria-live="polite" aria-busy="true" class={cn('space-y-5', className)}>
	<span class="sr-only">{label}</span>
	<div aria-hidden="true" class="space-y-5">
		<div class="space-y-2">
			<Skeleton class="h-6 w-56" />
			<Skeleton class="h-3.5 w-80 max-w-full" />
		</div>
		<div class="grid gap-4 md:grid-cols-2">
			{#each Array.from({ length: fields }, (_, i) => i) as i (i)}
				<div class="space-y-2">
					<Skeleton class="h-3 w-24" />
					<Skeleton class="h-9 w-full" />
				</div>
			{/each}
		</div>
	</div>
</div>
