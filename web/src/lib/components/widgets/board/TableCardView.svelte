<script lang="ts">
	import type { TableCard } from './types';
	import { formatValue } from './format';

	let {
		card,
		onsegment
	}: { card: TableCard; onsegment?: (segment: Record<string, unknown>) => void } = $props();

	const measures = $derived(card.columns.filter((c) => c.type === 'measure'));
	const drillable = $derived(card.columns.some((c) => c.type === 'dimension'));

	/** The first measure carries the chart's format; the rest are plain numbers. */
	function show(key: string, value: unknown): string {
		if (typeof value !== 'number')
			return value === null || value === undefined ? '—' : String(value);
		return measures[0]?.key === key
			? formatValue(value, card.format, card.currency)
			: formatValue(value, 'number');
	}
</script>

<div class="max-h-96 overflow-auto rounded-md border border-border">
	<table class="w-full text-sm">
		<thead class="sticky top-0 bg-muted/70 text-xs text-muted-foreground backdrop-blur">
			<tr>
				{#each card.columns as column (column.key)}
					<th
						class="px-3 py-2 font-medium {column.type === 'measure' ? 'text-right' : 'text-left'}"
					>
						{column.label}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each card.rows as row, i (i)}
				<tr
					class="border-t border-border {drillable ? 'cursor-pointer hover:bg-muted/40' : ''}"
					onclick={() => drillable && onsegment?.(row.keys)}
				>
					{#each card.columns as column (column.key)}
						<td class="px-3 py-1.5 {column.type === 'measure' ? 'text-right tabular-nums' : ''}">
							{show(column.key, row.cells[column.key])}
						</td>
					{/each}
				</tr>
			{:else}
				<tr>
					<td class="px-3 py-6 text-center text-muted-foreground" colspan={card.columns.length}>
						No data for this selection
					</td>
				</tr>
			{/each}
		</tbody>
		{#if card.totals && card.rows.length > 1}
			<tfoot class="border-t border-border bg-muted/30 font-medium">
				<tr>
					{#each card.columns as column, i (column.key)}
						<td class="px-3 py-1.5 {column.type === 'measure' ? 'text-right tabular-nums' : ''}">
							{#if column.type === 'measure'}
								{show(column.key, card.totals[column.key])}
							{:else if i === 0}
								Total
							{/if}
						</td>
					{/each}
				</tr>
			</tfoot>
		{/if}
	</table>
</div>
{#if card.truncated}
	<p class="mt-1 text-xs text-muted-foreground">Showing the first {card.rows.length} rows.</p>
{/if}
