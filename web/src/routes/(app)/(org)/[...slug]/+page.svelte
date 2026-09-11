<script lang="ts">
	import { buildPage } from '$lib/dsl';
	import type { PageDef } from '$lib/dsl/types';
	import PageRenderer from '$lib/components/widgets/PageRenderer.svelte';
	import SpinnerWidget from '$lib/components/widgets/SpinnerWidget.svelte';
	import SkeletonWidget from '$lib/components/widgets/SkeletonWidget.svelte';
	import * as Alert from '$lib/components/ui/alert/index.js';

	let { params } = $props();

	let loading = $state(true);
	let error = $state<string | null>(null);
	let page = $state<PageDef | null>(null);
	let warnings = $state<string[]>([]);

	async function loadPage(slug: string) {
		loading = true;
		error = null;
		try {
			const res = await fetch(`/api/ui/pages/${slug}`);
			if (!res.ok) throw new Error(`Failed to load page (${res.status})`);
			const data = await res.json();
			const built = buildPage(data.page);
			page = built.page;
			warnings = built.warnings;
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to load page';
			page = null;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void loadPage(params.slug);
	});
</script>

{#if loading}
	<div class="p-6">
		<SpinnerWidget node={{ type: 'spinner', label: 'Loading page…', inline: true }} />
		<div class="mt-4">
			<SkeletonWidget node={{ type: 'skeleton' }} />
		</div>
	</div>
{:else if error}
	<div class="p-6">
		<Alert.Root>
			<Alert.Title>Unable to load page</Alert.Title>
			<Alert.Description>{error}</Alert.Description>
		</Alert.Root>
	</div>
{:else if page}
	{#if warnings.length && import.meta.env.DEV}
		<div
			class="border-b border-amber-500/30 bg-amber-500/5 px-4 py-2 text-xs text-amber-800 dark:text-amber-200"
		>
			DSL warnings: {warnings.join('; ')}
		</div>
	{/if}
	<PageRenderer node={page} />
{/if}
