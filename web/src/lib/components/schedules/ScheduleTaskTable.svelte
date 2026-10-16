<script lang="ts">
	import type { ColumnDef, RowSelectionState } from '@tanstack/table-core';
	import { getCoreRowModel } from '@tanstack/table-core';
	import {
		createSvelteTable,
		DataTable,
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
		selectedKeys,
		onSelect,
		onOpen,
		onStop,
		onRestart,
		onRunNow
	}: {
		rows: ScheduleTask[];
		emptyLabel: string;
		rowWorking?: string | null;
		selectedKeys: Set<string>;
		onSelect: (keys: Set<string>) => void;
		onOpen: (task: ScheduleTask) => void;
		onStop: (task: ScheduleTask, event: MouseEvent) => void;
		onRestart: (task: ScheduleTask, event: MouseEvent) => void;
		onRunNow: (task: ScheduleTask, event: MouseEvent) => void;
	} = $props();

	// Selection lives in the panel so "select all" can span every table and survive filtering.
	const rowSelection = $derived<RowSelectionState>(
		Object.fromEntries([...selectedKeys].map((key) => [key, true]))
	);

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
				renderComponent(ScheduleTaskNameCell, { task: row.original })
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
		getRowId: (row) => scheduleKey(row),
		onRowSelectionChange: (updater) => {
			const next = typeof updater === 'function' ? updater(rowSelection) : updater;
			// Keep selections that belong to other tables or are hidden by the filter.
			const visible = new Set(rows.map(scheduleKey));
			const keys = new Set([...selectedKeys].filter((key) => !visible.has(key)));
			for (const [key, on] of Object.entries(next)) if (on) keys.add(key);
			onSelect(keys);
		},
		getCoreRowModel: getCoreRowModel(),
		enableRowSelection: true
	});

</script>

{#if rows.length === 0}
	<p class="text-sm text-muted-foreground">{emptyLabel}</p>
{:else}
	<div class="space-y-2">
		<DataTable {table} onRowClick={onOpen} class="min-w-[640px]" />
	</div>
{/if}
