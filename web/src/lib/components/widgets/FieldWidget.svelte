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
	import MarkdownBlocks from '$lib/media/viewers/MarkdownBlocks.svelte';
	import { parseMarkdown } from '$lib/media/viewers/markdown';
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
	/** A many2many with a `source`: chips for what is picked and a search box to add more. */
	const isMany = $derived(!isLink && resolved.fieldType === 'many2many' && Boolean(linkSource));
	const name = $derived(typeof node.name === 'string' ? node.name : '');
	/** An email address is never typed in capitals: `email`, `work_email`, `private_email`, or `fieldType="email"`. */
	const isEmail = $derived(String(node.fieldType) === 'email' || /(^|_)email$/.test(name));
	const label = $derived(resolved.label);
	const required = $derived(resolved.required);
	/** `readonly="true"` on the page: the value is shown, not edited. */
	const readonly = $derived(
		node.readonly === true || node.readonly === 'true' || matches(attrText(node.readonlyWhen))
	);
	/** `showWhen="field=value"`: the field is drawn only while the condition holds. */
	const shown = $derived(!attrText(node.showWhen) || matches(attrText(node.showWhen)));
	/**
	 * A condition on another field of the form: `a=b`, `a!=b`, several separated by commas
	 * (any one holding is enough).
	 */
	function matches(rule: string | undefined | null): boolean {
		if (!rule || !form) return false;
		return rule.split(',').some((part) => {
			const m = /^\s*([\w.]+)\s*(!=|=)\s*(.*?)\s*$/.exec(part);
			if (!m) return false;
			const actual = String(form.values[m[1]] ?? '').trim();
			return m[2] === '=' ? actual === m[3] : actual !== m[3];
		});
	}
	/**
	 * `min` / `max` on a date: a date (`2012-12-01`) or the name of another field of the form that holds
	 * one, so a form can say what the record allows (a birth date no later than a given day).
	 */
	function bound(attr: string | undefined | null): string | undefined {
		if (!attr) return undefined;
		if (/^\d{4}-\d{2}-\d{2}/.test(attr)) return attr;
		const value = String(form?.values[attr] ?? '').trim();
		return value || undefined;
	}
	const minDate = $derived(bound(attrText(node.min)));
	const maxDate = $derived(bound(attrText(node.max)));
	const help = $derived(resolved.help);
	const options = $derived(resolved.options);
	const step = $derived(numberStep(resolved));
	const optionLabelField = $derived(
		attrText(node.optionLabel) ?? attrText(node.option_label) ?? ''
	);
	const entityLabel = $derived(attrText(node.entity) ?? label.toLowerCase() + 's');
	const creatable = $derived(node.creatable === true || node.creatable === 'true');
	const createFn = $derived(attrText(node.create) ?? '');
	/** Other form fields (`depends="job,grade"`) whose values are sent to `source` and reload this picker when they change. */
	const dependsOn = $derived(attrText(node.depends) ?? '');
	const dependsFields = $derived(
		dependsOn
			.split(',')
			.map((part) => part.trim())
			.filter(Boolean)
	);

	// Inside a form the field edits the form's values; elsewhere it only holds its own.
	const form = getFormState();
	const page = getPageState();
	let own = $state<unknown>('');
	let linkItems = $state<SearchPickerItem[]>([]);
	/** The rows the link source answered, by id: `fills` copies from the chosen one. */
	let linkRows: Record<string, Record<string, unknown>> = {};
	let linkLoading = $state(false);
	let linkCreating = $state(false);
	const id = $props.id();
	const dependsValue = $derived(
		dependsFields.length > 0 && form
			? dependsFields.map((field) => String(form.values[field] ?? '').trim()).join('\u0000')
			: ''
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

	/** On a file field: the plugin whose guard the uploads go under, and whether several files may be attached. */
	const guardPlugin = $derived(attrText(node.guard) ?? undefined);
	const manyFiles = $derived(
		resolved.multiple || node.multiple === true || node.multiple === 'true'
	);
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
		if (!(isLink || isMany) || !linkSource || !page) {
			linkItems = [];
			return;
		}
		void dependsValue;
		let cancelled = false;
		linkLoading = true;
		const payload: Record<string, unknown> = { ...page.params, limit: 500 };
		for (const field of dependsFields) payload[field] = String(form?.values[field] ?? '').trim();
		// Failures open the shared error modal via `callPlugin` → `pageError`.
		void page.call(linkSource, payload).then((result) => {
			if (cancelled) return;
			linkLoading = false;
			if (!result.ok) {
				linkItems = [];
				return;
			}
			const rows = rowsOf(result.data);
			linkRows = Object.fromEntries(rows.map((row) => [String(row.id ?? ''), row]));
			const items = rows
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

	/**
	 * `fills="currency_code"` (names separated by commas): choosing a row copies those values from it into the
	 * form's fields of the same names, so picking a country also sets its currency. A value the row lacks is kept.
	 */
	const fills = $derived(
		(attrText(node.fills) ?? '')
			.split(',')
			.map((part) => part.trim())
			.filter(Boolean)
	);
	function fillFrom(chosen: string) {
		if (!form || !chosen || fills.length === 0) return;
		const row = linkRows[chosen];
		if (!row) return;
		for (const field of fills) {
			const next = row[field];
			if (next !== undefined && next !== null && next !== '') form.values[field] = next;
		}
	}

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

{#if shown}
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
						multiple={manyFiles}
						guard={guardPlugin}
						{readonly}
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
	{:else if fieldType === 'markdown'}
		<Field data-part="form.field" data-field={name}>
			<FieldLabel class="text-xs font-medium text-muted-foreground">{label}</FieldLabel>
			<FieldContent>
				<div class="text-sm" data-part="form.markdown">
					<MarkdownBlocks blocks={parseMarkdown(String(value.current ?? ''))} />
				</div>
			</FieldContent>
		</Field>
	{:else if fieldType === 'phone'}
		<PhoneField {id} {name} {label} {required} {help} {countryField} />
	{:else if fieldType === 'boolean'}
		<label class="flex items-center gap-2.5 text-sm" data-part="form.field" data-field={name}>
			<Switch
				disabled={readonly}
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
	{:else if isMany}
		{@const chosen = Array.isArray(value.current)
			? (value.current as unknown[]).map((v) => String(v))
			: []}
		<Field data-part="form.field" data-field={name}>
			<FieldLabel class="text-xs font-medium text-muted-foreground">{label}</FieldLabel>
			<FieldContent>
				<div class="flex flex-wrap gap-1.5" data-part="form.many2many">
					{#each chosen as item (item)}
						{@const known = linkItems.find((candidate) => sameId(candidate.id, item))}
						<Badge variant="secondary" class="gap-1">
							{known?.label ?? (linkLoading ? '…' : keyOf(item))}
							{#if !readonly}
								<button
									type="button"
									class="ml-0.5 rounded-sm text-muted-foreground hover:text-foreground"
									aria-label="Remove {known?.label ?? item}"
									onclick={() => (value.current = chosen.filter((other) => other !== item))}
									>×</button
								>
							{/if}
						</Badge>
					{:else}
						<span class="text-sm text-muted-foreground">None</span>
					{/each}
				</div>
				{#if !readonly}
					{#key chosen.length}
						<div class="mt-1.5">
							<SearchPicker
								items={linkItems.filter(
									(candidate) => !chosen.some((item) => sameId(candidate.id, item))
								)}
								value={null}
								placeholder={linkLoading ? 'Loading…' : `Add ${entityLabel}…`}
								searchPlaceholder={`Search ${entityLabel}…`}
								{entityLabel}
								clearable={false}
								onchange={(next) => {
									if (next) value.current = [...chosen, next];
								}}
							/>
						</div>
					{/key}
				{/if}
				{#if help}
					<FieldDescription>{help}</FieldDescription>
				{/if}
			</FieldContent>
		</Field>
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
					<div inert={readonly} class={readonly ? 'opacity-50' : ''}>
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
								fillFrom(next);
							}}
							onCreate={createLink}
						/>
					</div>
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
						disabled={readonly}
						{required}
						placeholder={label}
						value={String(value.current)}
						oninput={(event) => (value.current = event.currentTarget.value)}
					/>
				{:else if fieldType === 'selection'}
					<Select
						{id}
						disabled={readonly}
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
						disabled={readonly}
						{required}
						type={fieldType === 'datetime' ? 'datetime' : 'date'}
						min={minDate}
						max={maxDate}
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
						disabled={readonly}
						{required}
						type={numeric || decimal ? 'number' : 'text'}
						inputmode={isEmail ? 'email' : undefined}
						{step}
						placeholder={label}
						autocapitalize={isEmail ? 'none' : undefined}
						spellcheck={isEmail ? false : undefined}
						value={String(value.current)}
						oninput={(event) => {
							const input = event.currentTarget;
							let raw = input.value;
							if (isEmail && raw !== raw.toLowerCase()) {
								// Lower-case as it is typed, keeping the cursor where it was.
								const at = input.selectionStart;
								raw = raw.toLowerCase();
								input.value = raw;
								if (at !== null) input.setSelectionRange(at, at);
							}
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
{/if}
