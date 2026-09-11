<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { LoaderCircleIcon } from '@lucide/svelte';

	let { node }: { node: PageNode } = $props();

	const inline = $derived(node.inline === true);
</script>

{#if inline}
	<div class="inline-flex items-center gap-2 text-sm text-muted-foreground">
		<LoaderCircleIcon class="size-4 animate-spin" />
		{String(node.label ?? 'Loading…')}
	</div>
{:else}
	<!-- Hidden by default on page shell; used as overlay when pageLoading is true via host -->
	<div
		class="pointer-events-none fixed inset-0 z-40 hidden items-center justify-center bg-background/60"
		data-aether-spinner
	>
		<div class="flex items-center gap-2 rounded-md border border-border bg-card px-4 py-3 shadow-sm">
			<LoaderCircleIcon class="size-5 animate-spin text-primary" />
			<span class="text-sm text-foreground">{String(node.label ?? 'Loading…')}</span>
		</div>
	</div>
{/if}
