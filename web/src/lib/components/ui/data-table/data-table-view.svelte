<script lang="ts" generics="TData">
	import type { Table as TanStackTable } from '@tanstack/table-core';
	import * as Table from '$lib/components/ui/table/index.js';
	import { cn } from '$lib/utils.js';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import FlexRender from './flex-render.svelte';

	let {
		table,
		onRowClick,
		rowClass,
		groups,
		class: className
	}: {
		table: TanStackTable<TData>;
		onRowClick?: (row: TData) => void;
		/** Extra classes for a body row (e.g. cursor when clickable). */
		rowClass?: (row: TData) => string | undefined;
		/**
		 * Rows are already in group order. Each row gets a heading per field in `fields` where its
		 * value starts a new group; headings can be folded and show how many rows they hold.
		 */
		groups?: {
			fields: string[];
			title: (field: string) => string;
			value: (row: TData, field: string) => string;
		};
		class?: string;
	} = $props();

	let folded = $state<Record<string, boolean>>({});

	type Item =
		| { kind: 'heading'; id: string; depth: number; title: string; label: string; count: number }
		| { kind: 'row'; row: ReturnType<TanStackTable<TData>['getRowModel']>['rows'][number] };

	const items = $derived.by((): Item[] => {
		const rows = table.getRowModel().rows;
		if (!groups) return rows.map((row) => ({ kind: 'row', row }));
		const paths = rows.map((row) => groups.fields.map((f) => groups.value(row.original, f)));
		const counts = new Map<string, number>();
		for (const path of paths) {
			for (let d = 0; d < path.length; d++) {
				const id = path.slice(0, d + 1).join('\u0000');
				counts.set(id, (counts.get(id) ?? 0) + 1);
			}
		}
		const out: Item[] = [];
		let previous: string[] = [];
		rows.forEach((row, i) => {
			const path = paths[i]!;
			let start = 0;
			while (start < path.length && path[start] === previous[start]) start++;
			for (let d = start; d < path.length; d++) {
				const id = path.slice(0, d + 1).join('\u0000');
				out.push({
					kind: 'heading',
					id,
					depth: d,
					title: groups.title(groups.fields[d]!),
					label: path[d]!,
					count: counts.get(id) ?? 0
				});
			}
			previous = path;
			// A row hides while any group above it is folded.
			const hidden = path.some((_, d) => folded[path.slice(0, d + 1).join('\u0000')]);
			if (!hidden) out.push({ kind: 'row', row });
		});
		return out;
	});
	// A heading hides when a group above it is folded.
	const visible = $derived(
		items.filter((item) => {
			if (item.kind !== 'heading') return true;
			const parts = item.id.split('\u0000');
			return !parts.slice(0, -1).some((_, d) => folded[parts.slice(0, d + 1).join('\u0000')]);
		})
	);
</script>

<div
	data-part="list.table"
	class={cn('overflow-x-auto rounded-lg border border-border', className)}
>
	<Table.Root>
		<Table.Header>
			{#each table.getHeaderGroups() as headerGroup (headerGroup.id)}
				<Table.Row>
					{#each headerGroup.headers as header (header.id)}
						<Table.Head
							class={header.id === 'select'
								? 'w-10'
								: header.id === 'actions'
									? 'w-[1%] text-right'
									: undefined}
						>
							{#if header.id === 'actions'}
								<span class="sr-only">Actions</span>
							{:else if !header.isPlaceholder}
								<FlexRender
									content={header.column.columnDef.header}
									context={header.getContext()}
								/>
							{/if}
						</Table.Head>
					{/each}
				</Table.Row>
			{/each}
		</Table.Header>
		<Table.Body>
			{#each visible as item (item.kind === 'heading' ? `g:${item.id}` : item.row.id)}
				{#if item.kind === 'heading'}
					<Table.Row data-part="list.group" class="bg-muted/40 hover:bg-muted/40">
						<Table.Cell colspan={table.getVisibleLeafColumns().length} class="py-1.5">
							<button
								type="button"
								class="flex w-full items-center gap-1.5 text-left text-sm font-medium"
								style="padding-left: {item.depth * 1.25}rem"
								aria-expanded={!folded[item.id]}
								onclick={() => (folded = { ...folded, [item.id]: !folded[item.id] })}
							>
								{#if folded[item.id]}
									<ChevronRightIcon class="size-4 shrink-0" />
								{:else}
									<ChevronDownIcon class="size-4 shrink-0" />
								{/if}
								<span class="text-muted-foreground">{item.title}:</span>
								<span>{item.label}</span>
								<span class="ml-1 rounded-full bg-muted px-1.5 text-xs text-muted-foreground">
									{item.count}
								</span>
							</button>
						</Table.Cell>
					</Table.Row>
				{:else}
					{@const row = item.row}
				<Table.Row
				data-part="list.row"
				data-state={row.getIsSelected() ? 'selected' : undefined}
				class={cn(
					onRowClick && !rowClass ? 'cursor-pointer' : undefined,
					rowClass?.(row.original)
				)}
				tabindex={onRowClick ? 0 : undefined}
				onclick={() => onRowClick?.(row.original)}
				onkeydown={(event) => {
					if (!onRowClick) return;
					if (event.key === 'Enter' || event.key === ' ') {
						event.preventDefault();
						onRowClick(row.original);
					}
				}}
			>
				{#each row.getVisibleCells() as cell (cell.id)}
					<Table.Cell class={cell.column.id === 'actions' ? 'text-right' : undefined}>
						<FlexRender content={cell.column.columnDef.cell} context={cell.getContext()} />
					</Table.Cell>
				{/each}
			</Table.Row>
				{/if}
			{/each}
		</Table.Body>
	</Table.Root>
</div>
