<script lang="ts">
	import DownloadIcon from '@lucide/svelte/icons/download';
	import FileIcon from '@lucide/svelte/icons/file-text';
	import PaperclipIcon from '@lucide/svelte/icons/paperclip';
	import XIcon from '@lucide/svelte/icons/x';
	import { Button } from '$lib/components/ui/button';
	import { downloadMedia, filesOf, formatSize, uploadMedia, type MediaFile } from './api';

	let {
		id,
		value,
		multiple = false,
		onchange
	}: {
		id?: string;
		/** What the field holds: an uploaded file, or a list of them. */
		value: unknown;
		multiple?: boolean;
		onchange: (next: MediaFile | MediaFile[] | null) => void;
	} = $props();

	let busy = $state(false);
	let error = $state<string | null>(null);
	let dragging = $state(false);
	let input = $state<HTMLInputElement | null>(null);

	const files = $derived(filesOf(value));
	/** Text from before the field held files (a list of storage keys, say): shown as it is. */
	const legacy = $derived(files.length === 0 && typeof value === 'string' ? value.trim() : '');

	async function accept(list: FileList | File[] | null) {
		const picked = Array.from(list ?? []);
		if (picked.length === 0) return;
		busy = true;
		error = null;
		try {
			const uploaded: MediaFile[] = [];
			for (const file of multiple ? picked : picked.slice(0, 1)) {
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

	function remove(key: string) {
		error = null;
		onchange(multiple ? files.filter((file) => file.key !== key) : null);
	}

	async function save(file: MediaFile) {
		error = null;
		try {
			await downloadMedia(file);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The download failed.';
		}
	}
</script>

<div class="space-y-2">
	{#if files.length > 0}
		<ul class="divide-y divide-border rounded-md border border-border">
			{#each files as file (file.key)}
				<li class="flex items-center gap-3 px-3 py-2 text-sm">
					<FileIcon class="size-4 shrink-0 text-muted-foreground" />
					<span class="min-w-0 flex-1 truncate">{file.name}</span>
					<span class="shrink-0 text-xs text-muted-foreground">{formatSize(file.size)}</span>
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label="Download {file.name}"
						onclick={() => void save(file)}
					>
						<DownloadIcon />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label="Remove {file.name}"
						onclick={() => remove(file.key)}
					>
						<XIcon />
					</Button>
				</li>
			{/each}
		</ul>
	{:else if legacy}
		<p
			class="rounded-md border border-border bg-muted/40 px-3 py-2 text-sm break-all text-muted-foreground"
		>
			{legacy.startsWith('data:') ? 'An older attachment' : legacy}
		</p>
	{/if}

	{#if multiple || files.length === 0}
		<button
			type="button"
			disabled={busy}
			class="flex w-full items-center justify-center gap-2 rounded-md border border-dashed px-3 py-4 text-sm text-muted-foreground transition hover:bg-accent disabled:opacity-60 {dragging
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
			<PaperclipIcon class="size-4" />
			{busy
				? 'Uploading…'
				: multiple
					? 'Attach files, or drop them here'
					: 'Attach a file, or drop it here'}
		</button>
	{/if}

	<input
		{id}
		bind:this={input}
		type="file"
		{multiple}
		class="sr-only"
		onchange={(event) => void accept(event.currentTarget.files)}
	/>

	{#if error}
		<p class="text-xs text-destructive" role="alert">{error}</p>
	{/if}
</div>
