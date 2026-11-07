<script lang="ts">
	import { cn } from '$lib/utils.js';
	import Skeleton from './skeleton.svelte';

	let {
		rows = 6,
		columns = 4,
		label = 'Loading…',
		class: className
	}: {
		rows?: number;
		columns?: number;
		/** Read out to screen readers; nothing is shown as text. */
		label?: string;
		class?: string;
	} = $props();

	// Varied but fixed widths, so the placeholder looks like text and does not shimmer differently each time.
	const WIDTHS = ['w-3/4', 'w-1/2', 'w-2/3', 'w-5/6', 'w-1/3'];
	const widthFor = (row: number, column: number) => WIDTHS[(row * 2 + column) % WIDTHS.length];
</script>

<div
	role="status"
	aria-live="polite"
	aria-busy="true"
	class={cn('overflow-hidden rounded-md border border-border', className)}
>
	<span class="sr-only">{label}</span>
	<div aria-hidden="true">
		<div class="flex gap-4 bg-muted/40 px-3 py-3">
			{#each Array.from({ length: columns }, (_, c) => c) as column (column)}
				<Skeleton class="h-3 flex-1 bg-muted-foreground/20" />
			{/each}
		</div>
		{#each Array.from({ length: rows }, (_, r) => r) as row (row)}
			<div class="flex gap-4 border-t border-border px-3 py-3.5">
				{#each Array.from({ length: columns }, (_, c) => c) as column (column)}
					<div class="flex-1"><Skeleton class="h-3.5 {widthFor(row, column)}" /></div>
				{/each}
			</div>
		{/each}
	</div>
</div>
