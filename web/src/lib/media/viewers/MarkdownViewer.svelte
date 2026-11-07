<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import MarkdownBlocks from './MarkdownBlocks.svelte';
	import { parseMarkdown, type Block } from './markdown';
	import type { ViewerContext } from './types';

	/**
	 * Markdown as a rendered page (Preview) or as the text it was written in (Source). The page fills the width of
	 * the window it is in; the buttons, Ctrl and the mouse wheel, or Ctrl with + and -, zoom in and out, and Fit
	 * goes back to filling the width.
	 */
	let { ctx }: { ctx: ViewerContext } = $props();
	let blocks = $state<Block[]>([]);
	let source = $state('');
	let cut = $state(false);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let mode = $state<'preview' | 'source'>('preview');
	let zoom = $state(1);

	const MIN = 0.5;
	const MAX = 3;
	const step = (by: number) =>
		(zoom = Math.min(MAX, Math.max(MIN, Math.round((zoom + by) * 100) / 100)));

	$effect(() => {
		const current = ctx;
		loading = true;
		error = null;
		zoom = 1;
		mode = 'preview';
		current.setPrint(() => window.print());
		current
			.text()
			.then((body) => {
				source = body.text;
				blocks = parseMarkdown(body.text);
				cut = body.cut;
			})
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'))
			.finally(() => (loading = false));
		return () => current.setPrint(null);
	});

	function onwheel(event: WheelEvent) {
		if (!event.ctrlKey && !event.metaKey) return;
		event.preventDefault();
		step(event.deltaY < 0 ? 0.1 : -0.1);
	}

	function onkeydown(event: KeyboardEvent) {
		if (!event.ctrlKey && !event.metaKey) return;
		if (event.key === '+' || event.key === '=') step(0.1);
		else if (event.key === '-') step(-0.1);
		else if (event.key === '0') zoom = 1;
		else return;
		event.preventDefault();
	}
</script>

{#if loading}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else}
	<div class="flex h-full flex-col" data-part="markdown-viewer">
		<div class="flex shrink-0 items-center gap-1 border-b border-border bg-background px-2 py-1">
			<div
				class="mr-2 inline-flex overflow-hidden rounded-md border border-border"
				role="group"
				aria-label="View"
			>
				{#each [['preview', 'Preview'], ['source', 'Source']] as [value, label] (value)}
					<button
						type="button"
						class="px-2.5 py-0.5 text-xs {mode === value
							? 'bg-primary text-primary-foreground'
							: 'hover:bg-muted'}"
						aria-pressed={mode === value}
						onclick={() => (mode = value as 'preview' | 'source')}>{label}</button
					>
				{/each}
			</div>
			<Button
				size="icon-xs"
				variant="ghost"
				aria-label="Zoom out"
				disabled={zoom <= MIN}
				onclick={() => step(-0.1)}><AppIcon name="zoom-out" /></Button
			>
			<span class="w-10 text-center text-xs text-muted-foreground" aria-live="polite"
				>{Math.round(zoom * 100)}%</span
			>
			<Button
				size="icon-xs"
				variant="ghost"
				aria-label="Zoom in"
				disabled={zoom >= MAX}
				onclick={() => step(0.1)}><AppIcon name="zoom-in" /></Button
			>
			<Button size="icon-xs" variant="ghost" aria-label="Fit to window" onclick={() => (zoom = 1)}
				><AppIcon name="maximize" /></Button
			>
		</div>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div class="min-h-0 flex-1 overflow-auto" {onwheel} {onkeydown} tabindex="0" role="document">
			<div style="zoom:{zoom}" class="w-full">
				{#if mode === 'preview'}
					<div class="w-full bg-background p-6 text-sm"><MarkdownBlocks {blocks} /></div>
				{:else}
					<pre
						class="w-full p-6 font-mono text-xs break-words whitespace-pre-wrap"
						data-part="markdown-source">{source}</pre>
				{/if}
			</div>
			{#if cut}<p class="p-2 text-xs text-muted-foreground">
					Showing the start of a large file.
				</p>{/if}
		</div>
	</div>
{/if}
