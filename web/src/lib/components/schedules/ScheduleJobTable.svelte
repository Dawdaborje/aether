<script lang="ts">
	import type { ColumnDef } from '@tanstack/table-core';
	import { getCoreRowModel } from '@tanstack/table-core';
	import { createSvelteTable, DataTable, renderComponent } from '$lib/components/ui/data-table';
	import { Button } from '$lib/components/ui/button';
	import DataTableBadgeCell from '$lib/components/ui/data-table/data-table-badge-cell.svelte';
	import { formatDuration, type ScheduleJob } from '$lib/schedules/api';

	let {
		rows,
		emptyLabel,
		total,
		offset,
		pageSize,
		loading = false,
		onOpen,
		onPage
	}: {
		rows: ScheduleJob[];
		emptyLabel: string;
		total: number;
		offset: number;
		pageSize: number;
		loading?: boolean;
		onOpen: (job: ScheduleJob) => void;
		onPage: (offset: number) => void;
	} = $props();

	function formatWhen(iso: string | null | undefined): string {
		if (!iso) return '—';
		const date = new Date(iso);
		return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
	}

	function variant(state: string): 'default' | 'secondary' | 'outline' | 'destructive' {
		if (state === 'succeeded') return 'default';
		if (state === 'failed') return 'destructive';
		if (state === 'running') return 'secondary';
		return 'outline';
	}

	const columns: ColumnDef<ScheduleJob>[] = [
		{
			id: 'job',
			header: 'Job',
			cell: ({ row }) => {
				const job = row.original;
				if (job.plugin && job.function) return `${job.plugin}.${job.function}`;
				return job.kind;
			}
		},
		{
			id: 'origin',
			header: 'Origin',
			cell: ({ row }) => (row.original.task ? `Schedule: ${row.original.task}` : 'One-off')
		},
		{ id: 'queue', header: 'Queue', cell: ({ row }) => row.original.queue },
		{ id: 'created', header: 'Created', cell: ({ row }) => formatWhen(row.original.date_created) },
		{
			id: 'duration',
			header: 'Duration',
			cell: ({ row }) => formatDuration(row.original.duration_ms)
		},
		{
			id: 'state',
			header: 'State',
			cell: ({ row }) =>
				renderComponent(DataTableBadgeCell, {
					label: row.original.state,
					variant: variant(row.original.state)
				})
		}
	];

	const table = createSvelteTable({
		get data() {
			return rows;
		},
		columns,
		getRowId: (row) => row.id,
		manualPagination: true,
		getCoreRowModel: getCoreRowModel()
	});
</script>

{#if rows.length === 0}
	<p class="text-sm text-muted-foreground">{emptyLabel}</p>
{:else}
	<div class="space-y-2">
		<DataTable {table} onRowClick={onOpen} class="min-w-[640px]" />
		<div class="flex items-center justify-between gap-2 text-sm text-muted-foreground">
			<span>{offset + 1}–{offset + rows.length} of {total}</span>
			<div class="flex gap-2">
				<Button
					size="sm"
					variant="outline"
					disabled={loading || offset === 0}
					onclick={() => onPage(Math.max(0, offset - pageSize))}>Previous</Button
				>
				<Button
					size="sm"
					variant="outline"
					disabled={loading || offset + rows.length >= total}
					onclick={() => onPage(offset + pageSize)}>Next</Button
				>
			</div>
		</div>
	</div>
{/if}
