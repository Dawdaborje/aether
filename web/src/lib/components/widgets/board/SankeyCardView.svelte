<script lang="ts">
	import type { SankeyCard } from './types';
	import { clip, formatValue, seriesColor } from './format';

	let {
		card,
		onsegment
	}: {
		card: SankeyCard;
		onsegment?: (segment: Record<string, unknown>, summary?: string) => void;
	} = $props();

	const W = 720;
	const NODE = 12;
	const GAP = 10;
	const MARGIN = { left: 8, right: 8, top: 6, bottom: 6 };
	const LABEL = 130;

	const levels = $derived(Math.max(...card.nodes.map((n) => n.level), 0) + 1);
	const columns = $derived(
		Array.from({ length: levels }, (_, level) =>
			card.nodes
				.map((node, index) => ({ node, index }))
				.filter((entry) => entry.node.level === level)
		)
	);
	// One scale for every column, so a flow keeps its thickness from end to end.
	const rows = $derived(Math.max(1, ...columns.map((column) => column.length)));
	const H = $derived(Math.max(200, rows * 30 + MARGIN.top + MARGIN.bottom));
	const scale = $derived.by(() => {
		const room = H - MARGIN.top - MARGIN.bottom;
		return Math.min(
			...columns.map((column) => {
				const sum = column.reduce((total, entry) => total + entry.node.value, 0);
				return sum > 0 ? (room - GAP * (column.length - 1)) / sum : Infinity;
			})
		);
	});

	const laid = $derived.by(() => {
		const span = W - MARGIN.left - MARGIN.right - NODE;
		const boxes: Record<number, { x: number; y: number; h: number; level: number }> = {};
		columns.forEach((column, level) => {
			const used =
				column.reduce((total, entry) => total + entry.node.value * scale, 0) +
				GAP * Math.max(0, column.length - 1);
			let y = MARGIN.top + (H - MARGIN.top - MARGIN.bottom - used) / 2;
			for (const entry of column) {
				const h = Math.max(2, entry.node.value * scale);
				boxes[entry.index] = {
					x: MARGIN.left + (levels > 1 ? (span * level) / (levels - 1) : 0),
					y,
					h,
					level
				};
				y += h + GAP;
			}
		});
		// Each link takes the next free stretch on the side of both nodes it joins.
		const out: Record<number, number> = {};
		const into: Record<number, number> = {};
		const ribbons = card.links.map((link, index) => {
			const a = boxes[link.source];
			const b = boxes[link.target];
			const thickness = Math.max(1, link.value * scale);
			const y1 = a.y + (out[link.source] ?? 0) + thickness / 2;
			const y2 = b.y + (into[link.target] ?? 0) + thickness / 2;
			out[link.source] = (out[link.source] ?? 0) + thickness;
			into[link.target] = (into[link.target] ?? 0) + thickness;
			const x1 = a.x + NODE;
			const x2 = b.x;
			const mid = (x1 + x2) / 2;
			return {
				index,
				link,
				thickness,
				path: `M ${x1} ${y1} C ${mid} ${y1} ${mid} ${y2} ${x2} ${y2}`
			};
		});
		return { boxes, ribbons };
	});

	const name = (index: number) => card.nodes[index]?.name ?? '';
	const fmt = (value: number) => formatValue(value, card.format, card.currency);

	function node(index: number) {
		const n = card.nodes[index];
		onsegment?.({ [n.field]: n.key ?? null }, n.name);
	}
	function link(index: number) {
		const l = card.links[index];
		const from = card.nodes[l.source];
		const to = card.nodes[l.target];
		onsegment?.(
			{ [from.field]: from.key ?? null, [to.field]: to.key ?? null },
			`${from.name} → ${to.name}`
		);
	}
</script>

{#if card.links.length === 0}
	<div
		class="flex h-40 items-center justify-center rounded-md bg-muted/40 text-sm text-muted-foreground"
	>
		No flows for this selection
	</div>
{:else}
	<svg viewBox="0 0 {W} {H}" class="w-full" role="img" aria-label={card.title}>
		{#each laid.ribbons as ribbon (ribbon.index)}
			<path
				d={ribbon.path}
				fill="none"
				stroke={seriesColor(ribbon.link.source)}
				stroke-opacity="0.3"
				stroke-width={ribbon.thickness}
				class="cursor-pointer"
				role="button"
				tabindex="0"
				aria-label="{name(ribbon.link.source)} to {name(ribbon.link.target)}: {fmt(
					ribbon.link.value
				)}"
				onclick={() => link(ribbon.index)}
				onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && link(ribbon.index)}
			>
				<title
					>{name(ribbon.link.source)} → {name(ribbon.link.target)}: {fmt(ribbon.link.value)}</title
				>
			</path>
		{/each}
		{#each card.nodes as n, i (i)}
			{@const box = laid.boxes[i]}
			{#if box}
				<rect
					x={box.x}
					y={box.y}
					width={NODE}
					height={box.h}
					rx="2"
					fill={seriesColor(i)}
					class="cursor-pointer hover:opacity-80"
					role="button"
					tabindex="0"
					aria-label="{n.name}: {fmt(n.value)}"
					onclick={() => node(i)}
					onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && node(i)}
				>
					<title>{n.name}: {fmt(n.value)}</title>
				</rect>
				<text
					x={box.level === levels - 1 && levels > 1 ? box.x - 5 : box.x + NODE + 5}
					y={box.y + box.h / 2 + 3}
					text-anchor={box.level === levels - 1 && levels > 1 ? 'end' : 'start'}
					class="pointer-events-none fill-foreground text-[10px]"
					>{clip(n.name, Math.floor(LABEL / 6))} · {formatValue(n.value, 'compact')}</text
				>
			{/if}
		{/each}
	</svg>
	{#if card.truncated}
		<p class="mt-1 text-xs text-muted-foreground">Showing the largest {card.links.length} flows.</p>
	{/if}
{/if}
