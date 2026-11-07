<script lang="ts">
	import { parseDelimited } from '../api';
	import Grid from './Grid.svelte';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let rows = $state<string[][]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);

	$effect(() => {
		const current = ctx;
		loading = true;
		error = null;
		current
			.text()
			.then((body) => (rows = parseDelimited(body.text)))
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'))
			.finally(() => (loading = false));
	});
</script>

{#if loading}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else}
	<Grid {rows} />
	{#if rows.length >= 500}<p class="p-2 text-xs text-muted-foreground">
			Showing the first 500 rows. Download for the rest.
		</p>{/if}
{/if}
