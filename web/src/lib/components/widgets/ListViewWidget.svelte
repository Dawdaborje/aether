<script lang="ts">
	import { goto } from '$app/navigation';
	import SearchIcon from '@lucide/svelte/icons/search';
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import * as Table from '$lib/components/ui/table';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { navHref } from '$lib/components/layout/nav';
	import { getPageState, interpolate, keyOf, rowsOf, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();

	const columnsNode = $derived((node.children ?? []).find((c) => c.type === 'columns'));
	const declared = $derived(columnsNode?.children ?? []);
	const actions = $derived(
		((node.children ?? []).find((c) => c.type === 'actions')?.children ?? []).filter(
			(action) => text(action.function)
		)
	);
	const searchable = $derived((node.children ?? []).some((c) => c.type === 'search' || c.type === 'filter'));
	const empty = $derived((node.children ?? []).find((c) => c.type === 'empty'));
	/// `open="/notes/{id}"`: clicking a row goes to this page.
	const openTemplate = $derived(text(node.open));
	const source = $derived(text(node.source));

	let rows = $state<Record<string, unknown>[] | null>(null);
	let failure = $state<string | null>(null);
	let query = $state('');
	let latest = 0;

	async function load() {
		const current = ++latest;
		if (!source || !page) {
			rows = [];
			return;
		}
		const result = await page.call(source, { ...page.params });
		if (current !== latest) return; // a newer load is under way
		if (result.ok) {
			rows = rowsOf(result.data);
			failure = null;
		} else {
			rows = rows ?? [];
			failure = result.error;
		}
	}

	// Load on first show, and again whenever the page's plugin, address or data changes.
	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		void load();
	});

	// Columns as declared, or the fields of the first row.
	const columns = $derived.by(() => {
		if (declared.length > 0) {
			return declared.map((column) => ({
				field: String(column.field ?? column.name ?? ''),
				label: String(column.label ?? column.field ?? column.name ?? ''),
				kind: String(column.fieldType ?? '')
			}));
		}
		const first = rows?.[0];
		return Object.keys(first ?? {})
			.filter((key) => key !== 'id')
			.map((key) => ({ field: key, label: key.replaceAll('_', ' '), kind: '' }));
	});

	const shown = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return rows ?? [];
		return (rows ?? []).filter((row) =>
			columns.some((column) => String(row[column.field] ?? '').toLowerCase().includes(needle))
		);
	});

	function cell(value: unknown, kind: string): string {
		if (value === null || value === undefined) return '';
		if (kind === 'boolean') return value ? '✓' : '';
		if (kind === 'date' || kind === 'datetime') {
			const date = new Date(String(value));
			return Number.isNaN(date.getTime())
				? String(value)
				: kind === 'date'
					? date.toLocaleDateString()
					: date.toLocaleString();
		}
		if (kind === 'currency' && typeof value === 'number') {
			return value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
		}
		return typeof value === 'object' ? JSON.stringify(value) : String(value);
	}

	function openRow(row: Record<string, unknown>) {
		if (openTemplate) void goto(navHref(interpolate(openTemplate, row)));
	}

	/** An action waiting for the person to confirm it. */
	let pending = $state<{ action: PageNode; row: Record<string, unknown> } | null>(null);
	let busy = $state(false);

	function run(action: PageNode, row: Record<string, unknown>) {
		if (text(action.confirm)) {
			pending = { action, row };
			return;
		}
		void perform(action, row);
	}

	async function perform(action: PageNode, row: Record<string, unknown>) {
		const fn = text(action.function);
		if (!fn || !page) return;
		busy = true;
		const result = await page.call(fn, { ...page.params, id: keyOf(row.id) });
		busy = false;
		pending = null;
		if (result.ok) {
			failure = null;
			page.refresh();
		} else {
			failure = result.error;
		}
	}
</script>

<div class="space-y-3">
	{#if searchable}
		<div class="relative max-w-sm">
			<SearchIcon
				class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
			/>
			<input
				bind:value={query}
				type="search"
				placeholder="Search…"
				aria-label="Search"
				class="h-9 w-full rounded-md border border-border bg-card pr-3 pl-9 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/20"
			/>
		</div>
	{/if}

	{#if failure}
		<p class="text-sm text-destructive" role="alert">{failure}</p>
	{/if}

	{#if rows === null}
		<div class="space-y-2" aria-busy="true">
			<Skeleton class="h-9 w-full" />
			<Skeleton class="h-9 w-full" />
			<Skeleton class="h-9 w-3/4" />
		</div>
	{:else if rows.length === 0}
		{#if empty}
			<PageRenderer node={empty} />
		{:else}
			<p class="rounded-md border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground">
				Nothing here yet.
			</p>
		{/if}
	{:else if shown.length === 0}
		<p class="text-sm text-muted-foreground">No row matches “{query}”.</p>
	{:else}
		<div class="overflow-hidden rounded-md border border-border bg-card">
			<Table.Root>
				<Table.Header>
					<Table.Row>
						{#each columns as column (column.field)}
							<Table.Head>{column.label}</Table.Head>
						{/each}
						{#if actions.length > 0}
							<Table.Head class="w-[1%] text-right"><span class="sr-only">Actions</span></Table.Head>
						{/if}
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each shown as row, index (String(row.id ?? index))}
						<Table.Row
							class={openTemplate ? 'cursor-pointer' : ''}
							onclick={() => openRow(row)}
						>
							{#each columns as column (column.field)}
								<Table.Cell>
									{#if column.kind === 'badge'}
										<Badge variant="secondary">{cell(row[column.field], column.kind)}</Badge>
									{:else}
										{cell(row[column.field], column.kind)}
									{/if}
								</Table.Cell>
							{/each}
							{#if actions.length > 0}
								<Table.Cell class="text-right">
									<div class="flex justify-end gap-1">
										{#each actions as action (action.name ?? action.function)}
											<Button
												size="sm"
												variant={action.danger ? 'destructive' : 'outline'}
												onclick={(event: MouseEvent) => {
													event.stopPropagation();
													run(action, row);
												}}
											>
												{String(action.label ?? action.name ?? 'Action')}
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

<AlertDialog.Root open={pending !== null} onOpenChange={(open) => !open && (pending = null)}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{String(pending?.action.label ?? 'Are you sure?')}</AlertDialog.Title>
			<AlertDialog.Description>{text(pending?.action.confirm) ?? ''}</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
			<AlertDialog.Action
				disabled={busy}
				onclick={() => pending && perform(pending.action, pending.row)}
			>
				Confirm
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
