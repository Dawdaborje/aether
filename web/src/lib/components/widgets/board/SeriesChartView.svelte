<script lang="ts">
	import type { SeriesCard } from './types';
	import { arcPath, clip, formatValue, niceScale, seriesColor } from './format';

	let {
		card,
		onsegment
	}: {
		card: SeriesCard;
		/** A bar, slice or point was clicked: dimension field → raw value. */
		onsegment?: (segment: Record<string, unknown>, summary?: string) => void;
	} = $props();

	const HEIGHT = 240;
	const PAD = { top: 12, right: 12, bottom: 44, left: 52 };

	let width = $state(480);
	let hover = $state<{ c: number; s: number } | null>(null);

	const stacked = $derived(card.kind === 'stacked_bar' || card.kind === 'area');
	const horizontal = $derived(card.kind === 'row');
	const round = $derived(card.kind === 'pie' || card.kind === 'donut');
	const multi = $derived(card.series.length > 1);

	const totals = $derived(
		card.categories.map((_, c) => card.series.reduce((sum, s) => sum + (s.values[c] ?? 0), 0))
	);
	const scale = $derived.by(() => {
		const all = card.series.flatMap((s) => s.values);
		if (stacked) {
			return niceScale(Math.min(0, ...all), Math.max(0, ...totals));
		}
		return niceScale(Math.min(0, ...all), Math.max(0, ...all));
	});

	const innerW = $derived(Math.max(10, width - PAD.left - PAD.right));
	const innerH = HEIGHT - PAD.top - PAD.bottom;
	const count = $derived(Math.max(1, card.categories.length));

	function y(value: number): number {
		return PAD.top + innerH - ((value - scale.min) / (scale.max - scale.min || 1)) * innerH;
	}
	function xCenter(c: number): number {
		return PAD.left + (innerW / count) * (c + 0.5);
	}

	// Row charts lay categories down the side.
	const rowH = $derived(Math.max(18, Math.min(34, 220 / count)));
	const rowsHeight = $derived(Math.max(HEIGHT, count * rowH + 28));
	const rowLeft = $derived(Math.min(140, Math.max(70, width * 0.28)));
	function xRow(value: number): number {
		return rowLeft + ((value - scale.min) / (scale.max - scale.min || 1)) * (width - rowLeft - 16);
	}

	function segment(c: number, s: number | null): Record<string, unknown> {
		const out: Record<string, unknown> = {};
		const [first, second] = card.dimensions;
		if (first) out[first] = card.keys[c] ?? null;
		if (second && s !== null) out[second] = card.series[s]?.key ?? null;
		return out;
	}
	function pick(c: number, s: number | null) {
		const series = s === null ? null : card.series[s];
		onsegment?.(
			segment(c, s),
			series && multi ? `${card.categories[c]} · ${series.name}` : card.categories[c]
		);
	}
	function label(c: number, s: number): string {
		const series = card.series[s];
		const prefix = multi ? `${series.name} · ` : '';
		return `${card.categories[c]}: ${prefix}${formatValue(series.values[c], card.format, card.currency)}`;
	}

	// Stacked offsets: the value each series starts from in each category.
	const bases = $derived(
		card.series.map((_, s) =>
			card.categories.map((_, c) =>
				stacked
					? card.series
							.slice(0, s)
							.reduce((sum, other) => sum + Math.max(0, other.values[c] ?? 0), 0)
					: 0
			)
		)
	);

	function linePoints(s: number): string {
		return card.series[s].values
			.map(
				(v, c) => `${xCenter(c).toFixed(1)},${y((bases[s][c] ?? 0) + Math.max(0, v)).toFixed(1)}`
			)
			.join(' ');
	}
	function areaPath(s: number): string {
		const top = card.series[s].values.map(
			(v, c) => `${xCenter(c).toFixed(1)},${y((bases[s][c] ?? 0) + Math.max(0, v)).toFixed(1)}`
		);
		const bottom = card.series[s].values
			.map((_, c) => `${xCenter(c).toFixed(1)},${y(bases[s][c] ?? 0).toFixed(1)}`)
			.reverse();
		return `M ${top.join(' L ')} L ${bottom.join(' L ')} Z`;
	}

	// Pie and donut: one slice per category of the first series.
	const slices = $derived.by(() => {
		const values = (card.series[0]?.values ?? []).map((v) => Math.max(0, v));
		const sum = values.reduce((a, b) => a + b, 0) || 1;
		let angle = 0;
		return values.map((v, c) => {
			const start = angle;
			angle += (v / sum) * Math.PI * 2;
			return { c, start, end: angle, share: v / sum, value: v };
		});
	});
	const pieTotal = $derived((card.series[0]?.values ?? []).reduce((a, b) => a + b, 0));

	// Show about as many x labels as fit.
	const every = $derived(Math.max(1, Math.ceil(count / Math.max(1, Math.floor(innerW / 64)))));
</script>

<div bind:clientWidth={width} class="w-full">
	{#if card.categories.length === 0}
		<div
			class="flex h-40 items-center justify-center rounded-md bg-muted/40 text-sm text-muted-foreground"
		>
			No data for this selection
		</div>
	{:else if round}
		{@const radius = 84}
		<div class="flex flex-col items-center gap-4 sm:flex-row sm:justify-center">
			<svg viewBox="0 0 200 200" class="size-48 shrink-0" role="img" aria-label={card.title}>
				{#each slices as slice (slice.c)}
					<path
						d={arcPath(
							100,
							100,
							radius,
							card.kind === 'donut' ? radius * 0.58 : 0,
							slice.start,
							slice.end
						)}
						fill={seriesColor(slice.c)}
						stroke="var(--card)"
						stroke-width="1.5"
						fill-rule="evenodd"
						class="cursor-pointer transition-opacity {hover && hover.c !== slice.c
							? 'opacity-50'
							: ''}"
						role="button"
						tabindex="0"
						aria-label="{card.categories[slice.c]}: {formatValue(
							slice.value,
							card.format,
							card.currency
						)}"
						onmouseenter={() => (hover = { c: slice.c, s: 0 })}
						onmouseleave={() => (hover = null)}
						onclick={() => pick(slice.c, null)}
						onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && pick(slice.c, null)}
					>
						<title>
							{card.categories[slice.c]}: {formatValue(slice.value, card.format, card.currency)} ({Math.round(
								slice.share * 1000
							) / 10}%)
						</title>
					</path>
				{/each}
				{#if card.kind === 'donut'}
					<text
						x="100"
						y="98"
						text-anchor="middle"
						class="fill-foreground text-[15px] font-semibold"
					>
						{formatValue(
							pieTotal,
							card.format === 'number' ? 'compact' : card.format,
							card.currency
						)}
					</text>
					<text x="100" y="114" text-anchor="middle" class="fill-muted-foreground text-[9px]">
						{hover ? clip(card.categories[hover.c], 18) : 'Total'}
					</text>
				{/if}
			</svg>
			<ul class="max-h-48 min-w-0 space-y-1 overflow-auto text-xs">
				{#each slices as slice (slice.c)}
					<li class="flex items-center gap-2">
						<span class="size-2.5 shrink-0 rounded-sm" style="background: {seriesColor(slice.c)}"
						></span>
						<button
							type="button"
							class="truncate text-left text-muted-foreground hover:text-foreground"
							onclick={() => pick(slice.c, null)}
						>
							{card.categories[slice.c]}
						</button>
						<span class="ml-auto pl-3 font-medium tabular-nums">
							{formatValue(slice.value, card.format, card.currency)}
						</span>
					</li>
				{/each}
			</ul>
		</div>
	{:else if horizontal}
		<svg
			viewBox="0 0 {width} {rowsHeight}"
			class="w-full"
			style="height: {rowsHeight}px"
			role="img"
			aria-label={card.title}
		>
			{#each scale.ticks as tick (tick)}
				<line
					x1={xRow(tick)}
					x2={xRow(tick)}
					y1="0"
					y2={rowsHeight - 20}
					class="stroke-border"
					stroke-dasharray="2 3"
				/>
				<text
					x={xRow(tick)}
					y={rowsHeight - 6}
					text-anchor="middle"
					class="fill-muted-foreground text-[10px]"
				>
					{formatValue(tick, 'compact')}
				</text>
			{/each}
			{#each card.categories as name, c (c)}
				{@const slot = (rowH - 6) / Math.max(1, card.series.length)}
				<text
					x={rowLeft - 8}
					y={c * rowH + rowH / 2 + 3}
					text-anchor="end"
					class="fill-muted-foreground text-[11px]"
				>
					{clip(name, 22)}
				</text>
				{#each card.series as series, s (s)}
					{@const v = series.values[c] ?? 0}
					<rect
						x={xRow(Math.min(0, v))}
						y={c * rowH + 3 + s * slot}
						width={Math.max(1, Math.abs(xRow(v) - xRow(0)))}
						height={Math.max(2, slot - 1)}
						rx="2"
						fill={seriesColor(s)}
						class="cursor-pointer"
						role="button"
						tabindex="0"
						aria-label={label(c, s)}
						onclick={() => pick(c, multi ? s : null)}
						onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && pick(c, multi ? s : null)}
					>
						<title>{label(c, s)}</title>
					</rect>
				{/each}
			{/each}
		</svg>
	{:else}
		<svg
			viewBox="0 0 {width} {HEIGHT}"
			class="w-full"
			style="height: {HEIGHT}px"
			role="img"
			aria-label={card.title}
		>
			{#each scale.ticks as tick (tick)}
				<line
					x1={PAD.left}
					x2={width - PAD.right}
					y1={y(tick)}
					y2={y(tick)}
					class="stroke-border"
					stroke-dasharray="2 3"
				/>
				<text
					x={PAD.left - 8}
					y={y(tick) + 3}
					text-anchor="end"
					class="fill-muted-foreground text-[10px]"
				>
					{formatValue(tick, 'compact')}
				</text>
			{/each}
			{#each card.categories as name, c (c)}
				{#if c % every === 0}
					<text
						x={xCenter(c)}
						y={HEIGHT - PAD.bottom + 16}
						text-anchor="middle"
						class="fill-muted-foreground text-[10px]"
					>
						{clip(name, Math.max(6, Math.floor(((innerW / count) * every) / 6)))}
					</text>
				{/if}
			{/each}

			{#if card.kind === 'line' || card.kind === 'area'}
				{#each [...card.series.keys()] as s (s)}
					{#if card.kind === 'area'}
						<path d={areaPath(s)} fill={seriesColor(s)} fill-opacity="0.25" />
					{/if}
					<polyline
						fill="none"
						stroke={seriesColor(s)}
						stroke-width="2"
						stroke-linejoin="round"
						points={linePoints(s)}
					/>
				{/each}
				{#each [...card.categories.keys()] as c (c)}
					<!-- A tall invisible target per category, so a point is easy to hit. -->
					<rect
						x={xCenter(c) - innerW / count / 2}
						y={PAD.top}
						width={innerW / count}
						height={innerH}
						fill="transparent"
						class="cursor-pointer"
						role="button"
						tabindex="0"
						aria-label={card.categories[c]}
						onmouseenter={() => (hover = { c, s: 0 })}
						onmouseleave={() => (hover = null)}
						onclick={() => pick(c, null)}
						onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && pick(c, null)}
					>
						<title>{card.series.map((_s, s) => label(c, s)).join('\n')}</title>
					</rect>
				{/each}
				{#each card.series as series, s (s)}
					{#each series.values as v, c (c)}
						<circle
							cx={xCenter(c)}
							cy={y((bases[s][c] ?? 0) + Math.max(0, v))}
							r={card.categories.length > 24 ? 0 : 2.5}
							fill={seriesColor(s)}
						/>
					{/each}
				{/each}
				{#if hover}
					{#each card.series as series, s (s)}
						<circle
							cx={xCenter(hover.c)}
							cy={y((bases[s][hover.c] ?? 0) + Math.max(0, series.values[hover.c] ?? 0))}
							r="3.5"
							fill={seriesColor(s)}
							stroke="var(--card)"
							stroke-width="1.5"
						/>
					{/each}
				{/if}
			{:else}
				{@const group = innerW / count}
				{@const barW = stacked
					? Math.min(48, group * 0.7)
					: Math.min(40, (group * 0.78) / Math.max(1, card.series.length))}
				{#each [...card.categories.keys()] as c (c)}
					{#each card.series as series, s (s)}
						{@const v = series.values[c] ?? 0}
						{@const from = stacked ? (bases[s][c] ?? 0) : 0}
						{@const x = stacked
							? xCenter(c) - barW / 2
							: xCenter(c) - (barW * card.series.length) / 2 + s * barW}
						<rect
							{x}
							y={Math.min(y(from), y(from + v))}
							width={Math.max(2, barW - (stacked ? 0 : 2))}
							height={Math.max(1, Math.abs(y(from + v) - y(from)))}
							rx="2"
							fill={seriesColor(s)}
							class="cursor-pointer transition-opacity {hover && hover.c !== c ? 'opacity-60' : ''}"
							role="button"
							tabindex="0"
							aria-label={label(c, s)}
							onmouseenter={() => (hover = { c, s })}
							onmouseleave={() => (hover = null)}
							onclick={() => pick(c, multi ? s : null)}
							onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && pick(c, multi ? s : null)}
						>
							<title>{label(c, s)}</title>
						</rect>
					{/each}
				{/each}
			{/if}
		</svg>
	{/if}

	{#if multi && !round}
		<ul class="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
			{#each card.series as series, s (s)}
				<li class="flex items-center gap-1.5">
					<span class="size-2.5 rounded-sm" style="background: {seriesColor(s)}"></span>
					{series.name}
				</li>
			{/each}
		</ul>
	{/if}
	{#if card.truncated}
		<p class="mt-1 text-xs text-muted-foreground">
			Showing the first {card.categories.length} groups.
		</p>
	{/if}
</div>
