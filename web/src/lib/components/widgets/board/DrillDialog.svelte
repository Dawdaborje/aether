<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import type { DrillData } from './types';

	let {
		open = $bindable(false),
		title,
		summary,
		loading,
		error,
		data,
		onpage
	}: {
		open: boolean;
		title: string;
		/** What was clicked, in words: "Stage: Won · Month: 2026-03". */
		summary: string;
		loading: boolean;
		error: string | null;
		data: DrillData | null;
		onpage: (offset: number) => void;
	} = $props();

	const from = $derived(data && data.total > 0 ? data.offset + 1 : 0);
	const to = $derived(data ? Math.min(data.total, data.offset + data.rows.length) : 0);

	function show(value: unknown): string {
		if (value === null || value === undefined || value === '') return '—';
		if (typeof value === 'boolean') return value ? 'Yes' : 'No';
		if (typeof value === 'object') return JSON.stringify(value);
		return String(value);
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-w-4xl">
		<Dialog.Header>
			<Dialog.Title>{title}</Dialog.Title>
			<Dialog.Description>{summary}</Dialog.Description>
		</Dialog.Header>

		{#if error}
			<p class="text-sm text-destructive" role="alert">{error}</p>
		{:else if loading && !data}
			<Skeleton class="h-48 w-full" />
		{:else if data}
			<div class="max-h-[55vh] overflow-auto rounded-md border border-border">
				<table class="w-full text-sm">
					<thead class="sticky top-0 bg-muted/70 text-xs text-muted-foreground backdrop-blur">
						<tr>
							{#each data.columns as column (column.key)}
								<th class="px-3 py-2 text-left font-medium whitespace-nowrap">{column.label}</th>
							{/each}
						</tr>
					</thead>
					<tbody class={loading ? 'opacity-50' : ''}>
						{#each data.rows as row, i (row.id ?? i)}
							<tr class="border-t border-border">
								{#each data.columns as column (column.key)}
									<td class="px-3 py-1.5 whitespace-nowrap">{show(row.cells[column.key])}</td>
								{/each}
							</tr>
						{:else}
							<tr>
								<td
									class="px-3 py-6 text-center text-muted-foreground"
									colspan={data.columns.length}
								>
									No records
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
			<div class="flex items-center justify-between gap-2 text-sm text-muted-foreground">
				<span>{from}–{to} of {data.total}</span>
				<div class="flex gap-2">
					<Button
						size="sm"
						variant="outline"
						disabled={loading || data.offset === 0}
						onclick={() => onpage(Math.max(0, data!.offset - data!.limit))}
					>
						Previous
					</Button>
					<Button
						size="sm"
						variant="outline"
						disabled={loading || to >= data.total}
						onclick={() => onpage(data!.offset + data!.limit)}
					>
						Next
					</Button>
				</div>
			</div>
		{/if}
	</Dialog.Content>
</Dialog.Root>
