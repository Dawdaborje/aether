<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import ScreenSearchBar from '$lib/search/ScreenSearchBar.svelte';
	import { screenSearch } from '$lib/search/screenSearch.svelte';
	import { isMacPlatform, isTyping } from '$lib/search/shortcuts';
	import type { LayoutBlock } from '$lib/theme/types';
	import { textProp } from '../visibility';

	let { block, vertical = false }: { block: LayoutBlock; vertical?: boolean } = $props();

	const STYLES = ['field', 'button', 'icon'] as const;
	const look = $derived(STYLES.find((style) => style === textProp(block, 'style')) ?? 'field');
	/** `k`: Ctrl/⌘+K only (always on); `/`: the slash key too, outside fields. */
	const slash = $derived(textProp(block, 'shortcut') === '/');
	const hint = $derived(slash ? '/' : isMacPlatform() ? '⌘K' : 'Ctrl K');

	function onWindowKeydown(event: KeyboardEvent) {
		if (look === 'field' || !slash || event.key !== '/') return;
		if (event.defaultPrevented || isTyping(event.target)) return;
		event.preventDefault();
		screenSearch.open = true;
	}
</script>

<svelte:window onkeydown={onWindowKeydown} />

<!-- Opens the search palette: screens, apps and the commands plugins add. -->
{#if look === 'field'}
	<ScreenSearchBar {slash} />
{:else if look === 'button'}
	<button
		type="button"
		data-part="search.button"
		onclick={() => (screenSearch.open = true)}
		class="flex h-8 items-center gap-2 rounded-field border border-region-border px-2.5 text-sm text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring {vertical
			? 'w-full'
			: 'min-w-44'}"
	>
		<AppIcon name="search" class="size-3.5 shrink-0" />
		<span class="flex-1 text-left">Search…</span>
		<kbd class="rounded border border-region-border px-1 text-[10px]">{hint}</kbd>
	</button>
{:else}
	<button
		type="button"
		data-part="search.button"
		aria-label="Search ({hint})"
		title="Search ({hint})"
		onclick={() => (screenSearch.open = true)}
		class="flex size-8 items-center justify-center rounded-button text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring"
	>
		<AppIcon name="search" class="size-4" />
	</button>
{/if}
