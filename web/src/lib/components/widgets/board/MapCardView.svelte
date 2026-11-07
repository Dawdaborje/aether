<script lang="ts">
	import type { SeriesCard } from './types';
	import { findCountry, LAND, project } from './geo';
	import { formatValue } from './format';

	let {
		card,
		onsegment
	}: {
		card: SeriesCard;
		onsegment?: (segment: Record<string, unknown>, summary?: string) => void;
	} = $props();

	const W = 720;
	const H = Math.round((W * 142) / 360);

	/** Every category that is a country, with where it sits and what it holds. */
	const placed = $derived.by(() => {
		const values = card.series[0]?.values ?? [];
		const out: { c: number; name: string; value: number; x: number; y: number }[] = [];
		card.categories.forEach((name, c) => {
			const country = findCountry(name) ?? findCountry(card.keys[c]);
			if (!country) return;
			const at = project(country.lat, country.lng);
			out.push({ c, name: country.name, value: values[c] ?? 0, x: at.x * W, y: at.y * H });
		});
		return out;
	});
	const lost = $derived(
		card.categories.filter((name, c) => !placed.some((p) => p.c === c) && name !== '(none)')
	);
	const peak = $derived(Math.max(1, ...placed.map((p) => Math.abs(p.value))));
	const radius = (value: number) => 4 + Math.sqrt(Math.abs(value) / peak) * 22;

	const land = LAND.map(
		(ring) =>
			'M ' +
			ring
				.map(([lng, lat]) => {
					const at = project(lat, lng);
					return `${(at.x * W).toFixed(1)} ${(at.y * H).toFixed(1)}`;
				})
				.join(' L ') +
			' Z'
	).join(' ');
	const meridians = [-120, -60, 0, 60, 120].map((lng) => project(0, lng).x * W);
	const parallels = [60, 30, 0, -30].map((lat) => project(lat, 0).y * H);

	function pick(c: number, name: string) {
		const field = card.dimensions[0];
		onsegment?.(field ? { [field]: card.keys[c] ?? null } : {}, name);
	}
</script>

{#if card.categories.length === 0}
	<div
		class="flex h-40 items-center justify-center rounded-md bg-muted/40 text-sm text-muted-foreground"
	>
		No data for this selection
	</div>
{:else}
	<svg viewBox="0 0 {W} {H}" class="w-full" role="img" aria-label={card.title}>
		{#each meridians as x (x)}
			<line x1={x} x2={x} y1="0" y2={H} class="stroke-border" stroke-dasharray="2 4" />
		{/each}
		{#each parallels as y (y)}
			<line x1="0" x2={W} y1={y} y2={y} class="stroke-border" stroke-dasharray="2 4" />
		{/each}
		<path d={land} class="fill-muted stroke-border" stroke-width="0.6" />
		{#each placed as p (p.c)}
			<circle
				cx={p.x}
				cy={p.y}
				r={radius(p.value)}
				fill="var(--chart-1)"
				fill-opacity="0.55"
				stroke="var(--chart-1)"
				class="cursor-pointer transition-opacity hover:opacity-80"
				role="button"
				tabindex="0"
				aria-label="{p.name}: {formatValue(p.value, card.format, card.currency)}"
				onclick={() => pick(p.c, p.name)}
				onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && pick(p.c, p.name)}
			>
				<title>{p.name}: {formatValue(p.value, card.format, card.currency)}</title>
			</circle>
		{/each}
	</svg>
	{#if lost.length > 0}
		<p class="mt-1 text-xs text-muted-foreground">
			Not shown on the map, because they are not countries: {lost
				.slice(0, 5)
				.join(', ')}{lost.length > 5 ? ` and ${lost.length - 5} more` : ''}.
		</p>
	{/if}
	{#if card.truncated}
		<p class="mt-1 text-xs text-muted-foreground">
			Showing the first {card.categories.length} places.
		</p>
	{/if}
{/if}
