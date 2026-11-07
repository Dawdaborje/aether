<script lang="ts">
	/** A plain table of cell text, with a header row. Used by the CSV and spreadsheet viewers. */
	let { rows }: { rows: string[][] } = $props();
	const width = $derived(rows.reduce((n, r) => Math.max(n, r.length), 0));
</script>

<table class="w-full border-collapse text-xs">
	<tbody>
		{#each rows as row, r (r)}
			<tr class={r === 0 ? 'bg-muted font-medium' : 'border-t border-border'}>
				<th class="w-8 bg-muted/60 px-1 text-right font-normal text-muted-foreground">{r + 1}</th>
				{#each { length: width } as _, c (c)}
					<td class="px-2 py-1 whitespace-nowrap">{row[c] ?? ''}</td>
				{/each}
			</tr>
		{/each}
	</tbody>
</table>
