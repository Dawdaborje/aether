<script lang="ts">
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { NativeSelect } from '$lib/components/ui/native-select';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import {
		FIELD_TYPES,
		isValidName,
		planModel,
		saveModel,
		type ChatterDef,
		type FieldDef,
		type FieldType,
		type ModelDef,
		type OrgPlan,
		type PluginModels,
		type SaveResult,
		type VisitorChatter
	} from './api';

	let {
		plugin,
		model,
		isNew = false,
		onsaved
	}: {
		plugin: PluginModels;
		model: ModelDef;
		/** The model does not exist yet: saving creates it. */
		isNew?: boolean;
		onsaved: (result: SaveResult) => void;
	} = $props();

	/** A field being edited, with a key that survives renaming and has nothing to do with the id. */
	interface Row {
		key: number;
		open: boolean;
		def: FieldDef;
	}

	let counter = 0;
	const toRows = (fields: FieldDef[]): Row[] =>
		fields.map((field) => ({ key: counter++, open: false, def: structuredClone($state.snapshot(field)) }));

	// The page makes a new editor for each model (`{#key}`), so starting from the first values is right.
	// svelte-ignore state_referenced_locally
	let label = $state(model.label ?? '');
	// svelte-ignore state_referenced_locally
	let rows = $state<Row[]>(toRows(model.fields));
	// svelte-ignore state_referenced_locally
	let chatter = $state<ChatterDef>({
		enabled: false,
		messages: true,
		notes: true,
		followers: true,
		track_changes: true,
		visitors: 'none',
		...(model.chatter ?? {})
	});
	let applyTo = $state<string[]>([]);
	let writeFile = $state(true);
	let plans = $state<OrgPlan[] | null>(null);
	let working = $state<'preview' | 'save' | null>(null);
	let failure = $state<string | null>(null);

	const organizations = $derived(orgStore.organizations);
	const siblings = $derived(plugin.models.map((other) => other.name));
	const fileBlocked = $derived(!plugin.file_available);

	const colors = ['gray', 'teal', 'blue', 'green', 'amber', 'red'];

	/** The names fields had when the model was loaded, by id: to follow renames. */
	// svelte-ignore state_referenced_locally
	const originalNames = new Map(model.fields.filter((f) => f.id).map((f) => [f.id as string, f.name]));

	/** The view with renamed fields under their new names and hidden fields taken out. */
	function adjustView(view: unknown, rename: Map<string, string>, hidden: Set<string>): unknown {
		if (!view || typeof view !== 'object') return view;
		const v = view as { list?: string[]; form?: { section: string; columns: string[][] }[]; sort?: { field: string; dir?: string }[] };
		const name = (field: string) => rename.get(field) ?? field;
		const keep = (field: string) => !hidden.has(field);
		return {
			...(v.list ? { list: v.list.filter(keep).map(name) } : {}),
			...(v.form ? { form: v.form.map((s) => ({ ...s, columns: s.columns.map((c) => c.filter(keep).map(name)) })) } : {}),
			...(v.sort ? { sort: v.sort.filter((k) => keep(k.field)).map((k) => ({ ...k, field: name(k.field) })) } : {})
		};
	}

	/** The model as it would be saved: empty values left out. */
	function draft(): ModelDef {
		const rename = new Map<string, string>();
		const hidden = new Set<string>();
		for (const row of rows) {
			const was = row.def.id ? originalNames.get(row.def.id) : undefined;
			if (was && was !== row.def.name) rename.set(was, row.def.name);
			if (was && row.def.deprecated) hidden.add(was);
		}
		const titleField = model.title_field && !hidden.has(model.title_field) ? (rename.get(model.title_field) ?? model.title_field) : undefined;
		const clean = (field: FieldDef): FieldDef => {
			const out: FieldDef = { name: field.name, type: field.type };
			if (field.id) out.id = field.id;
			if (field.label?.trim()) out.label = field.label.trim();
			if (field.required) out.required = true;
			if (field.default !== undefined && field.default !== '') out.default = field.default;
			if (field.max_length && (field.type === 'string' || field.type === 'text')) out.max_length = field.max_length;
			if (field.index && field.type !== 'json' && field.type !== 'text') out.index = field.index;
			if (field.type === 'select') out.options = (field.options ?? []).filter((o) => o.value.trim() !== '');
			if (field.type === 'link' && field.target) out.target = field.target;
			if (field.help?.trim()) out.help = field.help.trim();
			if (field.deprecated) out.deprecated = true;
			if (field.track && chatter.enabled && field.type !== 'json') out.track = true;
			return out;
		};
		return {
			...(model.model_id ? { model_id: model.model_id } : {}),
			name: model.name,
			...(label.trim() ? { label: label.trim() } : {}),
			...(model.plural_label ? { plural_label: model.plural_label } : {}),
			...(model.icon ? { icon: model.icon } : {}),
			...(titleField ? { title_field: titleField } : {}),
			...(chatter.enabled ? { chatter: $state.snapshot(chatter) as ChatterDef } : {}),
			fields: rows.map((row) => clean($state.snapshot(row.def) as FieldDef)),
			...(model.view ? { view: adjustView(model.view, rename, hidden) } : {})
		};
	}

	const initial = JSON.stringify(draft());
	const dirty = $derived(JSON.stringify(draft()) !== initial);

	const problems = $derived.by(() => {
		const found: string[] = [];
		const seen = new Set<string>();
		for (const row of rows) {
			const name = row.def.name;
			if (!isValidName(name) || name === 'id') found.push(`“${name || '(empty)'}” is not a valid field name`);
			else if (seen.has(name)) found.push(`two fields are called “${name}”`);
			seen.add(name);
			if (row.def.type === 'select' && !(row.def.options ?? []).some((o) => o.value.trim() !== '')) {
				found.push(`“${name}” is a choice field and needs at least one option`);
			}
			if (row.def.type === 'link' && !row.def.target) found.push(`“${name}” needs a model to link to`);
		}
		if (chatter.enabled && chatter.visitors === 'read_write' && !chatter.messages) {
			found.push('visitors can only post messages, so messages must be on');
		}
		if (chatter.enabled && !chatter.messages && !chatter.notes && !chatter.track_changes) {
			found.push('chatter is on but has nothing to show: turn on messages, notes or tracked changes');
		}
		return found;
	});

	function addField() {
		rows.push({ key: counter++, open: true, def: { name: '', type: 'string' } });
	}

	function setType(row: Row, type: FieldType) {
		row.def.type = type;
		if (type === 'select' && !row.def.options?.length) row.def.options = [{ value: '' }];
		if (type !== 'select') row.def.options = undefined;
		if (type !== 'link') row.def.target = undefined;
		row.def.default = undefined;
	}

	function removeOrHide(row: Row, index: number) {
		if (row.def.id) row.def.deprecated = true;
		else rows.splice(index, 1);
	}

	function setDefault(row: Row, raw: string) {
		const type = row.def.type;
		if (raw === '') row.def.default = undefined;
		else if (type === 'int' || type === 'float') row.def.default = Number(raw);
		else if (type === 'bool') row.def.default = raw === 'true';
		else row.def.default = raw;
	}

	function toggleOrganization(db: string) {
		applyTo = applyTo.includes(db) ? applyTo.filter((item) => item !== db) : [...applyTo, db];
		plans = null;
	}

	async function preview() {
		working = 'preview';
		failure = null;
		try {
			plans = await planModel(plugin.name, draft(), applyTo);
		} catch (error) {
			failure = error instanceof Error ? error.message : 'Could not preview the change';
		} finally {
			working = null;
		}
	}

	async function save() {
		working = 'save';
		failure = null;
		try {
			onsaved(await saveModel(plugin.name, draft(), { writeFile: writeFile && !fileBlocked, applyTo }));
		} catch (error) {
			failure = error instanceof Error ? error.message : 'Could not save the model';
		} finally {
			working = null;
		}
	}

	const describe = (change: OrgPlan['changes'][number]) =>
		({
			create_table: 'create the table',
			define_field: `define the column for “${change.field}”`,
			define_index: `add ${change.unique ? 'a unique ' : 'an '}index on ${change.field}`,
			remove_index: `remove the index on ${change.field}`,
			backfill: `fill existing records for ${change.field}`
		})[change.kind] ?? change.kind;
</script>

<section class="space-y-5" aria-label="Model editor">
	<header class="flex flex-wrap items-end gap-4">
		<div class="space-y-1">
			<p class="text-xs text-muted-foreground">{plugin.label} · version {plugin.version}</p>
			<h2 class="text-lg font-semibold tracking-tight">
				{label || model.name}
				{#if isNew}<span class="ml-2 rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">New</span>{/if}
			</h2>
			<p class="font-mono text-xs text-muted-foreground">
				{model.name}{model.model_id ? ` · ${model.model_id}` : ''}
			</p>
		</div>
		<div class="ml-auto w-full max-w-xs space-y-1">
			<label for="model-label" class="text-xs font-medium text-muted-foreground">Label</label>
			<Input id="model-label" bind:value={label} placeholder={model.name} />
		</div>
	</header>

	<div class="rounded-lg border border-border bg-card px-4 py-3" data-testid="chatter-settings">
		<label class="flex items-start gap-3">
			<input type="checkbox" class="mt-1 size-4 rounded border-input" bind:checked={chatter.enabled} aria-label="Enable chatter" />
			<span>
				<span class="text-sm font-medium">Chatter</span>
				<span class="block text-xs text-muted-foreground">
					A conversation and history on every record: messages, internal notes, followers, and a log of changes to tracked fields. Off unless you turn it on.
				</span>
			</span>
		</label>
		{#if chatter.enabled}
			<div class="mt-3 grid gap-3 border-t border-dashed border-border pt-3 sm:grid-cols-2 lg:grid-cols-3">
				<label class="flex items-center gap-2 text-sm"><input type="checkbox" class="size-4 rounded border-input" bind:checked={chatter.messages} /> Messages <span class="text-xs text-muted-foreground">notify followers</span></label>
				<label class="flex items-center gap-2 text-sm"><input type="checkbox" class="size-4 rounded border-input" bind:checked={chatter.notes} /> Internal notes <span class="text-xs text-muted-foreground">notify nobody</span></label>
				<label class="flex items-center gap-2 text-sm"><input type="checkbox" class="size-4 rounded border-input" bind:checked={chatter.followers} /> Followers</label>
				<label class="flex items-center gap-2 text-sm"><input type="checkbox" class="size-4 rounded border-input" bind:checked={chatter.track_changes} /> Track changes <span class="text-xs text-muted-foreground">choose fields below</span></label>
				<div class="space-y-1 sm:col-span-2">
					<label class="text-xs font-medium text-muted-foreground" for="chatter-visitors">Anonymous visitors</label>
					<NativeSelect id="chatter-visitors" value={chatter.visitors ?? 'none'} onchange={(event) => (chatter.visitors = event.currentTarget.value as VisitorChatter)}>
						<option value="none">Cannot see it</option>
						<option value="read">Can read messages</option>
						<option value="read_write">Can read and post messages</option>
					</NativeSelect>
				</div>
			</div>
		{/if}
	</div>

	<div class="overflow-hidden rounded-lg border border-border bg-card">
		<div class="grid grid-cols-[1.4fr_1.2fr_1.2fr_5rem_6.5rem] gap-3 border-b border-border bg-muted/40 px-4 py-2 text-xs font-medium text-muted-foreground">
			<span>Label</span><span>Name</span><span>Type</span><span>Required</span><span></span>
		</div>
		{#each rows as row, index (row.key)}
			<div class="border-b border-border last:border-b-0 {row.def.deprecated ? 'bg-muted/30' : ''}">
				<div class="grid grid-cols-[1.4fr_1.2fr_1.2fr_5rem_6.5rem] items-center gap-3 px-4 py-2">
					<Input bind:value={row.def.label} placeholder="Label" disabled={row.def.deprecated} aria-label="Label" />
					<div class="space-y-0.5">
						<Input
							bind:value={row.def.name}
							class="font-mono"
							placeholder="field_name"
							disabled={row.def.deprecated}
							aria-label="Field name"
							aria-invalid={row.def.name !== '' && !isValidName(row.def.name)}
						/>
					</div>
					<NativeSelect
						value={row.def.type}
						onchange={(event) => setType(row, event.currentTarget.value as FieldType)}
						disabled={row.def.deprecated}
						aria-label="Type"
					>
						{#each FIELD_TYPES as type (type.value)}
							<option value={type.value}>{type.label}</option>
						{/each}
					</NativeSelect>
					<label class="flex items-center gap-2 text-sm">
						<input type="checkbox" class="size-4 rounded border-input" bind:checked={row.def.required} disabled={row.def.deprecated} />
					</label>
					<div class="flex items-center justify-end gap-1">
						<Button variant="ghost" size="sm" onclick={() => (row.open = !row.open)} aria-label="More settings" aria-expanded={row.open}>
							<ChevronRightIcon class="size-4 transition {row.open ? 'rotate-90' : ''}" />
						</Button>
						{#if row.def.deprecated}
							<Button variant="outline" size="sm" onclick={() => (row.def.deprecated = false)}>Show</Button>
						{:else}
							<Button variant="ghost" size="sm" class="text-muted-foreground hover:text-destructive" onclick={() => removeOrHide(row, index)}>
								{row.def.id ? 'Hide' : 'Remove'}
							</Button>
						{/if}
					</div>
				</div>
				{#if row.def.deprecated}
					<p class="px-4 pb-2 text-xs text-muted-foreground">Hidden. Plugins no longer see this field; its data is kept.</p>
				{/if}
				{#if row.open && !row.def.deprecated}
					<div class="grid gap-4 border-t border-dashed border-border bg-muted/20 px-4 py-3 sm:grid-cols-3">
						{#if row.def.type !== 'json' && row.def.type !== 'link'}
							<div class="space-y-1">
								<label class="text-xs font-medium text-muted-foreground" for="default-{row.key}">Default</label>
								{#if row.def.type === 'bool'}
									<NativeSelect id="default-{row.key}" value={row.def.default === undefined ? '' : String(row.def.default)} onchange={(event) => setDefault(row, event.currentTarget.value)}>
										<option value="">None</option><option value="true">Yes</option><option value="false">No</option>
									</NativeSelect>
								{:else if row.def.type === 'select'}
									<NativeSelect id="default-{row.key}" value={String(row.def.default ?? '')} onchange={(event) => setDefault(row, event.currentTarget.value)}>
										<option value="">None</option>
										{#each (row.def.options ?? []).filter((o) => o.value) as option (option.value)}<option value={option.value}>{option.value}</option>{/each}
									</NativeSelect>
								{:else}
									<Input id="default-{row.key}" type={row.def.type === 'int' || row.def.type === 'float' ? 'number' : 'text'} value={String(row.def.default ?? '')} onchange={(event) => setDefault(row, event.currentTarget.value)} />
								{/if}
							</div>
						{/if}
						{#if row.def.type === 'string' || row.def.type === 'text'}
							<div class="space-y-1">
								<label class="text-xs font-medium text-muted-foreground" for="max-{row.key}">Longest allowed</label>
								<Input id="max-{row.key}" type="number" min="1" value={String(row.def.max_length ?? '')} onchange={(event) => (row.def.max_length = event.currentTarget.value ? Number(event.currentTarget.value) : undefined)} />
							</div>
						{/if}
						{#if row.def.type !== 'json' && row.def.type !== 'text'}
							<div class="space-y-1">
								<label class="text-xs font-medium text-muted-foreground" for="index-{row.key}">Index</label>
								<NativeSelect id="index-{row.key}" value={row.def.index ?? ''} onchange={(event) => (row.def.index = (event.currentTarget.value || undefined) as FieldDef['index'])}>
									<option value="">None</option><option value="plain">Fast lookups</option><option value="unique">Unique</option>
								</NativeSelect>
							</div>
						{/if}
						{#if row.def.type === 'link'}
							<div class="space-y-1">
								<label class="text-xs font-medium text-muted-foreground" for="target-{row.key}">Links to</label>
								<NativeSelect id="target-{row.key}" value={row.def.target ?? ''} onchange={(event) => (row.def.target = event.currentTarget.value || undefined)}>
									<option value="">Choose a model…</option>
									{#each siblings as name (name)}<option value={name}>{name}</option>{/each}
								</NativeSelect>
							</div>
						{/if}
						{#if chatter.enabled && chatter.track_changes && row.def.type !== 'json'}
							<label class="flex items-center gap-2 text-sm sm:col-span-3">
								<input type="checkbox" class="size-4 rounded border-input" bind:checked={row.def.track} aria-label="Track changes" />
								Track changes <span class="text-xs text-muted-foreground">write old and new values to the chatter when this field changes</span>
							</label>
						{/if}
						<div class="space-y-1 sm:col-span-3">
							<label class="text-xs font-medium text-muted-foreground" for="help-{row.key}">Help text</label>
							<Input id="help-{row.key}" bind:value={row.def.help} />
						</div>
						{#if row.def.type === 'select'}
							<div class="space-y-2 sm:col-span-3">
								<p class="text-xs font-medium text-muted-foreground">Options</p>
								{#each row.def.options ?? [] as option, optionIndex (optionIndex)}
									<div class="flex items-center gap-2">
										<Input class="max-w-48 font-mono" bind:value={option.value} placeholder="value" aria-label="Option value" />
										<Input class="max-w-48" bind:value={option.label} placeholder="Label" aria-label="Option label" />
										<NativeSelect class="max-w-32" value={option.color ?? ''} onchange={(event) => (option.color = event.currentTarget.value || undefined)} aria-label="Option colour">
											<option value="">No colour</option>
											{#each colors as color (color)}<option value={color}>{color}</option>{/each}
										</NativeSelect>
										<Button variant="ghost" size="sm" onclick={() => row.def.options?.splice(optionIndex, 1)}>Remove</Button>
									</div>
								{/each}
								<Button variant="outline" size="sm" onclick={() => row.def.options?.push({ value: '' })}>Add option</Button>
							</div>
						{/if}
					</div>
				{/if}
			</div>
		{:else}
			<p class="px-4 py-6 text-sm text-muted-foreground">No fields yet.</p>
		{/each}
		<div class="border-t border-border bg-muted/20 px-4 py-2">
			<Button variant="ghost" size="sm" onclick={addField}><PlusIcon class="size-4" /> Add field</Button>
		</div>
	</div>

	<p class="text-xs text-muted-foreground">
		Renaming a field keeps its data: a field is stored under its id, and only the name changes. Hiding one keeps its data too.
	</p>

	<div class="space-y-4 rounded-lg border border-border bg-card p-4">
		<div class="space-y-2">
			<h3 class="text-sm font-semibold">Apply to organizations</h3>
			<p class="text-xs text-muted-foreground">
				Saving makes a new version of {plugin.label}. Organizations you tick are moved to it now; the others keep the version they have.
			</p>
			<div class="flex flex-wrap gap-x-5 gap-y-1">
				{#each organizations as organization (organization.db_name)}
					<label class="flex items-center gap-2 text-sm">
						<input type="checkbox" class="size-4 rounded border-input" checked={applyTo.includes(organization.db_name)} onchange={() => toggleOrganization(organization.db_name)} />
						{organization.name}
					</label>
				{:else}
					<span class="text-sm text-muted-foreground">There are no organizations yet.</span>
				{/each}
			</div>
		</div>

		<label class="flex items-start gap-2 text-sm {fileBlocked ? 'opacity-60' : ''}">
			<input type="checkbox" class="mt-0.5 size-4 rounded border-input" bind:checked={writeFile} disabled={fileBlocked} />
			<span>
				Also write the model file
				<span class="block text-xs text-muted-foreground">
					{#if fileBlocked}
						The plugin's source folder is not on this machine, so only the database can be changed.
					{:else if writeFile}
						The change is saved to the database and to <code>models/{model.name}.json</code> in the plugin's folder.
					{:else}
						The change is saved to the database only; the plugin's JSON file is left as it is.
					{/if}
				</span>
			</span>
		</label>

		{#if problems.length > 0}
			<ul class="list-disc space-y-0.5 pl-5 text-sm text-destructive" role="alert">
				{#each problems as problem (problem)}<li>{problem}</li>{/each}
			</ul>
		{/if}
		{#if failure}<p class="text-sm text-destructive" role="alert">{failure}</p>{/if}

		{#if plans}
			<div class="space-y-3 rounded-md border border-border bg-muted/20 p-3 text-sm">
				<p class="font-medium">What saving would do</p>
				{#each plans as plan (plan.organization)}
					<div>
						<p class="font-medium">{organizations.find((o) => o.db_name === plan.organization)?.name ?? plan.organization}</p>
						{#if plan.blockers.length > 0}
							{#each plan.blockers as blocker (blocker)}<p class="text-destructive">Blocked: {blocker}</p>{/each}
						{:else if plan.changes.length === 0}
							<p class="text-muted-foreground">No change to the database.</p>
						{:else}
							<ul class="list-disc pl-5 text-muted-foreground">
								{#each plan.changes as change, i (i)}<li>{describe(change)}</li>{/each}
							</ul>
						{/if}
						{#each plan.notes as note (note)}<p class="text-xs text-muted-foreground">{note}</p>{/each}
					</div>
				{:else}
					<p class="text-muted-foreground">No organization is ticked: the new version is saved but nobody is moved to it.</p>
				{/each}
			</div>
		{/if}

		<div class="flex flex-wrap gap-2">
			<Button variant="outline" onclick={preview} disabled={working !== null || problems.length > 0}>
				{working === 'preview' ? 'Checking…' : 'Preview changes'}
			</Button>
			<Button onclick={save} disabled={working !== null || problems.length > 0 || (!dirty && !isNew)}>
				{working === 'save' ? 'Saving…' : 'Save'}
			</Button>
		</div>
	</div>
</section>
