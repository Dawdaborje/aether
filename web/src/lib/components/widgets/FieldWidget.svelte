<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { numberStep, resolveField } from '$lib/dsl/resolveFieldType';
	import { Switch } from '$lib/components/ui/switch';
	import FileField from '$lib/media/FileField.svelte';
	import ImageField from '$lib/media/ImageField.svelte';
	import { Input } from '$lib/components/ui/input';
	import { DatePicker } from '$lib/components/ui/date-picker';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Badge } from '$lib/components/ui/badge';
	import { Select } from '$lib/components/ui/select';
	import { Field, FieldLabel, FieldContent, FieldDescription } from '$lib/components/ui/field';
	import { recordOptions, recordTitles } from '$lib/records/titles';
	import { SearchPicker, type SearchPickerItem } from '$lib/components/ui/search-picker';
	import PhoneField from '$lib/components/widgets/PhoneField.svelte';
	import {
		getFormState,
		getPageState,
		keyOf,
		rowsOf,
		text as attrText
	} from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	// What the page says, then what the model says (the server adds `model*` hints).
	const resolved = $derived(resolveField(node));
	/// `source` on many2one/link: plugin function that returns rows to pick from.
	const linkSource = $derived(attrText(node.source) ?? '');
	const declaredLink = $derived(node.fieldType === 'many2one' || node.fieldType === 'link');
	/**
	 * Related fields use SearchPicker. A link the model knows about but the page gave no
	 * `source` for stays a text box (there is nothing to pick from), rather than an error.
	 */
	/// A link the model points at a model for needs no `source`: the kernel lists that model's
	/// records by their `title_field`.
	const kernelTarget = $derived(!linkSource && resolved.target ? resolved.target : '');
	const isLink = $derived(
		declaredLink ||
			(resolved.fieldType === 'link' &&
				(Boolean(linkSource) || Boolean(kernelTarget) || !resolved.inferred))
	);
	const fieldType = $derived(isLink ? 'link' : resolved.fieldType);
	const name = $derived(typeof node.name === 'string' ? node.name : '');
	const label = $derived(resolved.label);
	const required = $derived(resolved.required);
	const help = $derived(resolved.help);
	const options = $derived(resolved.options);
	const step = $derived(numberStep(resolved));
	const optionLabelField = $derived(
		attrText(node.optionLabel) ?? attrText(node.option_label) ?? ''
	);
	const entityLabel = $derived(attrText(node.entity) ?? label.toLowerCase() + 's');
	const creatable = $derived(node.creatable === true || node.creatable === 'true');
	const createFn = $derived(attrText(node.create) ?? '');
	/** Another form field whose value is sent to `source` and reloads this picker when it changes. */
	const dependsOn = $derived(attrText(node.depends) ?? '');

	// Inside a form the field edits the form's values; elsewhere it only holds its own.
	const form = getFormState();
	const page = getPageState();
	let own = $state<unknown>('');
	let linkItems = $state<SearchPickerItem[]>([]);
	let linkLoading = $state(false);
	let linkCreating = $state(false);
	const id = $props.id();
	const dependsValue = $derived(
		dependsOn && form ? String(form.values[dependsOn] ?? '').trim() : ''
	);

	const value = {
		get current(): unknown {
			return form && name ? (form.values[name] ?? '') : own;
		},
		set current(next: unknown) {
			if (form && name) form.values[name] = next;
			else own = next;
		}
	};

	function labelOfRow(row: Record<string, unknown>): string {
		const preferred = optionLabelField ? row[optionLabelField] : undefined;
		if (typeof preferred === 'string' && preferred.trim()) return preferred;
		for (const key of ['display_name', 'name', 'title', 'label', 'code', 'employee_no'] as const) {
			const raw = row[key];
			if (typeof raw === 'string' && raw.trim()) return raw;
		}
		const key = keyOf(row.id);
		return key || 'Untitled';
	}

	function sublabelOfRow(row: Record<string, unknown>): string | undefined {
		for (const key of ['phone_prefix', 'code', 'employee_no', 'email', 'work_email'] as const) {
			const raw = row[key];
			if (typeof raw === 'string' && raw.trim() && raw !== labelOfRow(row)) return raw;
		}
		return undefined;
	}

	const countryField = $derived(attrText(node.countryField) ?? attrText(node.country_field) ?? '');

	function sameId(a: string, b: string): boolean {
		if (!a || !b) return false;
		return a === b || keyOf(a) === keyOf(b) || a === keyOf(b) || keyOf(a) === b;
	}

	const linkValue = $derived.by(() => {
		const raw = String(value.current ?? '').trim();
		if (!raw) return null;
		const match = linkItems.find((item) => sameId(item.id, raw));
		return match?.id ?? raw;
	});

	$effect(() => {
		if (isLink && !linkSource && kernelTarget && page) {
			let cancelled = false;
			linkLoading = true;
			void recordOptions(page.plugin, kernelTarget, '', 200).then(async (options) => {
				if (cancelled) return;
				let items: SearchPickerItem[] = options.map((option) => ({
					id: option.id,
					label: option.title
				}));
				// The record the field holds may not be among the first ones listed.
				const current = String(value.current ?? '');
				if (current && !items.some((item) => sameId(item.id, current))) {
					const found = await recordTitles(page.plugin, kernelTarget, [current]);
					const title = found[current];
					if (title) items = [{ id: current, label: title }, ...items];
				}
				if (cancelled) return;
				linkItems = items;
				linkLoading = false;
			});
			return () => {
				cancelled = true;
			};
		}
		if (!isLink || !linkSource || !page) {
			linkItems = [];
			return;
		}
		void dependsValue;
		let cancelled = false;
		linkLoading = true;
		const payload: Record<string, unknown> = { ...page.params, limit: 500 };
		if (dependsOn) payload[dependsOn] = dependsValue;
		// Failures open the shared error modal via `callPlugin` → `pageError`.
		void page.call(linkSource, payload).then((result) => {
			if (cancelled) return;
			linkLoading = false;
			if (!result.ok) {
				linkItems = [];
				return;
			}
			const items = rowsOf(result.data)
				.map((row) => ({
					id: String(row.id ?? ''),
					label: labelOfRow(row),
					sublabel: sublabelOfRow(row)
				}))
				.filter((item) => item.id);
			linkItems = items;
			// Dependent pickers drop a value that no longer matches the parent (e.g. branch after bank change).
			if (dependsOn) {
				const current = String(value.current ?? '').trim();
				if (current && !items.some((item) => sameId(item.id, current))) {
					value.current = '';
				}
			}
		});
		return () => {
			cancelled = true;
		};
	});

	async function createLink(query: string) {
		if (!page || !createFn || linkCreating) return;
		linkCreating = true;
		try {
			const result = await page.call(createFn, { name: query.trim() });
			if (!result.ok || !result.data || typeof result.data !== 'object') return;
			const row = result.data as Record<string, unknown>;
			const nextId = String(row.id ?? '');
			if (!nextId) return;
			const item = { id: nextId, label: labelOfRow(row), sublabel: sublabelOfRow(row) };
			if (!linkItems.some((existing) => sameId(existing.id, nextId))) {
				linkItems = [...linkItems, item];
			}
			value.current = nextId;
		} finally {
			linkCreating = false;
		}
	}
</script>

{#if fieldType === 'badge' || node.type === 'badge'}
	<Badge>{String(value.current || label)}</Badge>
{:else if fieldType === 'avatar' || node.type === 'avatar'}
	{@const text = String(value.current || label || '?')}
	{@const initials =
		text
			.trim()
			.split(/\s+/)
			.slice(0, 2)
			.map((p) => p[0]?.toUpperCase() ?? '')
			.join('') || '?'}
	<span
		class="inline-flex size-8 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground"
		title={text}
	>
		{initials}
	</span>
{:else if fieldType === 'progress'}
	{@const raw = value.current}
	{@const pct = Math.max(0, Math.min(100, typeof raw === 'number' ? raw : Number(raw) || 0))}
	<div class="h-2 w-full overflow-hidden rounded-full bg-muted" title="{pct}%">
		<div class="h-full bg-primary transition-[width]" style="width: {pct}%"></div>
	</div>
{:else if fieldType === 'image' || fieldType === 'binary' || fieldType === 'file'}
	<Field data-part="form.field" data-field={name}>
		<FieldLabel for={id} class="text-xs font-medium text-muted-foreground">
			{label}{#if required}<span class="text-destructive"> *</span>{/if}
		</FieldLabel>
		<FieldContent>
			{#if fieldType === 'file'}
				<FileField
					{id}
					value={value.current}
					multiple={resolved.multiple}
					onchange={(next) => (value.current = next)}
				/>
			{:else}
				<ImageField
					{id}
					value={value.current}
					multiple={resolved.multiple}
					avatar={node.avatar === true}
					onchange={(next) => (value.current = next)}
				/>
			{/if}
			{#if help}
				<FieldDescription>{help}</FieldDescription>
			{/if}
		</FieldContent>
	</Field>
{:else if fieldType === 'phone'}
	<PhoneField
		{id}
		{name}
		{label}
		{required}
		{help}
		countryField={countryField}
	/>
{:else if fieldType === 'boolean'}
	<label class="flex items-center gap-2.5 text-sm" data-part="form.field" data-field={name}>
		<Switch
			checked={value.current === true}
			onCheckedChange={(checked) => (value.current = checked)}
		/>
		<span>
			{label}{#if required}<span class="text-destructive"> *</span>{/if}
		</span>
	</label>
	{#if help}
		<p class="text-xs text-muted-foreground">{help}</p>
	{/if}
{:else if fieldType === 'many2many'}
	{@const picked = Array.isArray(value.current) ? (value.current as unknown[]) : []}
	<Field data-part="form.field" data-field={name}>
		<FieldLabel class="text-xs font-medium text-muted-foreground">{label}</FieldLabel>
		<FieldContent>
			<div class="flex flex-wrap gap-1.5">
				{#each picked as item (String(item))}
					<Badge variant="secondary">{keyOf(item)}</Badge>
				{:else}
					<span class="text-sm text-muted-foreground">None</span>
				{/each}
			</div>
			{#if help}
				<FieldDescription>{help}</FieldDescription>
			{/if}
		</FieldContent>
	</Field>
{:else if fieldType === 'json'}
	<Field data-part="form.field" data-field={name}>
		<FieldLabel class="text-xs font-medium text-muted-foreground">{label}</FieldLabel>
		<FieldContent>
			<pre
				class="max-h-48 overflow-auto rounded-md border border-border bg-muted p-3 font-mono text-xs">{JSON.stringify(
					value.current === '' ? null : value.current,
					null,
					2
				)}</pre>
		</FieldContent>
	</Field>
{:else if isLink}
	<Field data-part="form.field" data-field={name}>
		<FieldLabel for={id} class="text-xs font-medium text-muted-foreground">
			{label}{#if required}<span class="text-destructive"> *</span>{/if}
		</FieldLabel>
		<FieldContent>
			{#if !linkSource && !kernelTarget}
				<p class="text-xs text-destructive">
					Link field needs a <code class="text-xs">source</code> function.
				</p>
			{:else}
				<SearchPicker
					items={linkItems}
					value={linkValue}
					placeholder={linkLoading ? 'Loading…' : `Select ${label.toLowerCase()}…`}
					searchPlaceholder={`Search ${entityLabel}…`}
					{entityLabel}
					clearable={!required}
					creatable={creatable && Boolean(createFn)}
					creating={linkCreating}
					onchange={(next) => {
						value.current = next || '';
					}}
					onCreate={createLink}
				/>
			{/if}
			{#if help}
				<FieldDescription>{help}</FieldDescription>
			{/if}
		</FieldContent>
	</Field>
{:else}
	<Field data-part="form.field" data-field={name}>
		<FieldLabel for={id} class="text-xs font-medium text-muted-foreground">
			{label}{#if required}<span class="text-destructive"> *</span>{/if}
		</FieldLabel>
		<FieldContent>
			{#if fieldType === 'text'}
				<Textarea
					{id}
					rows={3}
					{required}
					placeholder={label}
					value={String(value.current)}
					oninput={(event) => (value.current = event.currentTarget.value)}
				/>
			{:else if fieldType === 'selection'}
				<Select
					{id}
					{required}
					placeholder="Select…"
					clearLabel="None"
					options={options.map((option) => ({
						value: option.value,
						// A choice written as `prefer_not_to_say` reads as "Prefer not to say".
						label:
							option.label === option.value && /^[a-z0-9_]+$/.test(option.value)
								? option.value.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase())
								: option.label
					}))}
					value={String(value.current ?? '')}
					onchange={(next) => (value.current = next)}
				/>
			{:else if fieldType === 'date' || fieldType === 'datetime'}
				<DatePicker
					{id}
					{required}
					type={fieldType === 'datetime' ? 'datetime' : 'date'}
					placeholder={label}
					value={String(value.current ?? '')}
					onchange={(next) => (value.current = next)}
				/>
			{:else}
				{@const numeric =
					fieldType === 'integer' || fieldType === 'float' || fieldType === 'currency'}
				<!-- A decimal travels as text (`"12.50"`), so editing it never rounds through a float. -->
				{@const decimal = fieldType === 'decimal'}
				<Input
					{id}
					{required}
					type={numeric || decimal ? 'number' : 'text'}
					{step}
					placeholder={label}
					value={String(value.current)}
					oninput={(event) => {
						const raw = event.currentTarget.value;
						value.current = numeric ? (raw === '' ? '' : Number(raw)) : raw;
					}}
				/>
			{/if}
			{#if help}
				<FieldDescription>{help}</FieldDescription>
			{/if}
		</FieldContent>
	</Field>
{/if}
