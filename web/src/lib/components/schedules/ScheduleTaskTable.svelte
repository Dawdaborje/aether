<script lang="ts">
	import type { ColumnDef, RowSelectionState } from '@tanstack/table-core';
	import { getCoreRowModel } from '@tanstack/table-core';
	import {
		createSvelteTable,
		DataTable,
		DataTableSelected,
		renderComponent,
		selectColumn
	} from '$lib/components/ui/data-table';
	import DataTableBadgeCell from '$lib/components/ui/data-table/data-table-badge-cell.svelte';
	import { formatEvery, type ScheduleTask } from '$lib/schedules/api';
	import ScheduleTaskActions from './ScheduleTaskActions.svelte';
	import ScheduleTaskNameCell from './ScheduleTaskNameCell.svelte';

	let {
		rows,
		emptyLabel,
		rowWorking = null,
		onOpen,
		onStop,
		onRestart,
		onRunNow
	}: {
		rows: ScheduleTask[];
		emptyLabel: string;
		rowWorking?: string | null;
		onOpen: (task: ScheduleTask) => void;
		onStop: (task: ScheduleTask, event: MouseEvent) => void;
		onRestart: (task: ScheduleTask, event: MouseEvent) => void;
		onRunNow: (task: ScheduleTask, event: MouseEvent) => void;
	} = $props();

	let rowSelection = $state<RowSelectionState>({});

	$effect(() => {
		void rows;
		rowSelection = {};
	});

	function scheduleKey(task: ScheduleTask): string {
		return `${task.plugin}:${task.name}`;
	}

	function formatWhen(iso: string | null | undefined): string {
		if (!iso) return '—';
		const date = new Date(iso);
		return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
	}

	function expression(task: ScheduleTask): string {
		if (task.kind === 'cron') {
			return task.timezone ? `${task.cron} (${task.timezone})` : String(task.cron ?? '');
		}
		return `every ${formatEvery(task.every_secs)}`;
	}

	const columns = $derived.by((): ColumnDef<ScheduleTask>[] => [
		selectColumn<ScheduleTask>(),
		{
			id: 'schedule',
			header: 'Schedule',
			cell: ({ row }) =>
				renderComponent(ScheduleTaskNameCell, {
					plugin: row.original.plugin,
					name: row.original.name,
					functionName: row.original.function,
					source: row.original.source
				})
		},
		{
			id: 'expression',
			header: 'Expression',
			cell: ({ row }) => expression(row.original)
		},
		{
			id: 'next',
			header: 'Next',
			cell: ({ row }) => formatWhen(row.original.next_run)
		},
		{
			id: 'status',
			header: 'Status',
			cell: ({ row }) =>
				renderComponent(DataTableBadgeCell, {
					label: row.original.enabled ? 'Enabled' : 'Stopped',
					variant: row.original.enabled ? 'default' : 'secondary'
				})
		},
		{
			id: 'actions',
			header: '',
			cell: ({ row }) =>
				renderComponent(ScheduleTaskActions, {
					task: row.original,
					busy: rowWorking === scheduleKey(row.original),
					onStop,
					onRestart,
					onRunNow
				})
		}
	]);

	const table = createSvelteTable({
		get data() {
			return rows;
		},
		get columns() {
			return columns;
		},
		state: {
			get rowSelection() {
				return rowSelection;
			}
		},
		onRowSelectionChange: (updater) => {
			rowSelection = typeof updater === 'function' ? updater(rowSelection) : updater;
		},
		getCoreRowModel: getCoreRowModel(),
		enableRowSelection: true,
		getRowId: (row) => scheduleKey(row)
	});

	const selectedCount = $derived(Object.keys(rowSelection).filter((key) => rowSelection[key]).length);
</script>

{#if rows.length === 0}
	<p class="text-sm text-muted-foreground">{emptyLabel}</p>
{:else}
	<div class="space-y-2">
		<DataTableSelected count={selectedCount} onClear={() => (rowSelection = {})} />
		<DataTable {table} onRowClick={onOpen} class="min-w-[640px]" />
	</div>
{/if}
