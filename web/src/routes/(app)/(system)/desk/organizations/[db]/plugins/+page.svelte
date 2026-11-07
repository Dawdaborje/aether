<script lang="ts">
	import { page } from '$app/state';
	import { textParam } from '$lib/url/filterParams';
	import { syncFilters } from '$lib/url/syncFilters.svelte';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { loadFailure, reportError } from '$lib/errors/report';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Empty } from '$lib/components/ui/empty';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { Switch } from '$lib/components/ui/switch';
	import ConfirmDialog from '$lib/components/dialogs/ConfirmDialog.svelte';
	import {
		fetchCatalog,
		fetchOrganizationPlugins,
		previewUninstall,
		purgeOrphans,
		setPluginEnabled,
		uninstallApps,
		type CatalogPlugin,
		type InstalledPlugin,
		type PluginData,
		type UninstallPreview
	} from '$lib/apps/catalogApi';

	let { params }: { params: { db: string } } = $props();

	type Row = InstalledPlugin & {
		label: string;
		description: string | null;
		is_app: boolean;
		category: string | null;
	};

	let plugins = $state<Row[] | null>(null);
	let orphans = $state<PluginData[]>([]);
	let failed = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let selected = $state<string[]>([]);
	let working = $state(false);
	/** Plugin whose switch is being flipped. */
	let toggling = $state<string | null>(null);

	async function load(db: string) {
		plugins = null;
		failed = null;
		selected = [];
		try {
			const [catalog, organization] = await Promise.all([
				fetchCatalog(),
				fetchOrganizationPlugins(db)
			]);
			const described = (name: string, version: string): CatalogPlugin | undefined =>
				catalog.find((plugin) => plugin.name === name && plugin.version === version) ??
				catalog.find((plugin) => plugin.name === name);
			plugins = organization.installed.map((row) => {
				const plugin = described(row.name, row.version);
				return {
					...row,
					label: plugin?.label || row.name,
					description: plugin?.description ?? null,
					is_app: plugin?.is_app ?? false,
					category: plugin?.category ?? null
				};
			});
			orphans = organization.orphans;
		} catch (error) {
			failed = loadFailure(error, 'Plugins could not be loaded.');
			plugins = [];
			orphans = [];
		}
	}

	let query = $state(textParam(page.url.searchParams, 'q'));
	/** Selected category; null shows all, '' shows plugins without one. */
	let category = $state<string | null>(page.url.searchParams.get('category'));
	syncFilters(() => ({ q: query, category }));

	const categories = $derived(
		[
			...new Set((plugins ?? []).map((plugin) => plugin.category).filter((c): c is string => !!c))
		].sort()
	);
	const hasUncategorized = $derived((plugins ?? []).some((plugin) => !plugin.category));

	const visible = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		return (plugins ?? []).filter((plugin) => {
			if (category !== null && (plugin.category ?? '') !== category) return false;
			if (!needle) return true;
			return [plugin.label, plugin.name, plugin.description ?? '', plugin.category ?? '']
				.join(' ')
				.toLowerCase()
				.includes(needle);
		});
	});

	const allSelected = $derived(
		visible.length > 0 && visible.every((plugin) => selected.includes(plugin.name))
	);

	function toggle(name: string) {
		selected = selected.includes(name)
			? selected.filter((item) => item !== name)
			: [...selected, name];
	}

	function toggleAll() {
		const names = visible.map((plugin) => plugin.name);
		selected = allSelected
			? selected.filter((name) => !names.includes(name))
			: [...new Set([...selected, ...names])];
	}

	function labelOf(name: string): string {
		return plugins?.find((plugin) => plugin.name === name)?.label ?? name;
	}

	async function flip(plugin: Row, enabled: boolean) {
		toggling = plugin.name;
		notice = null;
		failed = null;
		try {
			await setPluginEnabled(plugin.name, params.db, enabled);
			plugin.is_enabled = enabled;
			notice = `${plugin.label} ${enabled ? 'enabled' : 'disabled'}.`;
		} catch (error) {
			failed = reportError(
				error,
				`${plugin.label} could not be ${enabled ? 'enabled' : 'disabled'}.`
			);
		} finally {
			toggling = null;
		}
	}

	// Uninstall, with or without purge: the dialog shows what will go before anything does.
	let removing = $state<{ names: string[]; purge: boolean } | null>(null);
	let removeOpen = $state(false);
	let preview = $state<UninstallPreview | null>(null);
	let previewError = $state<string | null>(null);
	let removeError = $state<string | null>(null);

	async function askRemove(names: string[], purge: boolean) {
		if (names.length === 0) return;
		removing = { names, purge };
		preview = null;
		previewError = null;
		removeError = null;
		removeOpen = true;
		try {
			preview = await previewUninstall(names, params.db);
		} catch (error) {
			previewError = reportError(error, 'The uninstall could not be checked.');
		}
	}

	async function confirmRemove() {
		if (!removing) return;
		working = true;
		removeError = null;
		notice = null;
		failed = null;
		const { names, purge } = removing;
		try {
			const result = await uninstallApps(names, params.db, purge);
			const rows = (purge ? result.purged_data : result.kept_data).reduce(
				(sum, row) => sum + row.records,
				0
			);
			notice = `${result.uninstalled.length} plugin(s) uninstalled.${
				rows > 0 ? ` ${rows} record(s) ${purge ? 'dropped' : 'kept'}.` : ''
			}`;
			removeOpen = false;
			await load(params.db);
		} catch (error) {
			removeError = reportError(error, 'The plugins could not be uninstalled.');
		} finally {
			working = false;
		}
	}

	const removeTitle = $derived.by(() => {
		if (!removing) return '';
		const what =
			removing.names.length === 1 ? labelOf(removing.names[0]) : `${removing.names.length} plugins`;
		return removing.purge ? `Uninstall ${what} and purge their data?` : `Uninstall ${what}?`;
	});
	const previewRecords = $derived(preview?.data.reduce((sum, row) => sum + row.records, 0) ?? 0);

	// Purging data left by an earlier uninstall.
	let purging = $state<PluginData | null>(null);
	let purgeOpen = $state(false);
	let purgeError = $state<string | null>(null);

	function askPurge(data: PluginData) {
		purging = data;
		purgeError = null;
		purgeOpen = true;
	}

	async function confirmPurge() {
		if (!purging) return;
		working = true;
		purgeError = null;
		notice = null;
		try {
			const dropped = await purgeOrphans(purging.plugin, params.db);
			notice = `Dropped ${dropped.tables.length} table(s) and ${dropped.records} record(s) of ${dropped.plugin}.`;
			purgeOpen = false;
			await load(params.db);
		} catch (error) {
			purgeError = reportError(error, 'The data could not be purged.');
		} finally {
			working = false;
		}
	}

	$effect(() => {
		void load(params.db);
	});
</script>

{#snippet tables(data: PluginData[])}
	<ul class="max-h-56 space-y-2 overflow-y-auto rounded-md border border-border p-3 text-sm">
		{#each data as owned (owned.plugin)}
			<li>
				<p class="font-medium">
					{labelOf(owned.plugin)}
					<span class="font-normal text-muted-foreground">
						· {owned.tables.length} table(s), {owned.records} record(s)
					</span>
				</p>
				{#if owned.tables.length > 0}
					<ul class="mt-1 space-y-0.5 pl-3 font-mono text-xs text-muted-foreground">
						{#each owned.tables as table (table.table)}
							<li>{table.table} ({table.records})</li>
						{/each}
					</ul>
				{/if}
			</li>
		{/each}
	</ul>
{/snippet}

<div class="flex min-h-0 w-full flex-1 flex-col gap-3">
	<p class="text-sm text-muted-foreground">
		Plugins installed on this organization. Disable switches a plugin off and keeps everything;
		uninstall keeps their data; uninstall and purge also drops the tables their models created.
	</p>
	{#if notice}
		<p class="text-sm text-muted-foreground" role="status">{notice}</p>
	{/if}
	{#if failed && plugins && plugins.length > 0}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{/if}
	{#if plugins === null}
		<div class="space-y-2">
			{#each Array(4) as _, i (i)}
				<Skeleton class="h-12 w-full rounded-lg" />
			{/each}
		</div>
	{:else if failed && plugins.length === 0}
		<LoadFailed message={failed} onRetry={() => load(params.db)} />
	{:else if plugins.length === 0}
		<Empty
			icon="puzzle"
			title="No plugins installed"
			description="Install apps on this organization from the Apps page."
		/>
	{:else}
		<div class="flex flex-wrap items-center gap-2">
			<Input
				type="search"
				placeholder="Search plugins"
				aria-label="Search plugins"
				class="max-w-xs"
				bind:value={query}
			/>
			{#if categories.length > 0}
				<button type="button" onclick={() => (category = null)}>
					<Badge variant={category === null ? 'default' : 'outline'}>All</Badge>
				</button>
				{#each categories as name (name)}
					<button type="button" onclick={() => (category = category === name ? null : name)}>
						<Badge variant={category === name ? 'default' : 'outline'}>{name}</Badge>
					</button>
				{/each}
				{#if hasUncategorized}
					<button type="button" onclick={() => (category = category === '' ? null : '')}>
						<Badge variant={category === '' ? 'default' : 'outline'}>Other</Badge>
					</button>
				{/if}
			{/if}
		</div>
		<div class="flex flex-wrap items-center gap-3">
			<label class="flex items-center gap-2 text-sm">
				<input type="checkbox" checked={allSelected} onchange={toggleAll} disabled={working} />
				{selected.length > 0 ? `${selected.length} selected` : 'Select all'}
			</label>
			<div class="ml-auto flex gap-2">
				<Button
					size="sm"
					variant="outline"
					disabled={working || selected.length === 0}
					onclick={() => askRemove(selected, false)}
				>
					Uninstall
				</Button>
				<Button
					size="sm"
					variant="destructive"
					disabled={working || selected.length === 0}
					onclick={() => askRemove(selected, true)}
				>
					Uninstall and purge
				</Button>
			</div>
		</div>
		<ul class="divide-y divide-border rounded-lg border border-border">
			{#each visible as plugin (plugin.name)}
				<li class="flex flex-wrap items-center gap-3 px-4 py-3">
					<input
						type="checkbox"
						aria-label={`Select ${plugin.label}`}
						checked={selected.includes(plugin.name)}
						onchange={() => toggle(plugin.name)}
						disabled={working}
					/>
					<div class="min-w-0 flex-1">
						<p class="text-sm font-medium" class:text-muted-foreground={!plugin.is_enabled}>
							{plugin.label}
						</p>
						<p class="font-mono text-xs text-muted-foreground">{plugin.name}</p>
						{#if plugin.description}
							<p class="mt-1 text-xs text-muted-foreground">{plugin.description}</p>
						{/if}
						{#if plugin.dependencies.length > 0 || plugin.dependents.length > 0}
							<p class="mt-1 text-xs text-muted-foreground">
								{#if plugin.dependencies.length > 0}
									Needs {plugin.dependencies.map(labelOf).join(', ')}.
								{/if}
								{#if plugin.dependents.length > 0}
									Needed by {plugin.dependents.map(labelOf).join(', ')}.
								{/if}
							</p>
						{/if}
					</div>
					{#if plugin.load_failure}
						<Badge variant="destructive" title={plugin.load_failure.message}>Load failed</Badge>
					{/if}
					{#if !plugin.is_enabled}<Badge variant="outline">Disabled</Badge>{/if}
					{#if plugin.category}<Badge variant="outline">{plugin.category}</Badge>{/if}
					{#if plugin.is_app}<Badge variant="outline">App</Badge>{/if}
					<Badge variant="secondary">v{plugin.version}</Badge>
					<Switch
						size="sm"
						aria-label={`${plugin.is_enabled ? 'Disable' : 'Enable'} ${plugin.label}`}
						bind:checked={() => plugin.is_enabled, (checked) => void flip(plugin, checked)}
						disabled={working || toggling !== null}
					/>
				</li>
			{/each}
		</ul>
		{#if visible.length === 0}
			<p class="text-sm text-muted-foreground">No plugins match.</p>
		{/if}
		<p class="text-xs text-muted-foreground">
			{visible.length === plugins.length
				? `${plugins.length} installed`
				: `${visible.length} of ${plugins.length} installed`}
		</p>
	{/if}

	{#if plugins !== null && orphans.length > 0}
		<section class="space-y-2 pt-4">
			<h2 class="text-sm font-medium">Orphaned data</h2>
			<p class="text-sm text-muted-foreground">
				Data left by plugins uninstalled earlier. Installing a plugin again picks its data back up;
				purging drops it for good (the records are backed up first).
			</p>
			<ul class="divide-y divide-border rounded-lg border border-border">
				{#each orphans as data (data.plugin)}
					<li class="flex flex-wrap items-center gap-3 px-4 py-3">
						<div class="min-w-0 flex-1">
							<p class="font-mono text-sm">{data.plugin}</p>
							<p class="text-xs text-muted-foreground">
								{data.tables.length} table(s), {data.records} record(s)
							</p>
						</div>
						<Button
							size="sm"
							variant="destructive"
							disabled={working}
							onclick={() => askPurge(data)}
						>
							Purge
						</Button>
					</li>
				{/each}
			</ul>
		</section>
	{/if}
</div>

<ConfirmDialog
	bind:open={removeOpen}
	title={removeTitle}
	description={removing?.purge
		? 'Every table these plugins’ models created is dropped. The records are written to purge-backups first, but this cannot be undone from the desk.'
		: 'Their data is kept and comes back if you install them again.'}
	confirmLabel={removing?.purge ? 'Uninstall and purge' : 'Uninstall'}
	destructive={removing?.purge ?? false}
	typeToConfirm={removing?.purge ? params.db : null}
	busy={working}
	disabled={preview === null}
	error={removeError}
	onConfirm={confirmRemove}
>
	{#if previewError}
		<p class="text-sm text-destructive" role="alert">{previewError}</p>
	{:else if preview === null}
		<Skeleton class="h-16 w-full rounded-md" />
	{:else}
		{#if preview.plan.length > 1}
			<p class="text-sm text-muted-foreground">
				Removed in this order: {preview.plan.map((row) => labelOf(row.name)).join(', ')}.
			</p>
		{/if}
		{#if preview.data.some((row) => row.tables.length > 0)}
			<p class="text-sm">
				{removing?.purge ? 'Will drop' : 'Keeps'}
				{previewRecords} record(s):
			</p>
			{@render tables(preview.data.filter((row) => row.tables.length > 0))}
		{:else}
			<p class="text-sm text-muted-foreground">These plugins have no data of their own.</p>
		{/if}
	{/if}
</ConfirmDialog>

<ConfirmDialog
	bind:open={purgeOpen}
	title={`Purge the data of ${purging?.plugin ?? ''}?`}
	description="The tables below are dropped. The records are written to purge-backups first."
	confirmLabel="Purge"
	destructive
	typeToConfirm={params.db}
	busy={working}
	error={purgeError}
	onConfirm={confirmPurge}
>
	{#if purging}
		{@render tables([purging])}
	{/if}
</ConfirmDialog>
