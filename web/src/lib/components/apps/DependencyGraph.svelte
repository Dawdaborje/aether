<script lang="ts">
	import type { CatalogPlugin } from '$lib/apps/catalogApi';
	import {
		buildDependencyGraph,
		NODE_HEIGHT,
		NODE_WIDTH,
		type GraphNode
	} from '$lib/apps/dependencyGraph';

	let { name, catalog }: { name: string; catalog: CatalogPlugin[] } = $props();

	const graph = $derived(buildDependencyGraph(name, catalog));
	const byName = $derived(new Map(graph.nodes.map((n) => [n.name, n])));

	/** A curve from the facing edge of `from` to the facing edge of `to`. */
	function path(from: GraphNode, to: GraphNode): string {
		const leftward = from.x > to.x;
		const x1 = leftward ? from.x : from.x + NODE_WIDTH;
		const x2 = leftward ? to.x + NODE_WIDTH : to.x;
		const y1 = from.y + NODE_HEIGHT / 2;
		const y2 = to.y + NODE_HEIGHT / 2;
		const bend = Math.max(24, Math.abs(x2 - x1) / 2);
		const sign = leftward ? -1 : 1;
		return `M${x1},${y1} C${x1 + sign * bend},${y1} ${x2 - sign * bend},${y2} ${x2},${y2}`;
	}
</script>

{#if graph.nodes.length <= 1}
	<p class="text-muted-foreground">This plugin has no dependencies and nothing depends on it.</p>
{:else}
	<p class="text-xs text-muted-foreground">
		{graph.dependencyCount}
		{graph.dependencyCount === 1 ? 'dependency' : 'dependencies'} (all levels) · {graph.dependentCount}
		{graph.dependentCount === 1 ? 'plugin depends' : 'plugins depend'} on it. Arrows point to the plugin
		that is needed.
	</p>
	<div class="overflow-x-auto rounded-md border border-border bg-muted/20">
		<svg
			width={graph.width}
			height={graph.height}
			viewBox="0 0 {graph.width} {graph.height}"
			role="img"
			aria-label="Dependency graph of {name}"
			class="mx-auto block max-w-none"
		>
			<defs>
				<marker
					id="dep-arrow"
					viewBox="0 0 10 10"
					refX="9"
					refY="5"
					markerWidth="7"
					markerHeight="7"
					orient="auto-start-reverse"
				>
					<path d="M0,0 L10,5 L0,10 z" class="fill-muted-foreground" />
				</marker>
			</defs>
			{#each graph.edges as edge (edge.from + '>' + edge.to)}
				{@const from = byName.get(edge.from)}
				{@const to = byName.get(edge.to)}
				{#if from && to}
					<path
						d={path(from, to)}
						fill="none"
						class="stroke-muted-foreground/60"
						stroke-width="1.25"
						marker-end="url(#dep-arrow)"
					/>
				{/if}
			{/each}
			{#each graph.nodes as node (node.name)}
				{@const isRoot = node.level === 0}
				<g transform="translate({node.x},{node.y})">
					<title>{node.name}{node.missing ? ' (not in the catalog)' : ''}</title>
					<rect
						width={NODE_WIDTH}
						height={NODE_HEIGHT}
						rx="6"
						class={isRoot ? 'fill-primary/15 stroke-primary' : 'fill-background stroke-border'}
						stroke-width={isRoot ? 1.5 : 1}
						stroke-dasharray={node.missing ? '4 3' : undefined}
					/>
					<text
						x={NODE_WIDTH / 2}
						y={NODE_HEIGHT / 2}
						text-anchor="middle"
						dominant-baseline="central"
						class="fill-foreground text-[11px] {isRoot ? 'font-semibold' : ''}"
					>
						{node.label.length > 20 ? node.label.slice(0, 19) + '…' : node.label}
					</text>
				</g>
			{/each}
		</svg>
	</div>
{/if}
