<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import { FieldSet, FieldLegend } from '$lib/components/ui/field';

	let { node }: { node: PageNode } = $props();

	/// `cols="1|2|3"` — section columns (default 2).
	const cols = $derived.by(() => {
		const raw = node.cols ?? node.columns;
		const n = typeof raw === 'number' ? raw : Number(raw);
		if (n === 1 || n === 2 || n === 3) return n;
		return 2;
	});

	const gridClass = $derived(
		cols === 1
			? 'grid grid-cols-1 gap-4'
			: cols === 3
				? 'grid grid-cols-1 gap-4 md:grid-cols-3'
				: 'grid grid-cols-1 gap-4 md:grid-cols-2'
	);

	function spanClass(child: PageNode): string {
		const span = child.colspan ?? child.span;
		if (span === 'full' || span === true || String(span) === String(cols)) {
			return 'md:col-span-full';
		}
		const n = typeof span === 'number' ? span : Number(span);
		if (n === 2 && cols >= 2) return 'md:col-span-2';
		if (n === 3 && cols >= 3) return 'md:col-span-3';
		return '';
	}
</script>

<!-- A section of the page's sheet, not a card of its own: just a heading and a grid. -->
<FieldSet class="gap-4" data-part="form.group">
	{#if node.title}
		<FieldLegend class="text-sm font-semibold text-foreground">{node.title}</FieldLegend>
	{/if}
	<div class={gridClass}>
		{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
			<div class={spanClass(child)}>
				<PageRenderer node={child} />
			</div>
		{/each}
	</div>
</FieldSet>
