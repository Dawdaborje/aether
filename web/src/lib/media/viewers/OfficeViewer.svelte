<script lang="ts">
	import { previewOffice, type OfficePreview } from '../officePreview';
	import DocBlocks from './DocBlocks.svelte';
	import Grid from './Grid.svelte';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let office = $state<OfficePreview | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let sheet = $state(0);

	$effect(() => {
		const current = ctx;
		loading = true;
		error = null;
		office = null;
		sheet = 0;
		const ext = current.file.name.toLowerCase().split('.').pop() ?? '';
		if (!['docx', 'xlsx', 'pptx'].includes(ext)) {
			error = 'This kind of document is previewed after you download it.';
			loading = false;
			return;
		}
		current
			.bytes()
			.then((buffer) => previewOffice(buffer, ext))
			.then((result) => (office = result))
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'))
			.finally(() => (loading = false));
	});
</script>

{#if loading}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else if office?.kind === 'workbook'}
	<div class="flex h-full flex-col">
		<div
			class="flex shrink-0 gap-1 overflow-x-auto border-b border-border bg-background px-2 py-1"
			role="tablist"
		>
			{#each office.sheets as s, i (i)}
				<button
					type="button"
					role="tab"
					aria-selected={i === sheet}
					class="rounded px-2 py-0.5 text-xs whitespace-nowrap {i === sheet
						? 'bg-secondary font-medium'
						: 'text-muted-foreground hover:bg-muted'}"
					onclick={() => (sheet = i)}>{s.name}</button
				>
			{/each}
		</div>
		<div class="min-h-0 flex-1 overflow-auto">
			<Grid rows={office.sheets[sheet]?.rows ?? []} />
			<p class="p-2 text-xs text-muted-foreground">
				Values only, first 500 rows. Download for formulas and formatting.
			</p>
		</div>
	</div>
{:else if office?.kind === 'slides'}
	<div class="space-y-3 p-4">
		{#each office.slides as lines, i (i)}
			<section class="rounded-md border border-border bg-background p-4">
				<h3 class="mb-2 text-xs font-medium text-muted-foreground">Slide {i + 1}</h3>
				{#each lines as line, k (k)}
					<p class="{k === 0 ? 'text-base font-semibold' : 'text-sm'} break-words">{line}</p>
				{:else}
					<p class="text-sm text-muted-foreground">(no text)</p>
				{/each}
			</section>
		{/each}
		<p class="text-xs text-muted-foreground">Text of each slide. Download for layout and images.</p>
	</div>
{:else if office?.kind === 'doc'}
	<div class="mx-auto max-w-3xl space-y-1 bg-background p-6 text-sm">
		<DocBlocks blocks={office.blocks} />
	</div>
	<p class="p-2 text-xs text-muted-foreground">
		{office.cut ? 'Showing the start of a long document. ' : ''}Text and tables; download for images
		and layout.
	</p>
{/if}
