<script lang="ts">
	import { resolveField } from '$lib/dsl/resolveFieldType';
	import { goto } from '$app/navigation';
	import SearchXIcon from '@lucide/svelte/icons/search-x';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import type { ColumnDef, RowSelectionState } from '@tanstack/table-core';
	import { getCoreRowModel } from '@tanstack/table-core';
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import ListViewActions from './ListViewActions.svelte';
	import { recordTitles } from '$lib/records/titles';
	import ListViewCell from './ListViewCell.svelte';
	import ListViewSortHeader from './ListViewSortHeader.svelte';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import {
		createSvelteTable,
		DataTable,
		DataTableSelected,
		renderComponent,
		selectColumn
	} from '$lib/components/ui/data-table';
	import { navHref } from '$lib/components/layout/nav';
	import { NativeSelect } from '$lib/components/ui/native-select';
	import { getPageChrome } from '$lib/pages/pageChrome.svelte';
	import {
		getPageState,
		interpolate,
		keyOf,
		rowsOf,
		text,
		totalOf
	} from '$lib/pages/pageContext.svelte';
	import { Q_FIELD, buildListSearchPayload, chipsToQ } from '$lib/search/filterCodec';
	import { parseFilterOptions } from '$lib/search/filterOptions';
	import type { FilterPreset, GroupByOption } from '$lib/search/types';
	import { DEFAULT_LIST_PAGE_SIZE, LIST_PAGE_SIZE_CHOICES, fetchUiPrefs } from '$lib/ui/prefs';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const chrome = getPageChrome();

	const columnsNode = $derived((node.children ?? []).find((c) => c.type === 'columns'));
	const declared = $derived(columnsNode?.children ?? []);
	const actions = $derived(
		((node.children ?? []).find((c) => c.type === 'actions')?.children ?? []).filter((action) =>
			text(action.function)
		)
	);
	const filterNodes = $derived(
		(node.children ?? []).filter((child) => child.type === 'filter' && text(child.field))
	);
	const searchNode = $derived((node.children ?? []).find((c) => c.type === 'search'));
	const searchable = $derived(Boolean(searchNode) || filterNodes.length > 0);
	const groupByNodes = $derived(
		(node.children ?? []).filter(
			(child) => (child.type === 'groupby' || child.type === 'group_by') && text(child.field)
		)
	);
	const empty = $derived((node.children ?? []).find((c) => c.type === 'empty'));
	/// `open("/notes/{id}")`: clicking a row goes to this page.
	const openTemplate = $derived(text(node.open));
	const createHref = $derived(text(node.create));
	const createLabel = $derived(text(node.create_label) ?? text(node.createLabel) ?? 'New');
	const source = $derived(text(node.source));
	const modelName = $derived(text(node.model) || page?.model || null);
	const xmlPageSize = $derived.by(() => {
		const raw = node.page_size ?? node.pageSize;
		const n = typeof raw === 'number' ? raw : typeof raw === 'string' ? Number(raw) : NaN;
		return Number.isFinite(n) && n > 0 ? Math.min(Math.floor(n), 500) : 0;
	});
	/** Server paging / faceted search: the source gets `filter` / `q` / paging. */
	const serverMode = $derived(xmlPageSize > 0 || filterNodes.length > 0 || Boolean(searchNode));
	const defaultOrder = $derived(text(node.default_order) ?? text(node.defaultOrder) ?? '');

	let rows = $state<Record<string, unknown>[] | null>(null);
	/** Plugin, source, and route of the rows on screen. A new page must not keep the last page's rows. */
	let rowsFor = '';
	let total = $state<number | undefined>(undefined);
	let failure = $state<string | null>(null);
	let order = $state('');
	let offset = $state(0);
	let latest = 0;
	let settingPageSize = $state(DEFAULT_LIST_PAGE_SIZE);
	let pageSizeChoices = $state<number[]>([...LIST_PAGE_SIZE_CHOICES]);
	/** User-selected page size for this list (session-local). */
	let selectedPageSize = $state<number | null>(null);
	/** Debounced free-text draft for server-mode search (chips still win when set). */
	let liveQuery = $state('');

	const chips = $derived(chrome?.chips ?? []);
	const groupBy = $derived(chrome?.groupBy ?? []);
	const searchTick = $derived(chrome?.searchTick ?? 0);
	const draftQuery = $derived(chrome?.query ?? '');
	const activeQ = $derived(chipsToQ(chips) ?? '');
	/** Rows are grouped under headings by the fields in `g` (the Group by menu). */
	const grouped = $derived(groupBy.length > 0);
	/** A grouped list loads one large window so every group is whole, and has no pager. */
	const GROUPED_WINDOW = 500;
	const effectivePageSize = $derived.by(() => {
		if (!serverMode) return 0;
		if (grouped) return GROUPED_WINDOW;
		if (selectedPageSize && selectedPageSize > 0) return selectedPageSize;
		if (xmlPageSize > 0) return xmlPageSize;
		return settingPageSize > 0 ? settingPageSize : DEFAULT_LIST_PAGE_SIZE;
	});

	$effect(() => {
		void fetchUiPrefs().then((prefs) => {
			settingPageSize = prefs.list_page_size;
			pageSizeChoices = prefs.list_page_size_choices;
		});
	});

	// Debounce typing so server lists search without requiring Enter.
	$effect(() => {
		const next = draftQuery;
		const handle = setTimeout(() => {
			if (liveQuery === next) return;
			liveQuery = next;
			if (serverMode) offset = 0;
		}, 250);
		return () => clearTimeout(handle);
	});

	function csvFields(raw: string | null | undefined): string[] {
		return (raw ?? '')
			.split(',')
			.map((part) => part.trim())
			.filter(Boolean);
	}

	const searchFields = $derived.by(() => {
		const fromAttr = csvFields(text(searchNode?.fields));
		if (fromAttr.length > 0) return fromAttr;
		// Skip non-text column kinds — `like` over booleans/dates breaks free-text search.
		const skipKinds = new Set([
			'boolean',
			'bool',
			'date',
			'datetime',
			'currency',
			'integer',
			'float'
		]);
		return declared
			.filter((column) => {
				const kind = String(column.fieldType ?? column.field_type ?? '')
					.trim()
					.toLowerCase();
				return !skipKinds.has(kind);
			})
			.map((column) => String(column.field ?? column.name ?? '').trim())
			.filter(Boolean)
			.slice(0, 4);
	});

	const searchFieldLabels = $derived.by(() => {
		const byField = new Map(
			declared.map((column) => [
				String(column.field ?? column.name ?? '').trim(),
				String(column.label ?? column.field ?? column.name ?? '').trim()
			])
		);
		return searchFields.map((field) => byField.get(field) || field.replaceAll('_', ' '));
	});

	const filterPresets = $derived.by((): FilterPreset[] =>
		filterNodes.map((filter) => ({
			field: text(filter.field)!,
			label: text(filter.label) ?? text(filter.field)!,
			options: parseFilterOptions(text(filter.options))
		}))
	);

	const groupByOptions = $derived.by((): GroupByOption[] =>
		groupByNodes.map((row) => ({
			field: text(row.field)!,
			label: text(row.label) ?? text(row.field)!
		}))
	);

	// Register Odoo control-panel chrome (New + faceted search) with PageShell.
	// Reset only on destroy. Clearing inside this effect made every re-run write
	// `groupByOptions = []` and then the real list, which blew the update-depth limit.
	$effect(() => {
		if (!chrome) return;
		chrome.createHref = createHref ?? null;
		chrome.createLabel = createLabel;
		chrome.searchable = searchable;
		chrome.model = modelName;
		chrome.searchFields = searchFields;
		chrome.searchFieldLabels = searchFieldLabels;
		chrome.filterPresets = filterPresets;
		chrome.groupByOptions = groupByOptions;
	});

	$effect(() => {
		if (!chrome) return;
		return chrome.registerRefreshModel(modelName);
	});

	$effect(() => {
		if (!chrome) return;
		return () => {
			chrome.createHref = null;
			chrome.createLabel = 'New';
			chrome.searchable = false;
			chrome.model = null;
			chrome.searchFields = [];
			chrome.searchFieldLabels = [];
			chrome.filterPresets = [];
			chrome.groupByOptions = [];
		};
	});

	$effect(() => {
		order = defaultOrder;
		offset = 0;
	});

	$effect(() => {
		void searchTick;
		offset = 0;
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
			payload.limit = effectivePageSize;
			payload.offset = offset;
			if (order) payload.order = order;
			const search = buildListSearchPayload(chips, searchFields, groupBy);
			payload.filter = search.filter;
			const needle = (search.q ?? liveQuery.trim()) || undefined;
			if (needle) {
				payload.q = needle;
				// Older list sources read free-text as `text`.
				payload.text = needle;
				if (search.search_fields) {
					payload.search_fields = search.search_fields;
					payload.fields = search.search_fields;
				} else if (searchFields.length > 0) {
					payload.search_fields = searchFields;
					payload.fields = searchFields;
				}
			}
			if (search.group_by) payload.group_by = search.group_by;
			// Backward compatible flat filter keys for older plugins.
			for (const chip of chips) {
				if (chip.field === Q_FIELD || chip.op !== 'eq') continue;
				const value = String(chip.value ?? '').trim();
				if (value) payload[chip.field] = value;
			}
		}
		const rowsKey = `${page.plugin}|${source}|${JSON.stringify(page.params)}`;
		if (rowsKey !== rowsFor) {
			rowsFor = rowsKey;
			rows = null;
			failure = null;
		}
		const result = await page.call(source, payload);
		if (current !== latest) return;
		if (result.ok) {
			let next = rowsOf(result.data);
			let nextTotal = serverMode ? totalOf(result.data) : undefined;
			// Defensive: older list sources ignore limit and return every row.
			if (serverMode && effectivePageSize > 0 && next.length > effectivePageSize) {
				if (nextTotal === undefined) nextTotal = next.length;
				next = next.slice(0, effectivePageSize);
			}
			rows = next;
			total = nextTotal;
			failure = null;
		} else {
			// A failed refresh keeps rows already loaded for this page.
			// A failed first load does not keep the previous page's rows.
			if (rows === null) rows = [];
			failure = result.error;
		}
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		void serverMode;
		void effectivePageSize;
		void offset;
		void order;
		void searchTick;
		void chips;
		void groupBy;
		void searchFields;
		void liveQuery;
		void load();
	});
	const columns = $derived.by(() => {
		if (declared.length > 0) {
			return declared.map((column) => {
				const resolved = resolveField(column);
				// A column that names no type still shows a status as a badge, and a yes/no as
				// a tick, because the model says what the field is.
				const kind = !resolved.inferred
					? String(column.fieldType)
					: resolved.fieldType === 'selection'
						? 'badge'
						: ['boolean', 'date', 'datetime', 'image', 'file', 'json'].includes(resolved.fieldType)
							? resolved.fieldType
							: '';
				return {
					field: String(column.field ?? column.name ?? ''),
					label: String(column.label ?? resolved.label),
					kind,
					labels: Object.fromEntries(resolved.options.map((o) => [o.value, o.label])),
					/** The model a link column points at; its records are shown by title. */
					target: resolved.fieldType === 'link' ? resolved.target : null,
					sortable: column.sortable === true || column.sortable === 'true'
				};
			});
		}
		const first = rows?.[0];
		return Object.keys(first ?? {})
			.filter((key) => key !== 'id')
			.map((key) => ({
				field: key,
				label: key.replaceAll('_', ' '),
				kind: '',
				labels: {} as Record<string, string>,
				target: null as string | null,
				sortable: false
			}));
	});

	const shown = $derived.by(() => {
		if (serverMode) return rows ?? [];
		const needle = (activeQ || draftQuery).trim().toLowerCase();
		if (!needle) return rows ?? [];
		return (rows ?? []).filter((row) =>
			columns.some((column) =>
				String(row[column.field] ?? '')
					.toLowerCase()
					.includes(needle)
			)
		);
	});

	// Labels for group headings: a selection's option labels, a column of the row that names the
	// value (`department_name`), or, for a link, the rows of the `<groupby source>` function.
	let linkLabels = $state<Record<string, Record<string, string>>>({});
	$effect(() => {
		void page?.tick;
		if (!page) return;
		const wanted = groupByNodes.filter(
			(row) => groupBy.includes(text(row.field)!) && text(row.source)
		);
		let current = true;
		for (const row of wanted) {
			const field = text(row.field)!;
			const labelField = text(row.optionLabel) ?? 'display_name';
			void page.call(text(row.source)!, { ...page.params, limit: 500 }).then((result) => {
				if (!current || !result.ok) return;
				const map: Record<string, string> = {};
				for (const item of rowsOf(result.data)) {
					map[String(item.id ?? '')] = String(item[labelField] ?? item.name ?? item.id ?? '');
				}
				linkLabels = { ...linkLabels, [field]: map };
			});
		}
		return () => {
			current = false;
		};
	});

	// Link values are shown by the title the linked model names (its `title_field`), asked of
	// the kernel once per page of rows.
	let titles = $state<Record<string, Record<string, string>>>({});
	const linkTargets = $derived.by(() => {
		const found = new Map<string, string>();
		for (const column of columns) {
			if (column.target) found.set(column.field, column.target);
		}
		for (const row of groupByNodes) {
			const field = text(row.field)!;
			const resolved = resolveField(row);
			if (resolved.target && resolved.fieldType === 'link' && !found.has(field)) {
				found.set(field, resolved.target);
			}
		}
		return found;
	});
	$effect(() => {
		const plugin = page?.plugin;
		const loaded = rows;
		if (!plugin || !loaded || loaded.length === 0 || linkTargets.size === 0) return;
		let current = true;
		for (const [field, target] of linkTargets) {
			const ids = loaded
				.map((row) => row[field])
				.filter((value): value is string => typeof value === 'string' && value !== '');
			if (ids.length === 0) continue;
			void recordTitles(plugin, target, ids).then((found) => {
				if (!current || Object.keys(found).length === 0) return;
				titles = { ...titles, [field]: { ...(titles[field] ?? {}), ...found } };
			});
		}
		return () => {
			current = false;
		};
	});

	const keyPart = (value: string) => value.split(':').pop() ?? value;
	function groupLabel(row: Record<string, unknown>, field: string): string {
		const raw = row[field];
		if (raw === null || raw === undefined || raw === '') return 'None';
		const value = String(raw);
		const column = columns.find((c) => c.field === field);
		if (column && value in column.labels) return column.labels[value] ?? value;
		const sibling = row[`${field}_name`] ?? row[`${field}_label`] ?? row[`${field}_display`];
		if (typeof sibling === 'string' && sibling) return sibling;
		const named = titles[field];
		if (named && value in named) return named[value]!;
		const linked = linkLabels[field];
		if (linked) {
			if (value in linked) return linked[value]!;
			const hit = Object.entries(linked).find(([id]) => keyPart(id) === keyPart(value));
			if (hit) return hit[1];
		}
		return value;
	}
	const fieldTitle = (field: string) =>
		groupByNodes.find((row) => text(row.field) === field)?.label
			? String(groupByNodes.find((row) => text(row.field) === field)!.label)
			: (columns.find((c) => c.field === field)?.label ?? field.replaceAll('_', ' '));

	/** The rows in group order (by heading, "None" last), each group keeping the rows' order. */
	const ordered = $derived.by(() => {
		const base = shown;
		if (!grouped) return base;
		const keyOf = (row: Record<string, unknown>) => groupBy.map((f) => groupLabel(row, f));
		return base
			.map((row, index) => ({ row, index, key: keyOf(row) }))
			.sort((a, b) => {
				for (let i = 0; i < groupBy.length; i++) {
					const x = a.key[i]!;
					const y = b.key[i]!;
					if (x === y) continue;
					if (x === 'None') return 1;
					if (y === 'None') return -1;
					return x.localeCompare(y, undefined, { sensitivity: 'base', numeric: true });
				}
				return a.index - b.index;
			})
			.map((entry) => entry.row);
	});

	const pageCount = $derived(
		serverMode && total !== undefined && effectivePageSize > 0
			? Math.max(1, Math.ceil(total / effectivePageSize))
			: 0
	);
	const pageIndex = $derived(
		serverMode && effectivePageSize > 0 ? Math.floor(offset / effectivePageSize) + 1 : 1
	);
	const canPrev = $derived(serverMode && offset > 0);
	const canNext = $derived(
		serverMode &&
			((total !== undefined && offset + effectivePageSize < total) ||
				(total === undefined && (rows?.length ?? 0) >= effectivePageSize))
	);

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
	let rowSelection = $state<RowSelectionState>({});

	$effect(() => {
		void shown;
		rowSelection = {};
	});

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

	const columnDefs = $derived.by((): ColumnDef<Record<string, unknown>>[] => {
		const defs: ColumnDef<Record<string, unknown>>[] = [selectColumn<Record<string, unknown>>()];
		for (const column of columns) {
			// Option labels of a selection, and the titles of a link's records.
			const labels = { ...column.labels, ...(titles[column.field] ?? {}) };
			defs.push({
				id: column.field,
				accessorKey: column.field,
				header: () =>
					renderComponent(ListViewSortHeader, {
						label: column.label,
						field: column.field,
						sortable: serverMode && column.sortable,
						mark: sortMark(column.field),
						onToggle: toggleSort
					}),
				cell: ({ row }) =>
					renderComponent(ListViewCell, {
						value: row.original[column.field],
						kind: column.kind,
						labels
					})
			});
		}
		if (actions.length > 0) {
			defs.push({
				id: 'actions',
				header: () => '',
				cell: ({ row }) =>
					renderComponent(ListViewActions, {
						actions,
						row: row.original,
						onRun: run
					}),
				enableSorting: false,
				enableHiding: false
			});
		}
		return defs;
	});

	const table = createSvelteTable({
		get data() {
			return ordered;
		},
		get columns() {
			return columnDefs;
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
		getRowId: (row, index) => String(row.id ?? index)
	});

	const selectedCount = $derived(
		Object.keys(rowSelection).filter((key) => rowSelection[key]).length
	);
</script>

<div class="space-y-3">
	{#if rows === null}
		<div class="space-y-2" aria-busy="true">
			<Skeleton class="h-9 w-full" />
			<Skeleton class="h-9 w-full" />
			<Skeleton class="h-9 w-3/4" />
		</div>
	{:else if rows.length === 0 && !activeQ && !draftQuery.trim() && chips.length === 0}
		{#if empty}
			<PageRenderer node={empty} />
		{:else}
			<p
				class="rounded-md border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground"
			>
				Nothing here yet.
			</p>
		{/if}
	{:else if shown.length === 0}
		<!-- Something is searched or filtered and nothing matches: say what, and offer the way back. -->
		{@const typed = (activeQ || draftQuery).trim()}
		{@const facets = chips.filter((chip) => chip.field !== '_q')}
		<div
			data-part="list.no-results"
			class="flex flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-border bg-muted/30 px-6 py-14 text-center"
			role="status"
		>
			<span class="flex size-12 items-center justify-center rounded-full bg-muted">
				<SearchXIcon class="size-6 text-muted-foreground" />
			</span>
			<div class="space-y-1">
				<p class="text-base font-medium text-foreground">No results found</p>
				<p class="max-w-md text-sm text-muted-foreground">
					{#if typed}
						Nothing matches <span class="font-medium text-foreground">“{typed}”</span>{facets.length >
						0
							? ' with the filters you chose'
							: ''}. Check the spelling or try a shorter search.
					{:else}
						No record matches the filters you chose. Try removing one.
					{/if}
				</p>
			</div>
			{#if facets.length > 0}
				<ul class="flex flex-wrap justify-center gap-1.5">
					{#each facets as chip (chip.id)}
						<li>
							<button
								type="button"
								class="inline-flex items-center gap-1 rounded-full border border-border bg-card px-2.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground"
								title="Remove this filter"
								onclick={() => chrome?.removeChip(chip.id)}
							>
								{chip.label}
								<span aria-hidden="true">×</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
			<Button
				type="button"
				size="sm"
				variant="outline"
				onclick={() => {
					if (!chrome) return;
					chrome.query = '';
					chrome.setChips([]);
				}}
			>
				Clear search
			</Button>
		</div>
	{:else}
		<DataTableSelected count={selectedCount} onClear={() => (rowSelection = {})} />
		<DataTable
			{table}
			onRowClick={openTemplate ? openRow : undefined}
			rowClass={() => (openTemplate ? 'cursor-pointer' : undefined)}
			groups={grouped
				? {
						fields: groupBy,
						title: fieldTitle,
						value: (row, field) => groupLabel(row as Record<string, unknown>, field)
					}
				: undefined}
		/>

		{#if serverMode && !grouped && (canPrev || canNext || total !== undefined || effectivePageSize > 0)}
			<div class="flex flex-wrap items-center justify-between gap-3 text-sm text-muted-foreground">
				<span>
					{#if total !== undefined}
						{total === 0 ? '0' : `${offset + 1}–${Math.min(offset + effectivePageSize, total)}`} of {total}
					{:else}
						Page {pageIndex}
					{/if}
				</span>
				<div class="flex flex-wrap items-center gap-2">
					<label class="flex items-center gap-2">
						<span class="whitespace-nowrap">Rows</span>
						<NativeSelect
							size="sm"
							aria-label="Rows per page"
							value={String(effectivePageSize)}
							onchange={(event) => {
								const next = Number((event.currentTarget as HTMLSelectElement).value);
								if (!Number.isFinite(next) || next < 1) return;
								selectedPageSize = next;
								offset = 0;
							}}
						>
							{#each pageSizeChoices as size (size)}
								<option value={String(size)}>{size}</option>
							{/each}
							{#if !pageSizeChoices.includes(effectivePageSize)}
								<option value={String(effectivePageSize)}>{effectivePageSize}</option>
							{/if}
						</NativeSelect>
					</label>
					<div class="flex items-center gap-1">
						<Button
							size="sm"
							variant="outline"
							disabled={!canPrev}
							aria-label="Previous page"
							onclick={() => {
								offset = Math.max(0, offset - effectivePageSize);
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
								offset = offset + effectivePageSize;
							}}
						>
							<ChevronRightIcon class="size-4" />
						</Button>
					</div>
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
