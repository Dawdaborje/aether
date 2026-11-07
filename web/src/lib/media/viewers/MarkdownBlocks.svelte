<script lang="ts">
	import type { Block, Inline } from './markdown';
	import MarkdownBlocks from './MarkdownBlocks.svelte';
	import MarkdownInline from './MarkdownInline.svelte';

	let { blocks }: { blocks: Block[] } = $props();
	const text = (c: Inline[]) => c;
</script>

{#each blocks as block, i (i)}
	{#if block.t === 'heading'}
		{#if block.level === 1}<h1 class="mt-4 mb-2 text-2xl font-semibold">
				<MarkdownInline items={text(block.c)} />
			</h1>
		{:else if block.level === 2}<h2 class="mt-4 mb-2 text-xl font-semibold">
				<MarkdownInline items={text(block.c)} />
			</h2>
		{:else}<h3 class="mt-3 mb-1 font-semibold"><MarkdownInline items={text(block.c)} /></h3>{/if}
	{:else if block.t === 'p'}
		<p class="my-2 break-words"><MarkdownInline items={block.c} /></p>
	{:else if block.t === 'list'}
		{#if block.ordered}
			<ol class="my-2 ml-6 list-decimal">
				{#each block.items as item, k (k)}<li><MarkdownInline items={item} /></li>{/each}
			</ol>
		{:else}
			<ul class="my-2 ml-6 list-disc">
				{#each block.items as item, k (k)}<li><MarkdownInline items={item} /></li>{/each}
			</ul>
		{/if}
	{:else if block.t === 'quote'}
		<blockquote class="my-2 border-l-2 border-border pl-3 text-muted-foreground">
			<MarkdownBlocks blocks={block.c} />
		</blockquote>
	{:else if block.t === 'code'}
		<pre class="my-2 overflow-auto rounded-md bg-muted p-3 text-xs">{block.v}</pre>
	{:else if block.t === 'hr'}
		<hr class="my-4 border-border" />
	{:else if block.t === 'table'}
		<table class="my-2 border-collapse text-sm">
			<thead
				><tr
					>{#each block.head as cell, c (c)}<th
							class="border border-border bg-muted px-2 py-1 text-left"
							><MarkdownInline items={cell} /></th
						>{/each}</tr
				></thead
			>
			<tbody>
				{#each block.rows as row, r (r)}
					<tr
						>{#each row as cell, c (c)}<td class="border border-border px-2 py-1"
								><MarkdownInline items={cell} /></td
							>{/each}</tr
					>
				{/each}
			</tbody>
		</table>
	{/if}
{/each}
