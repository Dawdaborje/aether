<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import type { PDFDocumentProxy } from 'pdfjs-dist';
	import { Button } from '$lib/components/ui/button';
	import PdfPage from './PdfPage.svelte';
	import { openPdf, searchPdf } from './pdf';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();

	let doc = $state<PDFDocumentProxy | null>(null);
	let error = $state<string | null>(null);
	let scale = $state(1.2);
	let current = $state(1);
	let thumbs = $state(true);
	let query = $state('');
	let hits = $state<number[]>([]);
	let hit = $state(0);
	let searching = $state(false);
	let scroller = $state<HTMLElement | null>(null);
	let searchId = 0;
	let searched = $state(false);

	$effect(() => {
		const target = ctx;
		let alive = true;
		let opened: PDFDocumentProxy | null = null;
		doc = null;
		error = null;
		current = 1;
		hits = [];
		searched = false;
		query = '';
		target
			.bytes()
			.then(openPdf)
			.then((d) => {
				opened = d;
				if (alive) doc = d;
				else void d.loadingTask.destroy();
			})
			.catch((e) => {
				if (alive) error = e instanceof Error ? e.message : 'The file could not be opened.';
			});
		// Printing uses the browser's own PDF viewer, which prints the whole document.
		target.setPrint(() => {
			void target.objectUrl().then((url) => {
				const frame = document.createElement('iframe');
				frame.style.cssText = 'position:fixed;width:0;height:0;border:0';
				frame.src = url;
				frame.onload = () => {
					frame.contentWindow?.print();
					setTimeout(() => frame.remove(), 60_000);
				};
				document.body.appendChild(frame);
			});
		});
		return () => {
			alive = false;
			searchId++;
			target.setPrint(null);
			void opened?.loadingTask.destroy();
		};
	});

	function show(page: number) {
		if (!doc) return;
		const n = Math.min(Math.max(1, page), doc.numPages);
		current = n;
		scroller?.querySelector(`[data-page="${n}"]`)?.scrollIntoView({ block: 'start' });
	}

	// The page in view follows the scroll position.
	function onscroll() {
		if (!scroller) return;
		const top = scroller.getBoundingClientRect().top + 8;
		for (const el of scroller.querySelectorAll<HTMLElement>('[data-page]')) {
			if (el.getBoundingClientRect().bottom > top) {
				current = Number(el.dataset.page);
				break;
			}
		}
	}

	async function find() {
		if (!doc) return;
		const id = ++searchId;
		searching = true;
		searched = true;
		hits = await searchPdf(doc, query, () => id !== searchId);
		if (id !== searchId) return;
		searching = false;
		hit = 0;
		if (hits.length > 0) show(hits[0]);
	}

	function step(by: number) {
		if (hits.length === 0) return;
		hit = (hit + by + hits.length) % hits.length;
		show(hits[hit]);
	}
</script>

{#if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else if !doc}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else}
	<div class="flex h-full flex-col">
		<div
			class="flex shrink-0 flex-wrap items-center gap-1 border-b border-border bg-background px-2 py-1"
		>
			<Button
				size="icon-xs"
				variant={thumbs ? 'secondary' : 'ghost'}
				aria-label="Thumbnails"
				aria-pressed={thumbs}
				onclick={() => (thumbs = !thumbs)}><AppIcon name="side-panel-open" /></Button
			>
			<input
				type="number"
				min="1"
				max={doc.numPages}
				value={current}
				aria-label="Page"
				class="h-6 w-12 rounded border border-border bg-background px-1 text-xs"
				onchange={(e) => show(Number(e.currentTarget.value))}
			/>
			<span class="text-xs text-muted-foreground">/ {doc.numPages}</span>
			<Button
				size="icon-xs"
				variant="ghost"
				aria-label="Zoom out"
				onclick={() => (scale = Math.max(0.5, scale - 0.2))}><AppIcon name="zoom-out" /></Button
			>
			<span class="w-10 text-center text-xs text-muted-foreground">{Math.round(scale * 100)}%</span>
			<Button
				size="icon-xs"
				variant="ghost"
				aria-label="Zoom in"
				onclick={() => (scale = Math.min(3, scale + 0.2))}><AppIcon name="zoom-in" /></Button
			>
			<form
				class="ml-auto flex items-center gap-1"
				onsubmit={(e) => {
					e.preventDefault();
					void find();
				}}
			>
				<AppIcon name="search" class="size-3.5 text-muted-foreground" />
				<input
					bind:value={query}
					placeholder="Search"
					aria-label="Search the document"
					class="h-6 w-36 rounded border border-border bg-background px-1 text-xs"
				/>
				{#if searching}<span class="text-xs text-muted-foreground">…</span>
				{:else if hits.length > 0}<span class="text-xs text-muted-foreground"
						>{hit + 1}/{hits.length}</span
					>
				{:else if query && searched}<span class="text-xs text-muted-foreground">0</span>{/if}
				<Button
					type="button"
					size="icon-xs"
					variant="ghost"
					aria-label="Previous match"
					onclick={() => step(-1)}><AppIcon name="chevron-up" /></Button
				>
				<Button
					type="button"
					size="icon-xs"
					variant="ghost"
					aria-label="Next match"
					onclick={() => step(1)}><AppIcon name="chevron-down" /></Button
				>
			</form>
		</div>
		<div class="flex min-h-0 flex-1">
			{#if thumbs}
				<div
					class="w-28 shrink-0 space-y-2 overflow-y-auto border-r border-border bg-muted/40 p-2"
					aria-label="Pages"
				>
					{#each { length: doc.numPages } as _, i (i)}
						<button
							type="button"
							class="block w-full text-center {current === i + 1 ? 'ring-2 ring-primary' : ''}"
							aria-label="Page {i + 1}"
							onclick={() => show(i + 1)}
						>
							<PdfPage {doc} number={i + 1} scale={0.15} />
							<span class="text-[10px] text-muted-foreground">{i + 1}</span>
						</button>
					{/each}
				</div>
			{/if}
			<div
				bind:this={scroller}
				{onscroll}
				class="min-w-0 flex-1 space-y-3 overflow-auto bg-muted/30 p-3"
			>
				{#each { length: doc.numPages } as _, i (i)}
					<PdfPage {doc} number={i + 1} {scale} root={scroller} />
				{/each}
			</div>
		</div>
	</div>
{/if}
