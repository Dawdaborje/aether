<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { formatSize, mediaObjectUrl, viewKind } from '$lib/media/api';
	import { fileTag } from '$lib/media/fileTag';

	/**
	 * A file waiting to be sent, drawn as a preview card above the text of the composer: a thumbnail for a picture
	 * (the file itself while it uploads, the stored thumbnail afterwards) or a coloured tile with the kind of file,
	 * its name and size, a spinner while it uploads, and a button to take it off the message.
	 */
	let {
		name,
		size,
		mime = '',
		key = null,
		local = null,
		uploading = false,
		onremove
	}: {
		name: string;
		size: number;
		mime?: string;
		/** Media key once uploaded. */
		key?: string | null;
		/** Object URL of the picked file, for a preview before (or without) the stored one. */
		local?: string | null;
		uploading?: boolean;
		onremove: () => void;
	} = $props();

	const kind = $derived(viewKind({ name, mime }));
	let stored = $state<string | null>(null);
	const src = $derived(kind === 'image' ? (local ?? stored) : null);

	$effect(() => {
		const k = key;
		stored = null;
		if (kind !== 'image' || !k || local) return;
		void mediaObjectUrl(k, true).then(
			(url) => {
				if (k === key) stored = url;
			},
			() => {}
		);
	});

	const tag = $derived(fileTag({ name, mime }));
</script>

<li
	class="group relative flex w-56 items-center gap-2 overflow-hidden rounded-lg border border-border bg-muted/40 p-1.5 pr-8"
	data-part="composer-file"
	aria-busy={uploading}
>
	<span
		class="relative grid size-12 shrink-0 place-items-center overflow-hidden rounded-md {src
			? 'bg-muted'
			: tag.colour} text-[11px] font-semibold text-white"
	>
		{#if src}
			<img {src} alt="" class="size-full object-cover" />
		{:else}
			<AppIcon name="file-text" class="absolute top-1 left-1 size-3 opacity-70" />
			<span class="mt-2">{tag.label}</span>
		{/if}
		{#if uploading}
			<span class="absolute inset-0 grid place-items-center bg-background/70" role="status">
				<span class="size-5 animate-spin rounded-full border-2 border-primary border-t-transparent"
				></span>
				<span class="sr-only">Uploading</span>
			</span>
		{/if}
	</span>
	<span class="min-w-0 flex-1">
		<span class="block truncate text-sm font-medium" title={name}>{name}</span>
		<span class="block text-xs text-muted-foreground"
			>{uploading ? 'Uploading…' : formatSize(size)}</span
		>
	</span>
	<button
		type="button"
		class="absolute top-1 right-1 grid size-6 place-items-center rounded-full text-muted-foreground hover:bg-background hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
		aria-label="Remove {name}"
		onclick={onremove}><AppIcon name="close" class="size-3.5" /></button
	>
</li>
