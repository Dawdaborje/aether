import type { ColumnDef } from '@tanstack/table-core';
import { renderComponent } from './render-helpers.js';
import DataTableCheckbox from './data-table-checkbox.svelte';

/** Leading checkbox column for row selection (shadcn Data Table pattern). */
export function selectColumn<TData>(): ColumnDef<TData> {
	return {
		id: 'select',
		header: ({ table }) =>
			renderComponent(DataTableCheckbox, {
				checked: table.getIsAllPageRowsSelected(),
				indeterminate: table.getIsSomePageRowsSelected() && !table.getIsAllPageRowsSelected(),
				onCheckedChange: (value) => table.toggleAllPageRowsSelected(!!value),
				'aria-label': 'Select all'
			}),
		cell: ({ row }) =>
			renderComponent(DataTableCheckbox, {
				checked: row.getIsSelected(),
				onCheckedChange: (value) => row.toggleSelected(!!value),
				'aria-label': 'Select row'
			}),
		enableSorting: false,
		enableHiding: false
	};
}
