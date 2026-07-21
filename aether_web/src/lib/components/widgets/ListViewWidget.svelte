<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import * as Table from '$lib/components/ui/table';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';

	let { node }: { node: PageNode } = $props();

	const columnsNode = $derived((node.children ?? []).find((c) => c.type === 'columns'));
	const columns = $derived(columnsNode?.children ?? []);
	const actions = $derived((node.children ?? []).find((c) => c.type === 'actions'));
	const search = $derived(
		(node.children ?? []).find((c) => c.type === 'search' || c.type === 'filter')
	);
	const empty = $derived((node.children ?? []).find((c) => c.type === 'empty'));

	const demoRows = [
		{ number: 'INV/2026/001', partner: 'Acme Corp', amount_total: '$1,200.00', status: 'draft' },
		{ number: 'INV/2026/002', partner: 'Globex', amount_total: '$890.00', status: 'posted' },
		{ number: 'INV/2026/003', partner: 'Initech', amount_total: '$2,450.00', status: 'draft' }
	];
</script>

<div class="space-y-3">
	{#if search}
		<PageRenderer node={search} />
	{/if}

	{#if actions}
		<PageRenderer node={actions} />
	{/if}

	{#if demoRows.length === 0 && empty}
		<PageRenderer node={empty} />
	{:else}
		<div class="overflow-hidden rounded-md border border-border bg-card">
			<Table.Root>
				<Table.Header>
					<Table.Row>
						{#each columns as col (col.field ?? col.label)}
							<Table.Head>{String(col.label ?? col.field ?? '')}</Table.Head>
						{/each}
						{#if actions}
							<Table.Head class="w-[1%] text-right">Actions</Table.Head>
						{/if}
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each demoRows as row (row.number)}
						<Table.Row>
							{#each columns as col (col.field ?? col.label)}
								{@const key = String(col.field ?? '')}
								{@const value = (row as Record<string, string>)[key] ?? ''}
								<Table.Cell>
									{#if col.fieldType === 'badge'}
										<Badge variant={value === 'posted' ? 'default' : 'secondary'}>{value}</Badge>
									{:else}
										{value}
									{/if}
								</Table.Cell>
							{/each}
							{#if actions}
								<Table.Cell class="text-right">
									<div class="flex justify-end gap-1">
										{#each actions.children ?? [] as action (action.name)}
											<Button
												size="sm"
												variant={action.danger ? 'destructive' : 'outline'}
											>
												{String(action.label ?? action.name)}
											</Button>
										{/each}
									</div>
								</Table.Cell>
							{/if}
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
		</div>
	{/if}
</div>
