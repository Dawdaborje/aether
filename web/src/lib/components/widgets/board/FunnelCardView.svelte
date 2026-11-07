<script lang="ts">
	import type { SeriesCard } from './types';
	import { formatValue, seriesColor } from './format';

	let {
		card,
		onsegment
	}: {
		card: SeriesCard;
		onsegment?: (segment: Record<string, unknown>, summary?: string) => void;
	} = $props();

	/** Stages run from the largest down; a bar's width is its share of the first stage. */
	const stages = $derived.by(() => {
		const values = card.series[0]?.values ?? [];
		const order = card.categories
			.map((name, c) => ({ name, c, value: Math.max(0, values[c] ?? 0) }))
			.sort((a, b) => b.value - a.value);
		const top = order[0]?.value || 1;
		return order.map((stage, i) => ({
			...stage,
			width: Math.max(4, (stage.value / top) * 100),
			// The share kept from the stage above; the first has nothing above it.
			kept: i === 0 || order[i - 1].value === 0 ? null : stage.value / order[i - 1].value
		}));
	});

	function pick(c: number, name: string) {
		const field = card.dimensions[0];
		onsegment?.(field ? { [field]: card.keys[c] ?? null } : {}, name);
	}
</script>

{#if stages.length === 0}
	<div
		class="flex h-40 items-center justify-center rounded-md bg-muted/40 text-sm text-muted-foreground"
	>
		No data for this selection
	</div>
{:else}
	<ol class="space-y-1.5" aria-label={card.title}>
		{#each stages as stage, i (stage.c)}
			<li class="flex items-center gap-3 text-xs">
				<span class="w-28 shrink-0 truncate text-right text-muted-foreground" title={stage.name}>
					{stage.name}
				</span>
				<div class="min-w-0 flex-1">
					<button
						type="button"
						class="mx-auto flex h-7 items-center justify-end rounded-sm px-2 font-medium text-background tabular-nums transition-opacity hover:opacity-80"
						style="width: {stage.width}%; background: {seriesColor(i)}"
						onclick={() => pick(stage.c, stage.name)}
						aria-label="{stage.name}: {formatValue(stage.value, card.format, card.currency)}"
					>
						{formatValue(
							stage.value,
							card.format === 'number' ? 'compact' : card.format,
							card.currency
						)}
					</button>
				</div>
				<span
					class="w-12 shrink-0 text-muted-foreground tabular-nums"
					title="Kept from the stage above"
				>
					{stage.kept === null ? '' : `${Math.round(stage.kept * 100)}%`}
				</span>
			</li>
		{/each}
	</ol>
	{#if card.truncated}
		<p class="mt-1 text-xs text-muted-foreground">Showing the first {stages.length} stages.</p>
	{/if}
{/if}
