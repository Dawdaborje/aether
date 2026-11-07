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

	/// A group of only buttons is a toolbar, not a form section: columns apply at
	/// the pane's width (a rail is narrow on a wide screen) and each button fills its cell.
	const buttonsOnly = $derived(
		(node.children ?? []).length > 0 &&
			(node.children ?? []).every((child) => child.type === 'button')
	);

	const gridClass = $derived.by(() => {
		if (buttonsOnly) {
			if (cols === 1) return 'grid grid-cols-1 gap-2';
			if (cols === 3) return 'grid grid-cols-3 gap-2';
			return 'grid grid-cols-2 gap-2';
		}
		return cols === 1
			? 'grid grid-cols-1 gap-4'
			: cols === 3
				? 'grid grid-cols-1 gap-4 md:grid-cols-3'
				: 'grid grid-cols-1 gap-4 md:grid-cols-2';
	});

	function spanClass(child: PageNode): string {
		const span = child.colspan ?? child.span;
		const full = span === 'full' || span === true || String(span) === String(cols);
		const button = child.type === 'button' ? ' [&_[data-slot=button]]:w-full' : '';
		if (buttonsOnly) {
			return (full ? 'col-span-full' : '') + button;
		}
		if (full) return 'md:col-span-full' + button;
		const n = typeof span === 'number' ? span : Number(span);
		if (n === 2 && cols >= 2) return 'md:col-span-2' + button;
		if (n === 3 && cols >= 3) return 'md:col-span-3' + button;
		return button;
	}
</script>

<!-- A section of the page's sheet, not a card of its own: just a heading and a grid. -->
<FieldSet class={buttonsOnly ? 'gap-2 p-2' : 'gap-4'} data-part="form.group">
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
