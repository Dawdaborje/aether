<script lang="ts">
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let url = $state('');
	let error = $state<string | null>(null);
	const video = $derived(
		ctx.file.mime.startsWith('video/') || /\.(mp4|webm|ogv|mov|m4v)$/i.test(ctx.file.name)
	);

	$effect(() => {
		const current = ctx;
		url = '';
		error = null;
		current
			.objectUrl()
			.then((u) => (url = u))
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'));
	});
</script>

{#if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else if !url}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if video}
	<!-- svelte-ignore a11y_media_has_caption -->
	<video src={url} controls class="mx-auto max-h-full max-w-full"></video>
{:else}
	<div class="flex h-full items-center justify-center p-6">
		<audio src={url} controls class="w-full max-w-xl"></audio>
	</div>
{/if}
