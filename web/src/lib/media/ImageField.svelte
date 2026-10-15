<script lang="ts">
	import ImageIcon from '@lucide/svelte/icons/image';
	import UploadIcon from '@lucide/svelte/icons/upload';
	import XIcon from '@lucide/svelte/icons/x';
	import { Button } from '$lib/components/ui/button';
	import { filesOf, legacyImageSrc, uploadMedia, type MediaFile } from './api';
	import MediaImage from './MediaImage.svelte';

	let {
		id,
		value,
		multiple = false,
		avatar = false,
		onchange
	}: {
		id?: string;
		/** What the field holds: an uploaded file, a list of them, or old text. */
		value: unknown;
		multiple?: boolean;
		/** A round picture, for a person's photo or a logo. */
		avatar?: boolean;
		onchange: (next: MediaFile | MediaFile[] | null) => void;
	} = $props();

	let busy = $state(false);
	let error = $state<string | null>(null);
	let dragging = $state(false);
	let input = $state<HTMLInputElement | null>(null);

	const files = $derived(filesOf(value));
	const legacy = $derived(files.length === 0 ? legacyImageSrc(value) : '');
	const hasValue = $derived(files.length > 0 || legacy !== '');
	const shape = $derived(avatar ? 'rounded-full' : 'rounded-lg');

	async function accept(list: FileList | File[] | null) {
		const picked = Array.from(list ?? []).filter(Boolean);
		if (picked.length === 0) return;
		const images = picked.filter((file) => file.type.startsWith('image/'));
		if (images.length !== picked.length) {
			error = 'Choose picture files (PNG, JPEG, WebP or GIF).';
			return;
		}
		busy = true;
		error = null;
		try {
			const uploaded: MediaFile[] = [];
			for (const file of multiple ? images : images.slice(0, 1)) {
				uploaded.push(await uploadMedia(file));
			}
			onchange(multiple ? [...files, ...uploaded] : (uploaded[0] ?? null));
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The upload failed.';
		} finally {
			busy = false;
			if (input) input.value = '';
		}
	}

	function remove(key: string | null) {
		error = null;
		if (!multiple || key === null) onchange(multiple ? [] : null);
		else {
			const rest = files.filter((file) => file.key !== key);
			onchange(rest);
		}
	}
</script>

<div class="flex flex-wrap items-start gap-3">
	{#each files as file (file.key)}
		<div class="group relative">
			<MediaImage
				{file}
				alt={file.name}
				zoomable
				class="size-24 {shape} border border-border object-cover"
			/>
			<button
				type="button"
				class="absolute -top-1.5 -right-1.5 flex size-6 items-center justify-center rounded-full border border-border bg-card text-muted-foreground opacity-0 transition group-hover:opacity-100 hover:text-foreground focus-visible:opacity-100"
				aria-label="Remove {file.name}"
				onclick={() => remove(file.key)}
			>
				<XIcon class="size-3.5" />
			</button>
		</div>
	{/each}

	{#if legacy}
		<div class="relative">
			<MediaImage
				{legacy}
				alt=""
				zoomable
				class="size-24 {shape} border border-border object-cover"
			/>
		</div>
	{/if}

	{#if multiple || !hasValue}
		<!-- A drop target that is also the button: click, or drop pictures on it. -->
		<button
			type="button"
			disabled={busy}
			class="flex size-24 flex-col items-center justify-center gap-1 {shape} border border-dashed text-xs text-muted-foreground transition hover:bg-accent disabled:opacity-60 {dragging
				? 'border-primary bg-accent'
				: 'border-border'}"
			ondragover={(event) => {
				event.preventDefault();
				dragging = true;
			}}
			ondragleave={() => (dragging = false)}
			ondrop={(event) => {
				event.preventDefault();
				dragging = false;
				void accept(event.dataTransfer?.files ?? null);
			}}
			onclick={() => input?.click()}
		>
			{#if busy}
				<span>Uploading…</span>
			{:else}
				<ImageIcon class="size-5" />
				<span>{multiple ? 'Add pictures' : 'Add picture'}</span>
			{/if}
		</button>
	{:else}
		<div class="flex flex-col gap-2 self-center">
			<Button
				type="button"
				variant="outline"
				size="sm"
				disabled={busy}
				onclick={() => input?.click()}
			>
				<UploadIcon />
				{busy ? 'Uploading…' : 'Replace'}
			</Button>
			<Button type="button" variant="ghost" size="sm" onclick={() => remove(null)}>Remove</Button>
		</div>
	{/if}

	<input
		{id}
		bind:this={input}
		type="file"
		{multiple}
		accept="image/png,image/jpeg,image/webp,image/gif"
		class="sr-only"
		onchange={(event) => void accept(event.currentTarget.files)}
	/>
</div>

{#if error}
	<p class="mt-1.5 text-xs text-destructive" role="alert">{error}</p>
{/if}
