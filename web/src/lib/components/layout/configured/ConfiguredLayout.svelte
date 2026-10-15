<script lang="ts">
	import type { LayoutDef } from '$lib/theme/types';
	import BareLayout from '../bare/bareLayout.svelte';
	import { defaultLayout, deskLayout } from '../composed/builtins';
	import ComposedLayout from '../composed/ComposedLayout.svelte';
	import CustomLayout from '../custom/customLayout.svelte';

	let { children, def }: { children: import('svelte').Snippet; def: LayoutDef } = $props();

	// A plugin layout either composes its own regions or picks a first-party chrome.
	const composed = $derived.by((): LayoutDef => {
		if (def.chrome === 'composed' && def.regions) return def;
		return def.chrome === 'sidebar' ? deskLayout : defaultLayout;
	});
</script>

{#if def.chrome === 'bare'}
	<BareLayout>
		{@render children()}
	</BareLayout>
{:else if def.chrome === 'custom'}
	<CustomLayout>
		{@render children()}
	</CustomLayout>
{:else}
	<ComposedLayout def={composed}>
		{@render children()}
	</ComposedLayout>
{/if}
