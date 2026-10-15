<script lang="ts">
	import { resolveField } from '$lib/dsl/resolveFieldType';
	import { goto } from '$app/navigation';
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import type { PageNode } from '$lib/dsl/types';
	import { titlesForRows } from '$lib/records/titles';
	import PageRenderer from './PageRenderer.svelte';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { navHref } from '$lib/components/layout/nav';
	import {
		cardBodyFields,
		cardFooter,
		cardTitleField,
		columnProgress,
		formatCell,
		groupRows,
		initialsOf,
		parseColors,
		type KanbanColumn
	} from '$lib/pages/kanban';
	import { getPageChrome } from '$lib/pages/pageChrome.svelte';
	import { getPageState, interpolate, keyOf, rowsOf, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const chrome = getPageChrome();

	const source = $derived(text(node.source));
	const modelName = $derived(text(node.model) || page?.model || null);
	const groupBy = $derived(text(node.group_by) ?? text(node.groupBy) ?? '');
	const openTemplate = $derived(text(node.open));
	const moveFn = $derived(text(node.move));
	const createFn = $derived(text(node.create));

	const progressNode = $derived((node.children ?? []).find((c) => c.type === 'progressbar'));
	const cardNode = $derived((node.children ?? []).find((c) => c.type === 'card'));
	const actions = $derived(
		(
			(node.children ?? []).find((c) => c.type === 'actions' || c.type === 'menu')?.children ?? []
		).filter((action) => text(action.function))
	);
	const empty = $derived((node.children ?? []).find((c) => c.type === 'empty'));
	const legacyGroups = $derived(
		!source ? (node.children ?? []).filter((c) => c.type === 'group') : []
	);

	const progressField = $derived(text(progressNode?.field) ?? text(progressNode?.name) ?? '');
	const progressColors = $derived(parseColors(progressNode?.colors));
	const sumField = $derived(text(progressNode?.sum_field) ?? text(progressNode?.sumField));

	let rows = $state<Record<string, unknown>[] | null>(null);
	let failure = $state<string | null>(null);
	let progressFilter = $state<string | null>(null);
	let latest = 0;
	let draggingId = $state<string | null>(null);
	let dropTarget = $state<string | null>(null);
	let creatingFor = $state<string | null>(null);
	let createTitle = $state('');
	let createBusy = $state(false);

	async function load() {
		const current = ++latest;
		if (!source || !page) {
			rows = [];
			return;
		}
		const result = await page.call(source, { ...page.params });
		if (current !== latest) return;
		if (result.ok) {
			rows = rowsOf(result.data);
			failure = null;
		} else {
			rows = rows ?? [];
			failure = result.error;
		}
	}

	$effect(() => {
		if (!chrome) return;
		return chrome.registerRefreshModel(modelName);
	});

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		void load();
	});

	// Linked records show by name: the card's link fields, and the field the board groups by.
	let titles = $state<Record<string, Record<string, string>>>({});
	const linkFields = $derived.by(() => {
		const found = new Map<string, string>();
		const nodes = [...cardBodyFields(cardNode), ...(cardFooter(cardNode)?.children ?? [])];
		for (const field of nodes) {
			const resolved = resolveField(field);
			const name = fieldName(field);
			if (name && resolved.fieldType === 'link' && resolved.target) found.set(name, resolved.target);
		}
		const groupTarget = text(node.groupByTarget);
		if (groupBy && groupTarget && !found.has(groupBy)) found.set(groupBy, groupTarget);
		return found;
	});
	$effect(() => {
		const plugin = page?.plugin;
		const loaded = rows;
		if (!plugin || !loaded || loaded.length === 0 || linkFields.size === 0) return;
		let current = true;
		void titlesForRows(plugin, loaded, linkFields).then((found) => {
			if (current) titles = { ...titles, ...found };
		});
		return () => {
			current = false;
		};
	});
	/** A link value as its title; any other value unchanged. */
	function named(field: string, value: unknown): unknown {
		return typeof value === 'string' ? (titles[field]?.[value] ?? value) : value;
	}

	const columns = $derived.by((): KanbanColumn[] => {
		if (!rows || !groupBy) return [];
		return groupRows(rows, groupBy).map((col) => ({
			...col,
			label: named(groupBy, col.key) === col.key ? col.label : String(named(groupBy, col.key))
		}));
	});

	function filteredColumnRows(col: KanbanColumn): Record<string, unknown>[] {
		if (!progressFilter || !progressField) return col.rows;
		return col.rows.filter((row) => {
			const raw = row[progressField];
			const key =
				raw === null || raw === undefined || raw === ''
					? ''
					: typeof raw === 'object' && raw !== null && 'name' in raw
						? String((raw as { name?: unknown }).name ?? '')
						: String(raw);
			return key === progressFilter;
		});
	}

	function openRow(row: Record<string, unknown>) {
		if (openTemplate) void goto(navHref(interpolate(openTemplate, row)));
	}

	function fieldKind(field: PageNode): string {
		if (field.fieldType) return String(field.fieldType);
		if (field.type === 'badge' || field.type === 'avatar') return String(field.type);
		// The model says a status is one of a few options: show it as a badge.
		return resolveField(field).fieldType === 'selection' ? 'badge' : '';
	}

	function fieldName(field: PageNode): string {
		return String(field.name ?? field.field ?? '');
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

	function onDragStart(event: DragEvent, row: Record<string, unknown>) {
		if (!moveFn) return;
		const id = keyOf(row.id);
		draggingId = id;
		event.dataTransfer?.setData('text/plain', id);
		if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
	}

	function onDragEnd() {
		draggingId = null;
		dropTarget = null;
	}

	function onDragOver(event: DragEvent, columnKey: string) {
		if (!moveFn) return;
		event.preventDefault();
		dropTarget = columnKey;
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
	}

	async function onDrop(event: DragEvent, to: string) {
		event.preventDefault();
		dropTarget = null;
		if (!moveFn || !page || !groupBy) return;
		const id = event.dataTransfer?.getData('text/plain') || draggingId;
		draggingId = null;
		if (!id) return;
		const row = (rows ?? []).find((r) => keyOf(r.id) === id);
		if (!row) return;
		const from = groupBy ? String(row[groupBy] ?? '') : '';
		if (String(from) === to || (from === '' && to === 'Unassigned')) return;
		const result = await page.call(moveFn, {
			...page.params,
			id,
			group_by_field: groupBy,
			from,
			to: to === 'Unassigned' ? '' : to
		});
		if (result.ok) {
			failure = null;
			page.refresh();
		} else {
			failure = result.error;
		}
	}

	async function submitCreate(columnKey: string) {
		const title = createTitle.trim();
		if (!createFn || !page || !title || !groupBy) return;
		createBusy = true;
		const payload: Record<string, unknown> = {
			...page.params,
			title,
			[groupBy]: columnKey === 'Unassigned' ? '' : columnKey
		};
		const result = await page.call(createFn, payload);
		createBusy = false;
		if (result.ok) {
			createTitle = '';
			creatingFor = null;
			failure = null;
			page.refresh();
		} else {
			failure = result.error;
		}
	}
</script>

{#if !source && legacyGroups.length > 0}
	<!-- Static layout fallback when no source is declared -->
	<div class="grid gap-4 md:grid-cols-3">
		{#each legacyGroups as col, i (col.title ?? i)}
			<div class="min-h-40 space-y-2 rounded-md border border-border bg-muted/40 p-3">
				<h4 class="text-sm font-semibold text-foreground">{col.title}</h4>
				{#each col.children ?? [] as child, j (child.name ?? j)}
					<PageRenderer node={child} />
				{/each}
			</div>
		{/each}
	</div>
{:else}
	<div class="space-y-3">
		{#if rows === null}
			<div class="grid gap-4 md:grid-cols-3" aria-busy="true">
				{#each [1, 2, 3] as i (i)}
					<div class="space-y-2 rounded-md border border-border p-3">
						<Skeleton class="h-5 w-24" />
						<Skeleton class="h-20 w-full" />
						<Skeleton class="h-20 w-full" />
					</div>
				{/each}
			</div>
		{:else if rows.length === 0}
			{#if empty}
				<PageRenderer node={empty} />
			{:else}
				<p
					class="rounded-md border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground"
				>
					Nothing here yet.
				</p>
			{/if}
		{:else if !groupBy}
			<p class="text-sm text-muted-foreground" role="alert">
				Kanban needs a <code class="text-xs">group_by</code> field.
			</p>
		{:else}
			<div class="flex gap-3 overflow-x-auto pb-2" style="min-height: 16rem;">
				{#each columns as col (col.key)}
					{@const prog = progressField
						? columnProgress(col.rows, progressField, progressColors, sumField)
						: null}
					{@const shown = filteredColumnRows(col)}
					<div
						data-part="kanban.column"
						class="flex w-72 shrink-0 flex-col rounded-md border border-border bg-muted/30 {dropTarget ===
						col.key
							? 'ring-2 ring-ring'
							: ''}"
						ondragover={(e) => onDragOver(e, col.key)}
						ondrop={(e) => onDrop(e, col.key)}
						role="list"
						aria-label={col.label}
					>
						<div class="space-y-2 border-b border-border px-3 py-2">
							<div class="flex items-center justify-between gap-2">
								<h4 class="truncate text-sm font-semibold text-foreground">{col.label}</h4>
								<span class="text-xs text-muted-foreground">{col.rows.length}</span>
							</div>
							{#if prog && prog.total > 0}
								<div class="flex h-1.5 overflow-hidden rounded-full bg-muted">
									{#each prog.segments as seg (seg.key)}
										<button
											type="button"
											class="h-full {seg.color} transition-opacity {progressFilter &&
											progressFilter !== seg.key
												? 'opacity-30'
												: ''}"
											style="width: {(seg.count / prog.total) * 100}%"
											title="{seg.key}: {seg.count}"
											onclick={() => (progressFilter = progressFilter === seg.key ? null : seg.key)}
										></button>
									{/each}
								</div>
								{#if prog.sum !== null}
									<p class="text-xs text-muted-foreground">
										Σ {prog.sum.toLocaleString(undefined, {
											minimumFractionDigits: 0,
											maximumFractionDigits: 2
										})}
									</p>
								{/if}
							{/if}
						</div>

						<div class="flex flex-1 flex-col gap-2 p-2">
							{#each shown as row, index (String(row.id ?? index))}
								{@const titleField = cardTitleField(cardNode, row)}
								{@const body = cardBodyFields(cardNode)}
								{@const footer = cardFooter(cardNode)}
								<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
								<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									data-part="kanban.card"
									class="group relative rounded-md border border-border bg-card p-3 transition-colors hover:border-foreground/20 {openTemplate
										? 'cursor-pointer'
										: ''} {draggingId === keyOf(row.id) ? 'opacity-50' : ''}"
									role="listitem"
									tabindex={openTemplate ? 0 : undefined}
									draggable={moveFn ? true : undefined}
									ondragstart={(e) => onDragStart(e, row)}
									ondragend={onDragEnd}
									onclick={() => openRow(row)}
									onkeydown={(e) => {
										if (!openTemplate) return;
										if (e.key === 'Enter' || e.key === ' ') {
											e.preventDefault();
											openRow(row);
										}
									}}
								>
									{#if actions.length > 0}
										<div class="absolute top-1.5 right-1.5 opacity-0 group-hover:opacity-100">
											<DropdownMenu.Root>
												<DropdownMenu.Trigger
													class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
													onclick={(e: MouseEvent) => e.stopPropagation()}
												>
													<EllipsisIcon class="size-3.5" />
													<span class="sr-only">Actions</span>
												</DropdownMenu.Trigger>
												<DropdownMenu.Content align="end" class="w-40">
													{#each actions as action (action.name ?? action.function)}
														<DropdownMenu.Item
															variant={action.danger ? 'destructive' : 'default'}
															onclick={(e: Event) => {
																e.stopPropagation();
																run(action, row);
															}}
														>
															{String(action.label ?? action.name ?? 'Action')}
														</DropdownMenu.Item>
													{/each}
												</DropdownMenu.Content>
											</DropdownMenu.Root>
										</div>
									{/if}

									{#if body.length > 0}
										<div class="space-y-1.5 pr-6">
											{#each body as field, fi (fieldName(field) || fi)}
												{@const name = fieldName(field)}
												{@const kind = fieldKind(field)}
												{@const value = named(name, row[name])}
												{#if fi === 0}
													<p class="text-sm font-medium text-foreground">
														{formatCell(value, kind)}
													</p>
												{:else if kind === 'badge' || field.type === 'badge'}
													<Badge variant="secondary">{formatCell(value, kind)}</Badge>
												{:else if kind === 'avatar' || field.type === 'avatar'}
													<span
														class="inline-flex size-6 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground"
														title={formatCell(value, '')}
													>
														{initialsOf(value)}
													</span>
												{:else}
													<p class="text-xs text-muted-foreground">{formatCell(value, kind)}</p>
												{/if}
											{/each}
										</div>
									{:else}
										<p class="pr-6 text-sm font-medium text-foreground">
											{formatCell(named(titleField, row[titleField]), '')}
										</p>
									{/if}

									{#if footer && (footer.children?.length ?? 0) > 0}
										<div
											class="mt-2 flex flex-wrap items-center gap-1.5 border-t border-border pt-2"
										>
											{#each footer.children ?? [] as field, fi (fieldName(field) || fi)}
												{@const name = fieldName(field)}
												{@const kind = fieldKind(field)}
												{@const value = named(name, row[name])}
												{#if kind === 'badge' || field.type === 'badge'}
													<Badge variant="outline" class="text-xs">{formatCell(value, kind)}</Badge>
												{:else if kind === 'avatar' || field.type === 'avatar'}
													<span
														class="inline-flex size-6 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground"
														title={formatCell(value, '')}
													>
														{initialsOf(value)}
													</span>
												{:else}
													<span class="text-xs text-muted-foreground"
														>{formatCell(value, kind)}</span
													>
												{/if}
											{/each}
										</div>
									{/if}
								</div>
							{/each}

							{#if createFn}
								{#if creatingFor === col.key}
									<form
										class="space-y-2 rounded-md border border-dashed border-border bg-card p-2"
										onsubmit={(e) => {
											e.preventDefault();
											void submitCreate(col.key);
										}}
									>
										<Input
											bind:value={createTitle}
											placeholder="Title"
											aria-label="Quick create title"
											disabled={createBusy}
										/>
										<div class="flex gap-1">
											<Button type="submit" size="sm" disabled={createBusy || !createTitle.trim()}
												>Add</Button
											>
											<Button
												type="button"
												size="sm"
												variant="ghost"
												disabled={createBusy}
												onclick={() => {
													creatingFor = null;
													createTitle = '';
												}}>Cancel</Button
											>
										</div>
									</form>
								{:else}
									<Button
										variant="ghost"
										size="sm"
										class="justify-start text-muted-foreground"
										onclick={() => {
											creatingFor = col.key;
											createTitle = '';
										}}
									>
										<PlusIcon class="size-3.5" />
										Add
									</Button>
								{/if}
							{/if}
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
{/if}

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
