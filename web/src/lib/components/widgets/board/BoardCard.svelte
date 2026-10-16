<script lang="ts">
	import type { BoardItem, Card } from './types';
	import NumberCardView from './NumberCardView.svelte';
	import SeriesChartView from './SeriesChartView.svelte';
	import TableCardView from './TableCardView.svelte';
	import PivotCardView from './PivotCardView.svelte';

	let {
		item,
		ondrill
	}: {
		item: BoardItem;
		/** The records behind a bar, slice or cell of this card were asked for. */
		ondrill?: (item: BoardItem, segment: Record<string, unknown>, summary?: string) => void;
	} = $props();

	const card = $derived(item.card as Card | undefined);
	const skipped = $derived(card?.meta?.skipped ?? []);
	const title = $derived(card?.title || item.title || '');

	function drill(segment: Record<string, unknown>, summary?: string) {
		ondrill?.(item, segment, summary);
	}
</script>

<section
	class="h-full rounded-lg border border-border bg-card p-4 text-card-foreground"
	data-part="dashboard.chart"
	data-kind={card?.kind ?? 'error'}
>
	<header class="mb-3 flex items-start justify-between gap-2">
		<div class="min-w-0">
			<h3 class="truncate text-sm font-medium">{title}</h3>
			{#if card?.description}
				<p class="truncate text-xs text-muted-foreground">{card.description}</p>
			{/if}
		</div>
		<div class="flex shrink-0 items-center gap-1.5">
			{#if skipped.length > 0}
				<span
					class="cursor-help rounded border border-border px-1.5 text-[10px] text-muted-foreground"
					title={skipped.map((s) => `${s.field}: ${s.reason}`).join('\n')}
				>
					{skipped.length} filter{skipped.length === 1 ? '' : 's'} not applied
				</span>
			{/if}
			{#if card?.info}
				<span
					class="flex size-4 cursor-help items-center justify-center rounded-full border border-border text-[10px] text-muted-foreground"
					title={card.info}
					role="img"
					aria-label={card.info}>i</span
				>
			{/if}
		</div>
	</header>

	{#if item.error || !card}
		<p class="text-sm text-destructive" role="alert">
			{item.error ?? 'This chart could not be shown.'}
		</p>
	{:else if card.kind === 'number'}
		<NumberCardView {card} />
	{:else if card.kind === 'table'}
		<TableCardView {card} onsegment={drill} />
	{:else if card.kind === 'pivot'}
		<PivotCardView {card} onsegment={drill} />
	{:else}
		<SeriesChartView {card} onsegment={drill} />
	{/if}
</section>
