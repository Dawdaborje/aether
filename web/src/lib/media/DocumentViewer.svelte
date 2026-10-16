<script lang="ts">
	import DownloadIcon from '@lucide/svelte/icons/download';
	import PrinterIcon from '@lucide/svelte/icons/printer';
	import ZoomInIcon from '@lucide/svelte/icons/zoom-in';
	import ZoomOutIcon from '@lucide/svelte/icons/zoom-out';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { apiFetch } from '$lib/api/client';
	import { previewOffice, type OfficePreview } from './officePreview';
	import {
		downloadMedia,
		formatSize,
		mediaObjectUrl,
		mediaText,
		parseDelimited,
		viewKind,
		type MediaFile
	} from './api';

	/**
	 * Opens a stored file by its key. The bytes come through the server, so it does not matter
	 * which storage the organization uses (disk, S3-compatible, or another backend).
	 */
	let { file, open = $bindable(false) }: { file: MediaFile | null; open?: boolean } = $props();

	let loading = $state(false);
	let error = $state<string | null>(null);
	let url = $state('');
	let text = $state('');
	let cut = $state(false);
	let rows = $state<string[][]>([]);
	let office = $state<OfficePreview | null>(null);
	let zoom = $state(1);
	let frame = $state<HTMLIFrameElement | null>(null);

	const kind = $derived(file ? viewKind(file) : 'other');

	async function load(target: MediaFile) {
		loading = true;
		error = null;
		url = '';
		text = '';
		rows = [];
		office = null;
		zoom = 1;
		try {
			const k = viewKind(target);
			if (k === 'image' || k === 'pdf') {
				url = await mediaObjectUrl(target.key);
			} else if (k === 'office') {
				const ext = target.name.toLowerCase().split('.').pop() ?? '';
				if (!['docx', 'xlsx', 'pptx'].includes(ext)) throw new Error('This kind of document is previewed after you download it.');
				const response = await apiFetch(`/api/media/${target.key}?name=${encodeURIComponent(target.name)}`);
				if (!response.ok) throw new Error('The file could not be opened.');
				office = await previewOffice(await response.arrayBuffer(), ext);
			} else if (k === 'csv' || k === 'json' || k === 'text') {
				const body = await mediaText(target);
				cut = body.cut;
				if (k === 'csv') rows = parseDelimited(body.text);
				else if (k === 'json') {
					try {
						text = JSON.stringify(JSON.parse(body.text), null, 2);
					} catch {
						text = body.text;
					}
				} else text = body.text;
			}
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The file could not be opened.';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (open && file) void load(file);
	});

	function print() {
		if (kind === 'pdf') frame?.contentWindow?.print();
		else window.print();
	}

	async function save() {
		if (!file) return;
		try {
			await downloadMedia(file);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The download failed.';
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="flex h-[85vh] max-w-5xl flex-col gap-3 sm:max-w-5xl">
		<Dialog.Header class="shrink-0 pr-10">
			<Dialog.Title class="truncate">{file?.name ?? 'File'}</Dialog.Title>
			<Dialog.Description>
				{file ? formatSize(file.size) : ''}{file?.mime ? ` · ${file.mime}` : ''}
			</Dialog.Description>
		</Dialog.Header>

		<div class="flex shrink-0 items-center gap-1">
			{#if kind === 'image'}
				<Button size="icon-xs" variant="ghost" aria-label="Zoom out" onclick={() => (zoom = Math.max(0.25, zoom - 0.25))}><ZoomOutIcon /></Button>
				<Button size="icon-xs" variant="ghost" aria-label="Zoom in" onclick={() => (zoom = Math.min(4, zoom + 0.25))}><ZoomInIcon /></Button>
			{/if}
			{#if kind === 'pdf' || kind === 'image' || kind === 'text' || kind === 'json'}
				<Button size="icon-xs" variant="ghost" aria-label="Print" onclick={print}><PrinterIcon /></Button>
			{/if}
			<Button size="icon-xs" variant="ghost" aria-label="Download" onclick={() => void save()}><DownloadIcon /></Button>
		</div>

		<div class="min-h-0 flex-1 overflow-auto rounded-md border border-border bg-muted/30">
			{#if loading}
				<p class="p-4 text-sm text-muted-foreground">Opening…</p>
			{:else if error}
				<div class="space-y-2 p-4 text-sm">
					<p class="text-destructive" role="alert">{error}</p>
					{#if file}
						<Button size="sm" variant="outline" onclick={() => file && void load(file)}>Try again</Button>
					{/if}
				</div>
			{:else if kind === 'image' && url}
				<div class="flex min-h-full items-center justify-center p-2">
					<img src={url} alt={file?.name} style="width:{zoom * 100}%;max-width:none;height:auto" class="object-contain" />
				</div>
			{:else if kind === 'pdf' && url}
				<iframe bind:this={frame} title={file?.name} src={url} class="h-full w-full border-0"></iframe>
			{:else if kind === 'csv'}
				<table class="w-full border-collapse text-xs">
					<tbody>
						{#each rows as row, r (r)}
							<tr class={r === 0 ? 'bg-muted font-medium' : 'border-t border-border'}>
								{#each row as cell, c (c)}
									<td class="px-2 py-1 whitespace-nowrap">{cell}</td>
								{/each}
							</tr>
						{/each}
					</tbody>
				</table>
				{#if rows.length >= 500}<p class="p-2 text-xs text-muted-foreground">Showing the first 500 rows. Download for the rest.</p>{/if}
			{:else if kind === 'office' && office}
				{#if office.kind === 'table'}
					<table class="w-full border-collapse text-xs">
						<tbody>
							{#each office.rows as row, r (r)}
								<tr class={r === 0 ? 'bg-muted font-medium' : 'border-t border-border'}>
									{#each row as cell, c (c)}
										<td class="px-2 py-1 whitespace-nowrap">{cell}</td>
									{/each}
								</tr>
							{/each}
						</tbody>
					</table>
					<p class="p-2 text-xs text-muted-foreground">First sheet ({office.sheet}), values only. Download for formatting and other sheets.</p>
				{:else}
					<div class="space-y-2 p-4 text-sm">
						{#each office.paragraphs as line, i (i)}
							<p class="break-words">{line || '\u00a0'}</p>
						{/each}
					</div>
					<p class="p-2 text-xs text-muted-foreground">Text only. Download for layout and images.</p>
				{/if}
			{:else if kind === 'text' || kind === 'json'}
				<pre class="p-3 text-xs break-words whitespace-pre-wrap">{text}</pre>
				{#if cut}<p class="p-2 text-xs text-muted-foreground">Showing the start of a large file. Download for all of it.</p>{/if}
			{:else}
				<div class="space-y-2 p-6 text-sm text-muted-foreground">
					<p>
						{kind === 'office'
							? 'This document could not be previewed.'
							: kind === 'archive'
								? 'Archives are not previewed.'
								: 'This kind of file cannot be previewed.'}
					</p>
					<Button size="sm" onclick={() => void save()}>Download {file?.name}</Button>
				</div>
			{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>
