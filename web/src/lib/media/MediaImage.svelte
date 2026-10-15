<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { legacyImageSrc, mediaObjectUrl, type MediaFile } from './api';

	let {
		file = null,
		legacy = '',
		alt = '',
		thumb = true,
		zoomable = false,
		class: className = ''
	}: {
		/** An uploaded picture. */
		file?: MediaFile | null;
		/** An old data URL or web address, shown when there is no uploaded file. */
		legacy?: unknown;
		alt?: string;
		/** The small copy (256 px), which is what a list or a preview needs. */
		thumb?: boolean;
		/** Click to see the whole picture in a pop-up. */
		zoomable?: boolean;
		class?: string;
	} = $props();

	let loaded = $state<string>('');
	let failed = $state(false);

	const fallback = $derived(legacyImageSrc(legacy));

	$effect(() => {
		const key = file?.key;
		const small = thumb;
		loaded = '';
		failed = false;
		if (!key) return;
		let current = true;
		mediaObjectUrl(key, small).then(
			(url) => {
				if (current) loaded = url;
			},
			() => {
				if (current) failed = true;
			}
		);
		return () => {
			current = false;
		};
	});

	const src = $derived(file ? loaded : fallback);

	// The pop-up shows the full-size picture, fetched only when it is opened.
	let zoomOpen = $state(false);
	let full = $state('');
	$effect(() => {
		if (!zoomOpen) return;
		const key = file?.key;
		if (!key) {
			full = fallback;
			return;
		}
		full = '';
		let current = true;
		mediaObjectUrl(key, false).then(
			(url) => {
				if (current) full = url;
			},
			() => {
				if (current) full = loaded;
			}
		);
		return () => {
			current = false;
		};
	});
</script>

{#if src && !failed}
	{#if zoomable}
		<button
			type="button"
			class="inline-flex cursor-zoom-in rounded-[inherit] focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
			aria-label={alt ? `View ${alt}` : 'View picture'}
			onclick={(event) => {
				// A picture in a clickable row or card must not also open the row.
				event.stopPropagation();
				zoomOpen = true;
			}}
		>
			<img {src} {alt} class={className} onerror={() => (failed = true)} />
		</button>
		<Dialog.Root bind:open={zoomOpen}>
			<Dialog.Content class="flex max-h-[92vh] w-auto max-w-[min(96vw,64rem)] flex-col items-center gap-3">
				<Dialog.Header class="sr-only">
					<Dialog.Title>{alt || 'Picture'}</Dialog.Title>
					<Dialog.Description>The full picture.</Dialog.Description>
				</Dialog.Header>
				{#if full}
					<img src={full} alt={alt} class="max-h-[80vh] max-w-full rounded-md object-contain" />
				{:else}
					<p class="p-8 text-sm text-muted-foreground">Loading…</p>
				{/if}
			</Dialog.Content>
		</Dialog.Root>
	{:else}
		<img {src} {alt} class={className} onerror={() => (failed = true)} />
	{/if}
{:else}
	<span
		class="{className} bg-muted"
		role={failed ? 'img' : undefined}
		aria-label={failed ? 'Picture unavailable' : undefined}
	></span>
{/if}
