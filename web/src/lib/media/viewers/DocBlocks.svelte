<script lang="ts">
	import type { DocBlock } from '../officePreview';
	import DocBlocks from './DocBlocks.svelte';

	/** Draws the blocks of a Word document as components; the file never supplies markup. */
	let { blocks }: { blocks: DocBlock[] } = $props();
</script>

{#each blocks as block, i (i)}
	{#if block.type === 'table'}
		<table class="my-2 w-full border-collapse text-sm">
			<tbody>
				{#each block.rows as row, r (r)}
					<tr>
						{#each row as cell, c (c)}
							<td class="border border-border px-2 py-1 align-top"><DocBlocks blocks={cell} /></td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
	{:else}
		{@const content = block.runs}
		<p
			class="break-words {block.level === 1 ? 'mt-3 text-xl font-semibold' : ''} {block.level === 2
				? 'mt-3 text-lg font-semibold'
				: ''} {block.level >= 3 ? 'mt-2 font-semibold' : ''} {block.list
				? 'ml-5 list-item list-disc'
				: ''}"
		>
			{#each content as run, k (k)}<span
					class="{run.bold ? 'font-bold' : ''} {run.italic ? 'italic' : ''} {run.underline
						? 'underline'
						: ''}"
					style="white-space:pre-wrap">{run.text}</span
				>{:else}&nbsp;{/each}
		</p>
	{/if}
{/each}
