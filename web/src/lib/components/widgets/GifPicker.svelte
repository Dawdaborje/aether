<script lang="ts">
	import { searchGifs, type Gif } from '$lib/pages/gif';

	/**
	 * The panel of the composer's GIF button: a search box and a grid of the GIFs the registered packs offer.
	 * `onpick` gets the GIF to attach to the message.
	 */
	let { gifs, onpick, onclose }: { gifs: Gif[]; onpick: (gif: Gif) => void; onclose: () => void } =
		$props();

	let query = $state('');
	const shown = $derived(searchGifs(gifs, query));
</script>

<div
	class="absolute bottom-full left-0 z-20 mb-1 w-[26rem] max-w-[calc(100vw-1rem)] rounded-md border border-border bg-background p-2 shadow"
	role="dialog"
	aria-label="GIFs"
	data-part="gif-picker"
	tabindex="-1"
	onkeydown={(e) => e.key === 'Escape' && onclose()}
>
	<!-- svelte-ignore a11y_autofocus -->
	<input
		type="search"
		bind:value={query}
		autofocus
		placeholder="Search GIFs"
		aria-label="Search GIFs"
		class="mb-2 h-8 w-full rounded-md border border-input bg-transparent px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
	/>
	<div class="h-72 overflow-y-auto" role="listbox" aria-label="GIFs">
		{#if shown.length === 0}
			<p class="p-2 text-sm text-muted-foreground">No GIFs found.</p>
		{:else}
			<ul class="grid grid-cols-4 gap-1">
				{#each shown as gif (gif.url)}
					<li role="option" aria-selected="false">
						<button
							type="button"
							class="grid aspect-square w-full place-items-center overflow-hidden rounded bg-muted/40 hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
							title={gif.name}
							aria-label={gif.name}
							onmousedown={(e) => e.preventDefault()}
							onclick={() => onpick(gif)}
						>
							<img src={gif.url} alt="" loading="lazy" class="size-full object-contain" />
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</div>
