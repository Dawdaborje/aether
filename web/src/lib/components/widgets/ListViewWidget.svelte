<script lang="ts">
	import { goto } from '$app/navigation';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import * as Table from '$lib/components/ui/table';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { navHref } from '$lib/components/layout/nav';
	import { getPageChrome } from '$lib/pages/pageChrome.svelte';
	import { getPageState, interpolate, keyOf, rowsOf, text, totalOf } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const chrome = getPageChrome();

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
	const createHref = $derived(text(node.create));
	const createLabel = $derived(text(node.create_label) ?? text(node.createLabel) ?? 'New');
	const source = $derived(text(node.source));
	const pageSize = $derived.by(() => {
		const raw = node.page_size ?? node.pageSize;
		const n = typeof raw === 'number' ? raw : typeof raw === 'string' ? Number(raw) : NaN;
		return Number.isFinite(n) && n > 0 ? Math.min(Math.floor(n), 500) : 0;
	});
	const serverMode = $derived(pageSize > 0);
	const defaultOrder = $derived(text(node.default_order) ?? text(node.defaultOrder) ?? '');

	let rows = $state<Record<string, unknown>[] | null>(null);
	let total = $state<number | undefined>(undefined);
	let failure = $state<string | null>(null);
	let debouncedQuery = $state('');
	let order = $state('');
	let offset = $state(0);
	let latest = 0;
	let debounceTimer: ReturnType<typeof setTimeout> | undefined;

	const query = $derived(chrome?.query ?? '');

	// Register Odoo control-panel chrome (New + search) with PageShell.
	$effect(() => {
		if (!chrome) return;
		chrome.createHref = createHref ?? null;
		chrome.createLabel = createLabel;
		chrome.searchable = searchable;
		return () => {
			chrome.createHref = null;
			chrome.createLabel = 'New';
			chrome.searchable = false;
		};
	});

	$effect(() => {
		order = defaultOrder;
		offset = 0;
	});

	$effect(() => {
		const next = query;
		if (!serverMode) {
			debouncedQuery = next;
			return;
		}
		clearTimeout(debounceTimer);
		debounceTimer = setTimeout(() => {
			debouncedQuery = next;
			offset = 0;
		}, 250);
		return () => clearTimeout(debounceTimer);
	});

	async function load() {
		const current = ++latest;
		if (!source || !page) {
			rows = [];
			total = undefined;
			return;
		}
		const payload: Record<string, unknown> = { ...page.params };
		if (serverMode) {
			payload.limit = pageSize;
			payload.offset = offset;
			if (debouncedQuery.trim()) payload.q = debouncedQuery.trim();
			if (order) payload.order = order;
		}
		const result = await page.call(source, payload);
		if (current !== latest) return;
		if (result.ok) {
			rows = rowsOf(result.data);
			total = serverMode ? totalOf(result.data) : undefined;
			failure = null;
		} else {
			rows = rows ?? [];
			failure = result.error;
		}
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		void serverMode;
		void pageSize;
		void offset;
		void debouncedQuery;
		void order;
		void load();
	});

	const columns = $derived.by(() => {
		if (declared.length > 0) {
			return declared.map((column) => ({
				field: String(column.field ?? column.name ?? ''),
				label: String(column.label ?? column.field ?? column.name ?? ''),
				kind: String(column.fieldType ?? ''),
				sortable: column.sortable === true || column.sortable === 'true'
			}));
		}
		const first = rows?.[0];
		return Object.keys(first ?? {})
			.filter((key) => key !== 'id')
			.map((key) => ({ field: key, label: key.replaceAll('_', ' '), kind: '', sortable: false }));
	});

	const shown = $derived.by(() => {
		if (serverMode) return rows ?? [];
		const needle = query.trim().toLowerCase();
		if (!needle) return rows ?? [];
		return (rows ?? []).filter((row) =>
			columns.some((column) => String(row[column.field] ?? '').toLowerCase().includes(needle))
		);
	});

	const pageCount = $derived(
		serverMode && total !== undefined && pageSize > 0 ? Math.max(1, Math.ceil(total / pageSize)) : 0
	);
	const pageIndex = $derived(serverMode && pageSize > 0 ? Math.floor(offset / pageSize) + 1 : 1);
	const canPrev = $derived(serverMode && offset > 0);
	const canNext = $derived(
		serverMode &&
			((total !== undefined && offset + pageSize < total) ||
				(total === undefined && (rows?.length ?? 0) >= pageSize))
	);

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

	function toggleSort(field: string) {
		if (!serverMode || !field) return;
		offset = 0;
		if (order === field) order = `-${field}`;
		else if (order === `-${field}`) order = field;
		else order = field;
	}

	function sortMark(field: string): string {
		if (order === field) return ' ↑';
		if (order === `-${field}`) return ' ↓';
		return '';
	}

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
	{#if failure}
		<p class="text-sm text-destructive" role="alert">{failure}</p>
	{/if}

	{#if rows === null}
		<div class="space-y-2" aria-busy="true">
			<Skeleton class="h-9 w-full" />
			<Skeleton class="h-9 w-full" />
			<Skeleton class="h-9 w-3/4" />
		</div>
	{:else if rows.length === 0 && !query.trim()}
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
		<Card.Root size="sm" class="overflow-hidden py-0">
			<Card.Content class="p-0">
			<Table.Root>
				<Table.Header>
					<Table.Row>
						{#each columns as column (column.field)}
							<Table.Head>
								{#if serverMode && column.sortable}
									<button
										type="button"
										class="inline-flex items-center gap-0.5 font-medium hover:text-foreground"
										onclick={() => toggleSort(column.field)}
									>
										{column.label}{sortMark(column.field)}
									</button>
								{:else}
									{column.label}
								{/if}
							</Table.Head>
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
			</Card.Content>
		</Card.Root>

		{#if serverMode && (canPrev || canNext || total !== undefined)}
			<div class="flex items-center justify-between gap-3 text-sm text-muted-foreground">
				<span>
					{#if total !== undefined}
						{total === 0 ? '0' : `${offset + 1}–${Math.min(offset + pageSize, total)}`} of {total}
					{:else}
						Page {pageIndex}
					{/if}
				</span>
				<div class="flex items-center gap-1">
					<Button
						size="sm"
						variant="outline"
						disabled={!canPrev}
						aria-label="Previous page"
						onclick={() => {
							offset = Math.max(0, offset - pageSize);
						}}
					>
						<ChevronLeftIcon class="size-4" />
					</Button>
					{#if pageCount > 0}
						<span class="min-w-16 text-center tabular-nums">{pageIndex} / {pageCount}</span>
					{/if}
					<Button
						size="sm"
						variant="outline"
						disabled={!canNext}
						aria-label="Next page"
						onclick={() => {
							offset = offset + pageSize;
						}}
					>
						<ChevronRightIcon class="size-4" />
					</Button>
				</div>
			</div>
		{/if}
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
