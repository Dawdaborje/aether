<script lang="ts">
	import type { PivotCard } from './types';
	import { formatValue } from './format';

	let {
		card,
		onsegment
	}: { card: PivotCard; onsegment?: (segment: Record<string, unknown>) => void } = $props();

	const peak = $derived(Math.max(0, ...card.matrix.flat()));
	const hasColumns = $derived(Boolean(card.dimensions[1]));

	function shade(value: number): string {
		if (!peak || value <= 0) return '';
		const pct = Math.round(8 + (value / peak) * 42);
		return `background: color-mix(in oklab, var(--chart-1) ${pct}%, transparent)`;
	}
	function pick(r: number, c: number | null) {
		const out: Record<string, unknown> = {};
		const [rowField, colField] = card.dimensions;
		if (rowField) out[rowField] = card.row_keys[r] ?? null;
		if (colField && c !== null) out[colField] = card.column_keys[c] ?? null;
		onsegment?.(out);
	}
	const fmt = (v: number | null | undefined) => formatValue(v, card.format, card.currency);
</script>

<div class="max-h-96 overflow-auto rounded-md border border-border">
	<table class="w-full text-sm">
		<thead class="sticky top-0 bg-muted/70 text-xs text-muted-foreground backdrop-blur">
			<tr>
				<th class="px-3 py-2 text-left font-medium capitalize"
					>{(card.dimensions[0] ?? '').replaceAll('_', ' ')}</th
				>
				{#each card.columns as name (name)}
					<th class="px-3 py-2 text-right font-medium">{name}</th>
				{/each}
				{#if hasColumns}
					<th class="px-3 py-2 text-right font-medium">Total</th>
				{/if}
			</tr>
		</thead>
		<tbody>
			{#each card.rows as name, r (r)}
				<tr class="border-t border-border">
					<th class="px-3 py-1.5 text-left font-normal">
						<button type="button" class="hover:underline" onclick={() => pick(r, null)}
							>{name}</button
						>
					</th>
					{#each card.matrix[r] as value, c (c)}
						<td class="p-0 text-right tabular-nums">
							<button
								type="button"
								class="block w-full px-3 py-1.5 text-right hover:ring-1 hover:ring-ring"
								style={shade(value)}
								onclick={() => pick(r, hasColumns ? c : null)}
							>
								{fmt(value)}
							</button>
						</td>
					{/each}
					{#if hasColumns}
						<td class="px-3 py-1.5 text-right font-medium tabular-nums"
							>{fmt(card.row_totals[r])}</td
						>
					{/if}
				</tr>
			{:else}
				<tr>
					<td class="px-3 py-6 text-center text-muted-foreground" colspan={card.columns.length + 2}>
						No data for this selection
					</td>
				</tr>
			{/each}
		</tbody>
		{#if hasColumns && card.rows.length > 0}
			<tfoot class="border-t border-border bg-muted/30 font-medium">
				<tr>
					<td class="px-3 py-1.5">Total</td>
					{#each card.column_totals as total, c (c)}
						<td class="px-3 py-1.5 text-right tabular-nums">{fmt(total)}</td>
					{/each}
					<td class="px-3 py-1.5 text-right tabular-nums">{fmt(card.grand)}</td>
				</tr>
			</tfoot>
		{/if}
	</table>
</div>
