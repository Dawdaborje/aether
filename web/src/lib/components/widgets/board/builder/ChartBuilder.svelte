<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { goto } from '$app/navigation';
	import { untrack } from 'svelte';
	import { SearchPicker, type SearchPickerItem } from '$lib/components/ui/search-picker';
	import { Button } from '$lib/components/ui/button';
	import { navHref } from '$lib/components/layout/nav';
	import { getPageState, keyOf, rowsOf, text } from '$lib/pages/pageContext.svelte';
	import type { PageNode } from '$lib/dsl/types';
	import BoardCard from '../BoardCard.svelte';
	import type { Card } from '../types';
	import {
		GRAINS,
		KINDS,
		OPERATORS,
		defaultName,
		formatDimensions,
		formatFilter,
		formatMeasures,
		kindInfo,
		measureExpression,
		parseDimensions,
		parseFilter,
		parseMeasures,
		problems,
		type Condition,
		type Dim,
		type Filter,
		type Measure
	} from './spec';

	let { node }: { node: PageNode } = $props();

	interface Field {
		name: string;
		label: string;
		kind: string;
		options: { value: string; label: string }[];
		groupable: boolean;
		numeric: boolean;
		temporal: boolean;
	}

	const page = getPageState();
	const fn = (attr: unknown, fallback: string) => text(attr) ?? fallback;
	const FN = $derived({
		get: fn(node.get, 'get_chart'),
		create: fn(node.create, 'create_chart'),
		save: fn(node.save, 'set_chart'),
		run: fn(node.run, 'run_chart'),
		sources: fn(node.sources, 'search_sources'),
		fields: fn(node.fields, 'get_source'),
		values: fn(node.values, 'field_values'),
		workbooks: fn(node.workbooks, 'search_workbooks')
	});

	const input =
		'h-9 w-full min-w-0 rounded-md border border-input bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring';
	const label = 'mb-1 block text-xs font-medium text-muted-foreground';
	const PERIODS: [string, string][] = [
		['none', 'All time'],
		['today', 'Today'],
		['this_week', 'This week'],
		['this_month', 'This month'],
		['this_quarter', 'This quarter'],
		['this_year', 'This year'],
		['last_7_days', 'Last 7 days'],
		['last_30_days', 'Last 30 days'],
		['last_90_days', 'Last 90 days']
	];
	const FUNCTIONS: [Measure['fn'], string][] = [
		['count', 'Number of records'],
		['sum', 'Sum of'],
		['avg', 'Average of'],
		['min', 'Smallest'],
		['max', 'Largest'],
		['custom', 'Formula']
	];

	// ---- what is being built ----
	const chartId = $derived(page?.params.id ? keyOf(page.params.id) : '');
	let loading = $state(true);
	let workbook = $state<string | null>(null);
	let name = $state('');
	let folder = $state('');
	let kind = $state('bar');
	let source = $state<string | null>(null);
	let dims = $state<Dim[]>([]);
	let measures = $state<Measure[]>([]);
	let filter = $state<Filter>({ conditions: [], raw: null });
	let sortBy = $state('');
	let sortDescending = $state(true);
	let rowLimit = $state(12);
	let dateField = $state('');
	let period = $state('none');
	let compare = $state(false);
	let target = $state('');
	let lowerIsBetter = $state(false);
	let guide = $state('none');
	let format = $state('number');
	let currency = $state('');
	let description = $state('');
	let info = $state('');

	// ---- what pickers offer ----
	let sourceItems = $state<SearchPickerItem[]>([]);
	let workbookItems = $state<SearchPickerItem[]>([]);
	let fields = $state<Field[]>([]);
	let fieldsError = $state<string | null>(null);
	let loadedValues = $state<Record<string, { value: string; label: string }[]>>({});

	const fieldOf = (fieldName: string) => fields.find((f) => f.name === fieldName);
	const groupable = $derived(fields.filter((f) => f.groupable));
	const numeric = $derived(fields.filter((f) => f.numeric));
	const temporal = $derived(fields.filter((f) => f.temporal));
	const kinds = $derived(Object.fromEntries(fields.map((f) => [f.name, f.kind])));
	const info_ = $derived(kindInfo(kind));
	const missing = $derived(problems(kind, dims, measures));
	const sortChoices = $derived([
		...measures
			.filter((m) => m.name && measureExpression(m))
			.map((m) => ({ value: m.name, label: m.name.replaceAll('_', ' ') })),
		...dims
			.filter((d) => d.field)
			.map((d) => ({ value: d.field, label: fieldOf(d.field)?.label ?? d.field }))
	]);

	const spec = $derived({
		name: name || 'Preview',
		kind,
		source: source ?? '',
		dimensions: formatDimensions(dims),
		measures: formatMeasures(measures),
		filter: formatFilter(filter, kinds),
		sort: sortBy ? `${sortDescending ? '-' : ''}${sortBy}` : '',
		row_limit: Number(rowLimit) || 12,
		date_field: dateField,
		period,
		compare,
		target: target.trim() === '' || Number.isNaN(Number(target)) ? null : Number(target),
		lower_is_better: lowerIsBetter,
		guide,
		format,
		currency,
		description,
		info
	});

	// ---- loading ----
	async function loadLists() {
		if (!page) return;
		const [s, w] = await Promise.all([
			page.call(FN.sources, {}),
			page.call(FN.workbooks, { limit: 200 })
		]);
		if (s.ok) {
			sourceItems = rowsOf(s.data).map((r) => ({
				id: String(r.id),
				label: String(r.name ?? r.id)
			}));
		}
		if (w.ok) {
			workbookItems = rowsOf(w.data).map((r) => ({
				id: keyOf(r.id),
				label: String(r.name ?? r.id)
			}));
		}
	}

	async function loadChart() {
		if (!page || !chartId) {
			workbook = page?.query.workbook ? keyOf(page.query.workbook) : null;
			loading = false;
			return;
		}
		const result = await page.call(FN.get, { id: chartId });
		loading = false;
		if (!result.ok || !result.data || typeof result.data !== 'object') return;
		const c = result.data as Record<string, unknown>;
		const str = (key: string, fallback = '') =>
			typeof c[key] === 'string' ? (c[key] as string) : fallback;
		workbook = c.workbook ? keyOf(c.workbook) : null;
		name = str('name');
		folder = str('folder');
		kind = str('kind', 'bar');
		source = str('source') || null;
		dims = parseDimensions(str('dimensions'));
		measures = parseMeasures(str('measures'));
		filter = parseFilter(str('filter'));
		const sort = str('sort');
		sortBy = sort.replace(/^-/, '');
		sortDescending = sort === '' ? true : sort.startsWith('-');
		rowLimit = typeof c.row_limit === 'number' ? c.row_limit : 12;
		dateField = str('date_field');
		period = str('period', 'none');
		compare = c.compare === true;
		target = typeof c.target === 'number' ? String(c.target) : '';
		lowerIsBetter = c.lower_is_better === true;
		guide = str('guide', 'none');
		format = str('format', 'number');
		currency = str('currency');
		description = str('description');
		info = str('info');
	}

	$effect(() => {
		void page?.plugin;
		untrack(() => {
			void loadLists();
			void loadChart();
		});
	});

	let fieldsFor = '';
	$effect(() => {
		const current = source;
		if (!current || !page) {
			fields = [];
			return;
		}
		untrack(async () => {
			fieldsFor = current;
			const result = await page.call(FN.fields, { source: current });
			if (fieldsFor !== current) return;
			if (result.ok && result.data && typeof result.data === 'object') {
				fieldsError = null;
				fields = ((result.data as { fields?: Field[] }).fields ?? []).map((f) => ({
					...f,
					options: f.options ?? []
				}));
				loadedValues = {};
			} else {
				fields = [];
				fieldsError = result.error ?? 'The fields of this data could not be loaded.';
			}
		});
	});

	/** The values a link field takes, for the dropdown of a condition on it. */
	async function valuesOf(fieldName: string) {
		if (!page || !source || loadedValues[fieldName]) return;
		loadedValues[fieldName] = [];
		const result = await page.call(FN.values, { source, field: fieldName, limit: 100 });
		if (result.ok) {
			loadedValues[fieldName] = rowsOf(result.data).map((r) => ({
				value: String(r.value),
				label: String(r.label ?? r.value)
			}));
		}
	}

	// ---- preview ----
	let card = $state<Card | null>(null);
	let previewError = $state<string | null>(null);
	let previewing = $state(false);
	let latest = 0;
	let timer: ReturnType<typeof setTimeout> | undefined;

	$effect(() => {
		const payload = $state.snapshot(spec);
		const ready = payload.source !== '' && missing.length === 0 && !loading;
		clearTimeout(timer);
		if (!ready || !page) {
			card = null;
			previewError = null;
			previewing = false;
			return;
		}
		previewing = true;
		timer = setTimeout(async () => {
			const current = ++latest;
			const result = await page.call(FN.run, { spec: payload });
			if (current !== latest) return;
			previewing = false;
			if (result.ok) {
				card = result.data as Card;
				previewError = null;
			} else {
				card = null;
				previewError = result.error ?? 'The chart could not be drawn.';
			}
		}, 500);
		return () => clearTimeout(timer);
	});

	// ---- editing the rows ----
	function setKind(next: string) {
		kind = next;
		const wanted = kindInfo(next);
		if (dims.length > wanted.dims[1]) dims = dims.slice(0, wanted.dims[1]);
		if (measures.length > wanted.measures[1]) measures = measures.slice(0, wanted.measures[1]);
		while (dims.length < wanted.dims[0]) dims.push({ field: '', grain: '' });
		while (measures.length < wanted.measures[0]) addMeasure('count');
		if (guide !== 'none' && !['bar', 'line', 'area'].includes(next)) guide = 'none';
	}
	function addDim() {
		dims.push({ field: '', grain: '' });
	}
	function pickDim(dim: Dim, field: string) {
		dim.field = field;
		if (!fieldOf(field)?.temporal) dim.grain = '';
		else if (!dim.grain) dim.grain = 'month';
		if (!dateField && fieldOf(field)?.temporal) dateField = field;
	}
	function addMeasure(fnName: Measure['fn'] = 'sum') {
		const taken = measures.map((m) => m.name);
		measures.push({ name: defaultName(fnName, '', taken), fn: fnName, field: '', expr: '' });
	}
	function changeMeasure(measure: Measure) {
		// A name still made by the builder follows the choice; one the person typed stays.
		const others = measures.filter((m) => m !== measure).map((m) => m.name);
		const auto = /^(records|value|total_|average_|lowest_|highest_)/.test(measure.name);
		if (auto) measure.name = defaultName(measure.fn, measure.field, others);
		if (measure.fn === 'custom' && !measure.expr) measure.expr = 'count';
	}
	function addCondition() {
		filter.conditions.push({ field: '', op: 'eq', value: '' });
	}
	function pickConditionField(condition: Condition, field: string) {
		condition.field = field;
		condition.value = fieldOf(field)?.kind === 'bool' ? 'true' : '';
		if (fieldOf(field)?.kind === 'link') void valuesOf(field);
	}
	function pickOperator(condition: Condition, op: string) {
		condition.op = op as Condition['op'];
		condition.value = op === 'null' ? 'true' : '';
	}
	const choicesFor = (condition: Condition) => {
		const f = fieldOf(condition.field);
		if (!f || !['eq', 'ne'].includes(condition.op)) return null;
		if (f.kind === 'select') return f.options;
		if (f.kind === 'bool')
			return [
				{ value: 'true', label: 'Yes' },
				{ value: 'false', label: 'No' }
			];
		if (f.kind === 'link') return loadedValues[f.name] ?? [];
		return null;
	};
	const inputType = (condition: Condition) => {
		const f = fieldOf(condition.field);
		if (f?.numeric) return 'number';
		if (f?.kind === 'date') return 'date';
		return 'text';
	};

	// ---- saving ----
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let saved = $state(false);
	const canSave = $derived(
		Boolean(name.trim() && workbook && source && missing.length === 0 && !saving)
	);

	async function save() {
		if (!page || !canSave) return;
		saving = true;
		saveError = null;
		saved = false;
		const body = { ...$state.snapshot(spec), name: name.trim(), folder: folder.trim(), workbook };
		const result = chartId
			? await page.call(FN.save, { id: chartId, ...body })
			: await page.call(FN.create, body);
		saving = false;
		if (!result.ok) {
			saveError = result.error ?? 'The chart could not be saved.';
			return;
		}
		saved = true;
		if (!chartId) {
			const row = result.data as Record<string, unknown> | null;
			const target = text(node.redirect) ?? (row?.id ? `/prism/charts/${keyOf(row.id)}` : '/prism');
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies the base path
			await goto(navHref(target.replace('{id}', keyOf(row?.id))));
		} else {
			page.refresh();
		}
	}
</script>

<div class="grid gap-6 lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]" data-part="chart-builder">
	<div class="space-y-5">
		<section class="space-y-3">
			<div>
				<span class={label}>Name</span>
				<input class={input} bind:value={name} placeholder="Deals by stage" aria-label="Name" />
			</div>
			<div class="grid gap-3 sm:grid-cols-2">
				<div>
					<span class={label}>Workbook</span>
					<SearchPicker
						items={workbookItems}
						bind:value={workbook}
						placeholder="Pick a workbook"
						entityLabel="workbooks"
					/>
				</div>
				<div>
					<span class={label}>Folder (optional)</span>
					<input class={input} bind:value={folder} placeholder="Sales" aria-label="Folder" />
				</div>
			</div>
		</section>

		<section>
			<span class={label}>Data</span>
			<SearchPicker
				items={sourceItems}
				bind:value={source}
				placeholder="Pick the records to count or add up"
				searchPlaceholder="Search the data you can read…"
				entityLabel="data sources"
			/>
			{#if fieldsError}
				<p class="mt-1 text-xs text-destructive" role="alert">{fieldsError}</p>
			{/if}
		</section>

		<section>
			<span class={label}>Chart type</span>
			<div
				class="grid grid-cols-3 gap-1.5 sm:grid-cols-5"
				role="radiogroup"
				aria-label="Chart type"
			>
				{#each KINDS as option (option.value)}
					<button
						type="button"
						role="radio"
						aria-checked={kind === option.value}
						class="rounded-md border px-2 py-1.5 text-xs transition-colors {kind === option.value
							? 'border-primary bg-primary/10 font-medium'
							: 'border-border hover:bg-muted'}"
						onclick={() => setKind(option.value)}>{option.label}</button
					>
				{/each}
			</div>
			<p class="mt-1.5 text-xs text-muted-foreground">{info_.hint}</p>
		</section>

		{#if source && fields.length > 0}
			{#if info_.dims[1] > 0}
				<section class="space-y-2">
					<span class={label}>Group by</span>
					{#each dims as dim, i (i)}
						<div class="flex items-center gap-2">
							<span class="w-24 shrink-0 text-xs text-muted-foreground"
								>{info_.dimLabels[i] ?? 'Then'}</span
							>
							<select
								class={input}
								value={dim.field}
								aria-label={info_.dimLabels[i] ?? 'Group by'}
								onchange={(e) => pickDim(dim, e.currentTarget.value)}
							>
								<option value="">Pick a field…</option>
								{#each groupable as f (f.name)}
									<option value={f.name}>{f.label}</option>
								{/each}
							</select>
							{#if fieldOf(dim.field)?.temporal}
								<select class="{input} w-32" bind:value={dim.grain} aria-label="Group dates by">
									{#each GRAINS as grain (grain)}
										<option value={grain}>by {grain}</option>
									{/each}
								</select>
							{/if}
							{#if dims.length > info_.dims[0]}
								<button
									type="button"
									class="text-muted-foreground hover:text-foreground"
									aria-label="Remove"
									onclick={() => dims.splice(i, 1)}
								>
									<AppIcon name="close" class="size-4" />
								</button>
							{/if}
						</div>
					{/each}
					{#if dims.length < info_.dims[1]}
						<Button type="button" variant="ghost" size="sm" onclick={addDim}
							><AppIcon name="plus" class="size-4" /> Add a field</Button
						>
					{/if}
				</section>
			{/if}

			<section class="space-y-2">
				<span class={label}>Measures</span>
				{#each measures as measure, i (i)}
					<div class="flex flex-wrap items-center gap-2 rounded-md border border-border p-2">
						<span class="w-14 shrink-0 text-xs text-muted-foreground"
							>{info_.measureLabels?.[i] ?? ''}</span
						>
						<select
							class="{input} w-40"
							bind:value={measure.fn}
							onchange={() => changeMeasure(measure)}
							aria-label="Calculation"
						>
							{#each FUNCTIONS as [value, text_] (value)}
								<option {value}>{text_}</option>
							{/each}
						</select>
						{#if measure.fn === 'custom'}
							<input
								class="{input} min-w-40 flex-1"
								bind:value={measure.expr}
								placeholder="ratio(won, deals)"
								aria-label="Formula"
							/>
						{:else if measure.fn !== 'count'}
							<select
								class="{input} min-w-40 flex-1"
								bind:value={measure.field}
								onchange={() => changeMeasure(measure)}
								aria-label="Field"
							>
								<option value="">Pick a field…</option>
								{#each measure.fn === 'min' || measure.fn === 'max' ? [...numeric, ...temporal] : numeric as f (f.name)}
									<option value={f.name}>{f.label}</option>
								{/each}
							</select>
						{/if}
						<input
							class="{input} w-36"
							bind:value={measure.name}
							aria-label="Shown as"
							placeholder="Shown as"
						/>
						{#if measures.length > info_.measures[0]}
							<button
								type="button"
								class="text-muted-foreground hover:text-foreground"
								aria-label="Remove"
								onclick={() => measures.splice(i, 1)}
							>
								<AppIcon name="close" class="size-4" />
							</button>
						{/if}
					</div>
				{/each}
				{#if measures.length < info_.measures[1]}
					<Button type="button" variant="ghost" size="sm" onclick={() => addMeasure('sum')}
						><AppIcon name="plus" class="size-4" /> Add a measure</Button
					>
				{/if}
			</section>

			<section class="space-y-2">
				<span class={label}>Only records where</span>
				{#if filter.raw !== null}
					<p class="text-xs text-muted-foreground">
						This filter has more in it than rows can show, so it is kept as written.
					</p>
					<textarea
						class="{input} h-24 py-1.5 font-mono text-xs"
						bind:value={filter.raw}
						aria-label="Filter as JSON"
					></textarea>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onclick={() => (filter = { conditions: [], raw: null })}>Start again with rows</Button
					>
				{:else}
					{#each filter.conditions as condition, i (i)}
						{@const choices = choicesFor(condition)}
						<div class="flex flex-wrap items-center gap-2">
							<select
								class="{input} w-40"
								value={condition.field}
								onchange={(e) => pickConditionField(condition, e.currentTarget.value)}
								aria-label="Field"
							>
								<option value="">Field…</option>
								{#each fields as f (f.name)}
									<option value={f.name}>{f.label}</option>
								{/each}
							</select>
							<select
								class="{input} w-40"
								value={condition.op}
								onchange={(e) => pickOperator(condition, e.currentTarget.value)}
								aria-label="Condition"
							>
								{#each OPERATORS as o (o.op)}
									<option value={o.op}>{o.label}</option>
								{/each}
							</select>
							{#if condition.op === 'null'}
								<select class="{input} w-32" bind:value={condition.value} aria-label="Empty or not">
									<option value="true">Empty</option>
									<option value="false">Has a value</option>
								</select>
							{:else if choices}
								<select
									class="{input} min-w-32 flex-1"
									bind:value={condition.value}
									aria-label="Value"
								>
									<option value="">Pick…</option>
									{#each choices as choice (choice.value)}
										<option value={choice.value}>{choice.label}</option>
									{/each}
								</select>
							{:else}
								<input
									class="{input} min-w-32 flex-1"
									type={inputType(condition)}
									bind:value={condition.value}
									aria-label="Value"
								/>
							{/if}
							<button
								type="button"
								class="text-muted-foreground hover:text-foreground"
								aria-label="Remove"
								onclick={() => filter.conditions.splice(i, 1)}
							>
								<AppIcon name="close" class="size-4" />
							</button>
						</div>
					{/each}
					<Button type="button" variant="ghost" size="sm" onclick={addCondition}
						><AppIcon name="plus" class="size-4" /> Add a condition</Button
					>
				{/if}
			</section>

			<section class="grid gap-3 sm:grid-cols-2">
				<div>
					<span class={label}>Sort by</span>
					<div class="flex items-center gap-2">
						<select class={input} bind:value={sortBy} aria-label="Sort by">
							<option value="">Automatic</option>
							{#each sortChoices as choice (choice.value)}
								<option value={choice.value}>{choice.label}</option>
							{/each}
						</select>
						{#if sortBy}
							<label class="flex shrink-0 items-center gap-1 text-xs"
								><input type="checkbox" bind:checked={sortDescending} /> Largest first</label
							>
						{/if}
					</div>
				</div>
				<div>
					<span class={label}>Rows to show</span>
					<input
						class={input}
						type="number"
						min="1"
						max="1000"
						bind:value={rowLimit}
						aria-label="Rows to show"
					/>
				</div>
			</section>

			<section class="space-y-3">
				<span class={label}>Time and goal</span>
				<div class="grid gap-3 sm:grid-cols-2">
					<div>
						<span class={label}>Date the period applies to</span>
						<select class={input} bind:value={dateField} aria-label="Date field">
							<option value="">None</option>
							{#each temporal as f (f.name)}
								<option value={f.name}>{f.label}</option>
							{/each}
						</select>
					</div>
					<div>
						<span class={label}>Period</span>
						<select class={input} bind:value={period} aria-label="Period">
							{#each PERIODS as [value, text_] (value)}
								<option {value}>{text_}</option>
							{/each}
						</select>
					</div>
				</div>
				<div class="grid gap-3 sm:grid-cols-3">
					<div>
						<span class={label}>Goal</span>
						<input class={input} type="number" bind:value={target} aria-label="Goal" />
					</div>
					<div>
						<span class={label}>Format</span>
						<select class={input} bind:value={format} aria-label="Format">
							<option value="number">Number</option>
							<option value="compact">Compact (1.2k)</option>
							<option value="currency">Currency</option>
							<option value="percent">Percent</option>
							<option value="days">Days</option>
						</select>
					</div>
					{#if format === 'currency'}
						<div>
							<span class={label}>Currency code</span>
							<input
								class={input}
								maxlength="8"
								bind:value={currency}
								placeholder="USD"
								aria-label="Currency code"
							/>
						</div>
					{/if}
					{#if ['bar', 'line', 'area'].includes(kind)}
						<div>
							<span class={label}>Guide line</span>
							<select class={input} bind:value={guide} aria-label="Guide line">
								<option value="none">None</option>
								<option value="average">Average</option>
								<option value="median">Median</option>
								<option value="trend">Trend</option>
							</select>
						</div>
					{/if}
				</div>
				<div class="flex flex-wrap gap-4 text-sm">
					<label class="flex items-center gap-2"
						><input type="checkbox" bind:checked={compare} /> Compare with the period before</label
					>
					<label class="flex items-center gap-2"
						><input type="checkbox" bind:checked={lowerIsBetter} /> Lower is better</label
					>
				</div>
			</section>

			<section class="space-y-3">
				<div>
					<span class={label}>Description</span>
					<input class={input} bind:value={description} maxlength="300" aria-label="Description" />
				</div>
				<div>
					<span class={label}>Definition and caveats</span>
					<textarea
						class="{input} h-20 py-1.5"
						bind:value={info}
						aria-label="Definition and caveats"
					></textarea>
				</div>
			</section>
		{/if}

		<div class="flex flex-wrap items-center gap-3 border-t border-border pt-4">
			<Button type="button" onclick={save} disabled={!canSave}
				>{chartId ? 'Save' : 'Create chart'}</Button
			>
			{#if saved && chartId}<span class="text-sm text-muted-foreground">Saved</span>{/if}
			{#if saveError}<span class="text-sm text-destructive" role="alert">{saveError}</span>{/if}
			{#if !canSave && !saving}
				<span class="text-xs text-muted-foreground">
					{#if !name.trim()}Give the chart a name.{:else if !workbook}Pick a workbook.{:else if !source}Pick
						the data.{:else if missing.length}{missing[0]}{/if}
				</span>
			{/if}
		</div>
	</div>

	<div class="lg:sticky lg:top-4 lg:self-start">
		<span class={label}>Preview {previewing ? '…' : ''}</span>
		{#if card}
			<BoardCard item={{ id: 'preview', card }} />
		{:else if previewError}
			<div
				class="rounded-lg border border-border bg-card p-4 text-sm text-destructive"
				role="alert"
			>
				{previewError}
			</div>
		{:else}
			<div
				class="flex h-56 items-center justify-center rounded-lg border border-dashed border-border p-4 text-center text-sm text-muted-foreground"
			>
				{#if !source}Pick the data to start.{:else if missing.length}{missing[0]}{:else}Drawing the
					chart…{/if}
			</div>
		{/if}
	</div>
</div>
