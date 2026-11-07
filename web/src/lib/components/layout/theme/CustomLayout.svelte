<script lang="ts">
	import type { Snippet } from 'svelte';
	import ScreenSearchPalette from '$lib/search/ScreenSearchPalette.svelte';
	import type { LayoutDef } from '$lib/theme/types';
	import BareLayout from '../bare/bareLayout.svelte';
	import ComposedLayout from '../composed/ComposedLayout.svelte';

	let { def, children }: { def: LayoutDef; children: Snippet } = $props();
</script>

<!--
	The layout a theme plugin ships (`layouts/<name>.xml`, compiled by the server to the `def`
	this takes). It draws exactly what the definition says, regions and blocks and the links and
	groups written inside them, and nothing else: there is no fallback to the built-in layout,
	so a theme with no sidebar region has no sidebar. The built-in layouts are separate
	components (see `registry.ts`); `AppShell` picks this one when the theme reply carries a layout.
-->
{#if def.chrome === 'bare'}
	<BareLayout>
		{@render children()}
	</BareLayout>
{:else if def.regions}
	<ComposedLayout {def}>
		{@render children()}
	</ComposedLayout>
{:else}
	<!-- A layout with no regions: the page alone. -->
	<main class="flex min-h-screen w-full flex-col" id="content" tabindex="-1">
		{@render children()}
	</main>
	<ScreenSearchPalette />
{/if}
