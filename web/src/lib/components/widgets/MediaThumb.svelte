<script lang="ts">
	import { mediaObjectUrl, type MediaFile } from '$lib/media/api';

	/**
	 * A picture attached to a message, shown as a small preview. Clicking it calls `onopen`. The box has a fixed
	 * size while the thumbnail loads, so the conversation does not jump when it arrives. A GIF is shown as the
	 * original file, since the thumbnail is a still, and plays in place: it is not a button and opens no viewer.
	 */
	let { file, onopen }: { file: MediaFile; onopen: () => void } = $props();

	const animated = $derived(
		file.mime === 'image/gif' || (!file.mime && file.name.toLowerCase().endsWith('.gif'))
	);

	let src = $state<string | null>(null);
	let failed = $state(false);

	$effect(() => {
		const key = file.key;
		const still = !animated;
		src = null;
		failed = false;
		mediaObjectUrl(key, still)
			.then((url) => {
				if (key === file.key) src = url;
			})
			.catch(() => {
				if (key === file.key) failed = true;
			});
	});
</script>

{#snippet picture()}
	{#if src}
		<img {src} alt={file.name} class="block max-h-60 max-w-full object-contain" />
	{:else}
		<span
			class="grid h-40 w-56 max-w-full place-items-center text-xs text-muted-foreground {failed
				? ''
				: 'animate-pulse'}">{failed ? file.name : ''}</span
		>
	{/if}
{/snippet}

{#if animated}
	<div
		class="block overflow-hidden rounded-lg border border-border bg-background/60"
		title={file.name}
		data-part="image-thumb"
	>
		{@render picture()}
	</div>
{:else}
	<button
		type="button"
		class="block overflow-hidden rounded-lg border border-border bg-background/60 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
		aria-label="Open {file.name}"
		title={file.name}
		data-part="image-thumb"
		onclick={onopen}
	>
		{@render picture()}
	</button>
{/if}
