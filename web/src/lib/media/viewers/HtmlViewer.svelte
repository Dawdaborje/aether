<script lang="ts">
	import SafeHtmlFrame from './SafeHtmlFrame.svelte';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let html = $state('');
	let cut = $state(false);
	let error = $state<string | null>(null);
	let loading = $state(true);

	$effect(() => {
		const current = ctx;
		loading = true;
		error = null;
		current
			.text()
			.then((body) => {
				html = body.text;
				cut = body.cut;
			})
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'))
			.finally(() => (loading = false));
	});
</script>

{#if loading}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else}
	<div class="flex h-full flex-col">
		<p
			class="shrink-0 border-b border-border bg-background px-3 py-1 text-xs text-muted-foreground"
		>
			Scripts, forms and outside content are blocked.{cut
				? ' Showing the start of a large file.'
				: ''}
		</p>
		<div class="min-h-0 flex-1"><SafeHtmlFrame {html} /></div>
	</div>
{/if}
