<script lang="ts">
	import type { BubbleCard } from './types';
	import { clip, formatValue, niceScale, seriesColor } from './format';

	let {
		card,
		onsegment
	}: {
		card: BubbleCard;
		onsegment?: (segment: Record<string, unknown>, summary?: string) => void;
	} = $props();

	const W = 560;
	const H = 280;
	const PAD = { top: 14, right: 22, bottom: 42, left: 56 };

	const xs = $derived(
		niceScale(
			Math.min(0, ...card.points.map((p) => p.x)),
			Math.max(0, ...card.points.map((p) => p.x))
		)
	);
	const ys = $derived(
		niceScale(
			Math.min(0, ...card.points.map((p) => p.y)),
			Math.max(0, ...card.points.map((p) => p.y))
		)
	);
	const biggest = $derived(Math.max(1, ...card.points.map((p) => Math.abs(p.size ?? 0))));

	const px = (value: number) =>
		PAD.left + ((value - xs.min) / (xs.max - xs.min || 1)) * (W - PAD.left - PAD.right);
	const py = (value: number) =>
		H - PAD.bottom - ((value - ys.min) / (ys.max - ys.min || 1)) * (H - PAD.top - PAD.bottom);
	const radius = (size: number | null) =>
		size === null ? 7 : 5 + Math.sqrt(Math.abs(size) / biggest) * 20;

	function pick(index: number) {
		const field = card.dimensions[0];
		const point = card.points[index];
		onsegment?.(field ? { [field]: point.key ?? null } : {}, point.label);
	}
	const fmt = (value: number) => formatValue(value, 'compact');
</script>

{#if card.points.length === 0}
	<div
		class="flex h-40 items-center justify-center rounded-md bg-muted/40 text-sm text-muted-foreground"
	>
		No data for this selection
	</div>
{:else}
	<svg viewBox="0 0 {W} {H}" class="w-full" role="img" aria-label={card.title}>
		{#each ys.ticks as tick (tick)}
			<line x1={PAD.left} x2={W - PAD.right} y1={py(tick)} y2={py(tick)} class="stroke-border" />
			<text
				x={PAD.left - 6}
				y={py(tick) + 3}
				text-anchor="end"
				class="fill-muted-foreground text-[10px]">{fmt(tick)}</text
			>
		{/each}
		{#each xs.ticks as tick (tick)}
			<text
				x={px(tick)}
				y={H - PAD.bottom + 14}
				text-anchor="middle"
				class="fill-muted-foreground text-[10px]">{fmt(tick)}</text
			>
		{/each}
		<text
			x={(PAD.left + W - PAD.right) / 2}
			y={H - 6}
			text-anchor="middle"
			class="fill-muted-foreground text-[11px]">{card.axes.x}</text
		>
		<text
			transform="translate(12 {(PAD.top + H - PAD.bottom) / 2}) rotate(-90)"
			text-anchor="middle"
			class="fill-muted-foreground text-[11px]">{card.axes.y}</text
		>
		{#each card.points as point, i (i)}
			<circle
				cx={px(point.x)}
				cy={py(point.y)}
				r={radius(point.size)}
				fill={seriesColor(i)}
				fill-opacity="0.6"
				stroke={seriesColor(i)}
				class="cursor-pointer transition-opacity hover:opacity-80"
				role="button"
				tabindex="0"
				aria-label={point.label}
				onclick={() => pick(i)}
				onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && pick(i)}
			>
				<title
					>{point.label}: {card.axes.x}
					{formatValue(point.x, card.format, card.currency)}, {card.axes.y}
					{formatValue(point.y, card.format, card.currency)}{point.size === null
						? ''
						: `, ${card.axes.size} ${formatValue(point.size, card.format, card.currency)}`}</title
				>
			</circle>
			{#if card.points.length <= 12}
				<text
					x={px(point.x)}
					y={py(point.y) - radius(point.size) - 3}
					text-anchor="middle"
					class="pointer-events-none fill-foreground text-[10px]">{clip(point.label, 14)}</text
				>
			{/if}
		{/each}
	</svg>
	{#if card.truncated}
		<p class="mt-1 text-xs text-muted-foreground">Showing the first {card.points.length} groups.</p>
	{/if}
{/if}
