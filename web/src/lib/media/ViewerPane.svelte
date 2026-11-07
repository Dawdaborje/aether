<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		downloadMedia,
		mediaBytes,
		mediaExtractedText,
		mediaObjectUrl,
		mediaText,
		type ExtractedText,
		type MediaFile
	} from './api';
	import { registerBuiltinViewers } from './viewers/builtin';
	import { ensurePluginViewers } from './viewers/plugin';
	import { resolveViewer } from './viewers/registry';
	import type { ViewerContext } from './viewers/types';

	/**
	 * The universal viewer: picks the viewer for a file from the registry and wraps it with the
	 * actions every file has (download, print, extracted text). Used by the dialog, and anywhere a file
	 * is shown inline.
	 */
	let { file }: { file: MediaFile } = $props();

	registerBuiltinViewers();

	// Plugin viewers are listed once; until they are, the built-in ones answer.
	let pluginsReady = $state(false);
	void ensurePluginViewers().then(() => (pluginsReady = true));

	let printFn = $state<(() => void) | null>(null);
	let showText = $state(false);
	let extracting = $state(false);
	let extracted = $state<ExtractedText | null>(null);
	let extractError = $state<string | null>(null);
	let actionError = $state<string | null>(null);

	const def = $derived((void pluginsReady, resolveViewer(file)));
	const Viewer = $derived(def?.component);

	// A new context per file: bytes are fetched once and shared by whatever asks for them.
	const ctx = $derived.by((): ViewerContext => {
		const target = file;
		let bytes: Promise<ArrayBuffer> | null = null;
		return {
			file: target,
			objectUrl: () => mediaObjectUrl(target.key),
			bytes: () => (bytes ??= mediaBytes(target)),
			text: (limit) => mediaText(target, limit),
			setPrint: (fn) => (printFn = fn)
		};
	});

	$effect(() => {
		// Opening another file starts from a clean slate.
		void file.key;
		showText = false;
		extracted = null;
		extractError = null;
		actionError = null;
		printFn = null;
	});

	async function toggleText() {
		showText = !showText;
		if (!showText || extracted || extracting) return;
		extracting = true;
		extractError = null;
		try {
			extracted = await mediaExtractedText(file);
		} catch (cause) {
			extractError = cause instanceof Error ? cause.message : 'The text could not be read.';
		} finally {
			extracting = false;
		}
	}

	async function save() {
		actionError = null;
		try {
			await downloadMedia(file);
		} catch (cause) {
			actionError = cause instanceof Error ? cause.message : 'The download failed.';
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col gap-2">
	<div class="flex shrink-0 items-center gap-1">
		{#if printFn}
			<Button size="icon-xs" variant="ghost" aria-label="Print" onclick={() => printFn?.()}
				><AppIcon name="printer" /></Button
			>
		{/if}
		{#if def?.canExtract}
			<Button
				size="xs"
				variant={showText ? 'secondary' : 'ghost'}
				aria-pressed={showText}
				onclick={() => void toggleText()}
			>
				<AppIcon name="scan-text" /> Extracted text
			</Button>
		{/if}
		<Button size="icon-xs" variant="ghost" aria-label="Download" onclick={() => void save()}
			><AppIcon name="download" /></Button
		>
		{#if actionError}<span class="text-xs text-destructive" role="alert">{actionError}</span>{/if}
	</div>

	<div class="min-h-0 flex-1 overflow-auto rounded-md border border-border bg-muted/30">
		{#if showText && def?.canExtract}
			<div class="space-y-2 p-4 text-sm">
				{#if extracting}
					<p class="text-muted-foreground">Reading the document… a scan can take a little while.</p>
				{:else if extractError}
					<p class="text-destructive" role="alert">{extractError}</p>
					<Button
						size="sm"
						variant="outline"
						onclick={() => {
							extractError = null;
							showText = false;
							void toggleText();
						}}>Try again</Button
					>
				{:else if extracted}
					{#if extracted.ocr_used}<p class="text-xs text-muted-foreground">
							Some pages were read from images, so the text may have mistakes.
						</p>{/if}
					{#each extracted.pages as page (page.page)}
						<section>
							{#if extracted.pages.length > 1}<h3 class="text-xs font-medium text-muted-foreground">
									Page {page.page}
								</h3>{/if}
							<pre class="text-xs break-words whitespace-pre-wrap">{page.text || '(no text)'}</pre>
						</section>
					{:else}
						<pre class="text-xs break-words whitespace-pre-wrap">{extracted.text ||
								'(no text found)'}</pre>
					{/each}
					{#if extracted.truncated}<p class="text-xs text-muted-foreground">
							Only the start is shown: the file has more text.
						</p>{/if}
				{/if}
			</div>
		{:else if Viewer}
			{#key file.key}
				<Viewer {ctx} spec={def?.spec} />
			{/key}
		{/if}
	</div>
</div>
