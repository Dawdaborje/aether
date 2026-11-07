<script lang="ts" generics="TData">
	import type { Table } from '@tanstack/table-core';
	import { Button } from '$lib/components/ui/button';

	let { table, sizes = [10, 20, 50, 100] }: { table: Table<TData>; sizes?: number[] } = $props();

	const total = $derived(table.getPrePaginationRowModel().rows.length);
	const pagination = $derived(table.getState().pagination);
	const first = $derived(total === 0 ? 0 : pagination.pageIndex * pagination.pageSize + 1);
	const last = $derived(Math.min(first + pagination.pageSize - 1, total));
</script>

{#if total > Math.min(...sizes)}
	<nav class="flex flex-wrap items-center justify-between gap-3" aria-label="Pagination">
		<p class="text-xs text-muted-foreground">{first}–{last} of {total}</p>
		<div class="flex flex-wrap items-center gap-3">
			<label class="flex items-center gap-2 text-xs text-muted-foreground">
				Rows per page
				<select
					class="h-8 rounded-sm border border-input bg-card px-2 text-sm text-foreground"
					value={pagination.pageSize}
					onchange={(event) => table.setPageSize(Number(event.currentTarget.value))}
				>
					{#each sizes as size (size)}
						<option value={size}>{size}</option>
					{/each}
				</select>
			</label>
			<span class="text-xs text-muted-foreground">
				Page {pagination.pageIndex + 1} of {Math.max(table.getPageCount(), 1)}
			</span>
			<div class="flex items-center gap-2">
				<Button
					variant="outline"
					size="sm"
					disabled={!table.getCanPreviousPage()}
					onclick={() => table.previousPage()}>Previous</Button
				>
				<Button
					variant="outline"
					size="sm"
					disabled={!table.getCanNextPage()}
					onclick={() => table.nextPage()}>Next</Button
				>
			</div>
		</div>
	</nav>
{/if}
