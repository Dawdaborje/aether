<script lang="ts">
	import type { NumberCard } from './types';
	import { formatChange, formatValue } from './format';

	let { card }: { card: NumberCard } = $props();

	const trend = $derived(card.trend ?? []);
	const spark = $derived.by(() => {
		if (trend.length < 2) return '';
		const values = trend.map((p) => p.value);
		const low = Math.min(...values);
		const high = Math.max(...values);
		const span = high - low || 1;
		return trend
			.map((p, i) => {
				const x = (i / (trend.length - 1)) * 100;
				const y = 28 - ((p.value - low) / span) * 24;
				return `${x.toFixed(2)},${y.toFixed(2)}`;
			})
			.join(' ');
	});
	const progress = $derived(card.progress == null ? null : Math.max(0, Math.min(1, card.progress)));

	function direction(change: number | null): 'up' | 'down' | 'flat' {
		if (change === null || change === 0) return 'flat';
		return change > 0 ? 'up' : 'down';
	}
</script>

<div class="flex flex-wrap items-start gap-x-8 gap-y-4">
	{#each card.readings as reading, i (reading.name)}
		{@const dir = direction(reading.change)}
		<div class="min-w-0">
			{#if card.readings.length > 1}
				<p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
					{reading.label}
				</p>
			{/if}
			<p class="text-3xl font-semibold tabular-nums" data-reading={reading.name}>
				{formatValue(reading.value, card.format, card.currency)}
			</p>
			{#if card.compared}
				<p class="mt-1 flex items-center gap-2 text-xs tabular-nums">
					<span
						class="rounded px-1.5 py-0.5 font-medium {dir === 'up'
							? 'bg-chart-2/15 text-foreground'
							: dir === 'down'
								? 'bg-destructive/10 text-destructive'
								: 'bg-muted text-muted-foreground'}"
						title="Compared with the previous period"
					>
						{dir === 'up' ? '▲' : dir === 'down' ? '▼' : '•'}
						{formatChange(reading.change)}
					</span>
					<span class="text-muted-foreground">
						was {formatValue(reading.previous, card.format, card.currency)}
					</span>
				</p>
			{/if}
			{#if i === 0 && progress !== null}
				<div class="mt-3 w-48 max-w-full">
					<div
						class="h-1.5 overflow-hidden rounded-full bg-muted"
						role="progressbar"
						aria-valuenow={Math.round(progress * 100)}
						aria-valuemin="0"
						aria-valuemax="100"
					>
						<div class="h-full rounded-full bg-chart-1" style="width: {progress * 100}%"></div>
					</div>
					<p class="mt-1 text-xs text-muted-foreground">
						{Math.round((card.progress ?? 0) * 100)}% of {formatValue(
							card.target,
							card.format,
							card.currency
						)}
					</p>
				</div>
			{/if}
		</div>
	{/each}
	{#if spark}
		<svg
			viewBox="0 0 100 32"
			preserveAspectRatio="none"
			class="h-10 min-w-24 flex-1 text-chart-1"
			role="img"
			aria-label="Trend"
		>
			<polyline
				fill="none"
				stroke="currentColor"
				stroke-width="1.5"
				vector-effect="non-scaling-stroke"
				points={spark}
			/>
		</svg>
	{/if}
</div>
