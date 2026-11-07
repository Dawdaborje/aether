<script lang="ts">
	import { TableSkeleton } from '$lib/components/ui/skeleton';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Badge } from '$lib/components/ui/badge';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { OTHER_CATEGORY, categoryLabel } from '$lib/apps/categories';
	import {
		fetchCatalog,
		installApps,
		type CatalogPlugin,
		type InstallOutcome
	} from '$lib/apps/catalogApi';
	import { groupCatalogByName } from '$lib/apps/catalogGroup';
	import { formatCatalogDate } from '$lib/apps/formatCatalogDate';
	import { matchPlugin } from '$lib/apps/matchPlugin';
	import { iconRef } from '$lib/icons';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import OrgPicker from '$lib/components/apps/OrgPicker.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let {
		open = $bindable(false),
		preselected = [],
		preselectedWorkspaces = []
	}: {
		open?: boolean;
		/** `name@version` keys to check when the modal opens (single-plugin install). */
		preselected?: string[];
		/** Category labels to select entirely when the modal opens. */
		preselectedWorkspaces?: string[];
	} = $props();

	let catalog = $state<CatalogPlugin[] | null>(null);
	let loadError = $state<string | null>(null);
	let chosenWorkspaces = $state<string[]>([]);
	let chosenApps = $state<string[]>([]);
	let chosenOrgs = $state<string[]>([]);
	let working = $state(false);
	let results = $state<InstallOutcome[] | null>(null);
	let failed = $state<string | null>(null);
	let query = $state('');
	let collapsed = $state<string[]>([]);

	const key = (plugin: CatalogPlugin) => `${plugin.name}@${plugin.version}`;
	const organizations = $derived(orgStore.organizations);
	const orgName = (dbName: string) =>
		organizations.find((org) => org.db_name === dbName)?.name ?? dbName;

	/** Chosen version per plugin name (defaults to newest when selecting). */
	let versionByName = $state<Record<string, string>>({});

	const groups = $derived(groupCatalogByName(catalog ?? []));

	const filteredGroups = $derived.by(() =>
		groups.filter((group) => group.versions.some((plugin) => matchPlugin(plugin, query)))
	);

	function selectedKey(name: string): string | null {
		const group = groups.find((g) => g.name === name);
		if (!group) return null;
		const version = versionByName[name] ?? group.latest.version;
		return `${name}@${version}`;
	}

	const workspaces = $derived.by(() => {
		const map = new Map<string, typeof filteredGroups>();
		for (const group of filteredGroups) {
			const label = categoryLabel(group.latest.category);
			const list = map.get(label) ?? [];
			list.push(group);
			map.set(label, list);
		}
		return [...map.entries()]
			.sort(([a], [b]) => {
				if (a === OTHER_CATEGORY) return 1;
				if (b === OTHER_CATEGORY) return -1;
				return a.localeCompare(b);
			})
			.map(([label, plugins]) => ({
				label,
				plugins,
				keys: plugins.map((g) => selectedKey(g.name)!).filter(Boolean)
			}));
	});

	/** Full workspace membership (unfiltered) — one key per plugin name (chosen/latest version). */
	const workspaceAllKeys = $derived.by(() => {
		const map = new Map<string, string[]>();
		for (const group of groups) {
			const label = categoryLabel(group.latest.category);
			const list = map.get(label) ?? [];
			const k = selectedKey(group.name);
			if (k) list.push(k);
			map.set(label, list);
		}
		return map;
	});

	async function load() {
		catalog = null;
		loadError = null;
		try {
			catalog = await fetchCatalog();
		} catch (error) {
			loadError = error instanceof Error ? error.message : 'Could not load the catalog';
			catalog = [];
		}
	}

	function keysForGroups(
		labels: string[],
		pluginGroups: ReturnType<typeof groupCatalogByName>,
		versions: Record<string, string>
	): string[] {
		const wanted = new Set(labels);
		return pluginGroups
			.filter((group) => wanted.has(categoryLabel(group.latest.category)))
			.map((group) => `${group.name}@${versions[group.name] ?? group.latest.version}`);
	}

	function setVersion(name: string, version: string) {
		const prev = selectedKey(name);
		versionByName = { ...versionByName, [name]: version };
		const next = `${name}@${version}`;
		if (prev && chosenApps.includes(prev) && prev !== next) {
			chosenApps = chosenApps.map((k) => (k === prev ? next : k));
		}
	}

	// Start fresh each time the modal opens.
	$effect(() => {
		if (!open) return;
		results = null;
		failed = null;
		query = '';
		versionByName = {};
		chosenOrgs = orgStore.current ? [orgStore.current] : [];
		chosenWorkspaces = [...preselectedWorkspaces];
		chosenApps = [...preselected];
		void load().then(() => {
			if (!open || !catalog) return;
			const pluginGroups = groupCatalogByName(catalog);
			const nextVersions: Record<string, string> = {};
			for (const group of pluginGroups) {
				const seeded = preselected.find((k) => k.startsWith(`${group.name}@`));
				nextVersions[group.name] = seeded
					? seeded.slice(group.name.length + 1)
					: group.latest.version;
			}
			versionByName = nextVersions;
			if (preselectedWorkspaces.length > 0) {
				chosenWorkspaces = [...preselectedWorkspaces];
				chosenApps = keysForGroups(preselectedWorkspaces, pluginGroups, nextVersions);
				return;
			}
			if (preselected.length > 0) {
				chosenApps = [...preselected];
				const selected = new Set(preselected);
				const byWorkspace = new Map<string, string[]>();
				for (const group of pluginGroups) {
					const label = categoryLabel(group.latest.category);
					const list = byWorkspace.get(label) ?? [];
					list.push(`${group.name}@${nextVersions[group.name] ?? group.latest.version}`);
					byWorkspace.set(label, list);
				}
				chosenWorkspaces = [...byWorkspace.entries()]
					.filter(([, keys]) => keys.length > 0 && keys.every((k) => selected.has(k)))
					.map(([label]) => label);
				return;
			}
			chosenWorkspaces = [];
			chosenApps = [];
		});
	});

	function toggle(list: string[], value: string): string[] {
		return list.includes(value) ? list.filter((item) => item !== value) : [...list, value];
	}

	function toggleWorkspace(label: string) {
		const allKeys = workspaceAllKeys.get(label) ?? [];
		if (allKeys.length === 0) return;
		const selecting = !chosenWorkspaces.includes(label);
		chosenWorkspaces = selecting
			? [...chosenWorkspaces, label]
			: chosenWorkspaces.filter((item) => item !== label);
		if (selecting) {
			const set = new Set(chosenApps);
			for (const k of allKeys) set.add(k);
			chosenApps = [...set];
		} else {
			const drop = new Set(allKeys);
			chosenApps = chosenApps.filter((k) => !drop.has(k));
		}
	}

	function togglePlugin(pluginKey: string, workspaceLabel: string) {
		const next = toggle(chosenApps, pluginKey);
		chosenApps = next;
		const allKeys = workspaceAllKeys.get(workspaceLabel) ?? [];
		const selected = new Set(next);
		const fullySelected = allKeys.length > 0 && allKeys.every((k) => selected.has(k));
		if (fullySelected && !chosenWorkspaces.includes(workspaceLabel)) {
			chosenWorkspaces = [...chosenWorkspaces, workspaceLabel];
		} else if (!fullySelected && chosenWorkspaces.includes(workspaceLabel)) {
			chosenWorkspaces = chosenWorkspaces.filter((item) => item !== workspaceLabel);
		}
	}

	function workspaceState(label: string): 'all' | 'some' | 'none' {
		const allKeys = workspaceAllKeys.get(label) ?? [];
		if (allKeys.length === 0) return 'none';
		const selected = new Set(chosenApps);
		const count = allKeys.filter((k) => selected.has(k)).length;
		if (count === 0) return 'none';
		if (count === allKeys.length) return 'all';
		return 'some';
	}

	const searching = $derived(query.trim().length > 0);
	const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

	function toggleCollapsed(label: string) {
		collapsed = toggle(collapsed, label);
	}

	/** How many of the chosen organizations already have this plugin (any version). */
	function installedCount(plugin: CatalogPlugin): number {
		const there = new Set(plugin.installed_in);
		return chosenOrgs.filter((db) => there.has(db)).length;
	}

	const hint = $derived(
		chosenApps.length === 0
			? 'Choose at least one app'
			: chosenOrgs.length === 0
				? 'Choose at least one organization'
				: null
	);

	const totals = $derived.by(() => {
		const sum = { installed: 0, upgraded: 0, already: 0, failed: 0 };
		for (const o of results ?? []) {
			if (o.error) sum.failed += 1;
			sum.installed += o.installed.length;
			sum.upgraded += o.upgraded.length;
			sum.already += o.already_installed.length;
		}
		return sum;
	});

	async function install() {
		working = true;
		failed = null;
		try {
			results = await installApps(chosenApps, chosenOrgs);
			await load();
			// The launcher list needs an organization; skip it on the desk when none is chosen.
			if (orgStore.current) {
				await appsStore.load();
			}
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not install';
		} finally {
			working = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content
		class="flex h-[min(90vh,50rem)] w-[min(64rem,calc(100%-2rem))] max-w-[min(64rem,calc(100%-2rem))] flex-col gap-4 overflow-hidden sm:max-w-[min(64rem,calc(100%-2rem))]"
	>
		<Dialog.Header class="shrink-0 pr-10">
			<Dialog.Title>{results ? 'Install finished' : 'Install apps'}</Dialog.Title>
			<Dialog.Description>
				{#if results}
					Here is what happened in each organization.
				{:else}
					Pick the apps, then the organizations that should get them. Dependencies come along
					automatically.
				{/if}
			</Dialog.Description>
		</Dialog.Header>

		{#if results}
			<div class="flex shrink-0 flex-wrap gap-2">
				<Badge>{plural(totals.installed, 'app')} installed</Badge>
				{#if totals.upgraded > 0}<Badge variant="secondary">{totals.upgraded} upgraded</Badge>{/if}
				{#if totals.already > 0}
					<Badge variant="outline">{totals.already} already there</Badge>
				{/if}
				{#if totals.failed > 0}
					<Badge variant="destructive">{plural(totals.failed, 'organization')} failed</Badge>
				{/if}
			</div>
			<ul class="min-h-0 flex-1 space-y-2 overflow-auto text-sm">
				{#each results as outcome (outcome.organization)}
					<li class="rounded-md border border-border p-3">
						<div class="flex items-center gap-2">
							{#if outcome.error}
								<AppIcon name="error-filled" class="size-4 shrink-0 text-destructive" />
							{:else}
								<AppIcon name="check-circle-filled" class="size-4 shrink-0 text-primary" />
							{/if}
							<p class="min-w-0 flex-1 truncate font-medium">{orgName(outcome.organization)}</p>
						</div>
						{#if outcome.error}
							<p class="mt-1 pl-6 text-destructive" role="alert">{outcome.error}</p>
						{:else}
							<dl class="mt-2 space-y-1.5 pl-6">
								{#each [['Installed', outcome.installed], ['Upgraded', outcome.upgraded], ['Already there', outcome.already_installed]] as [label, names] (label)}
									{#if (names as string[]).length > 0}
										<div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
											<dt class="w-24 shrink-0 text-xs text-muted-foreground">{label}</dt>
											<dd class="flex min-w-0 flex-1 flex-wrap gap-1">
												{#each names as string[] as name (name)}
													<Badge variant="secondary" class="font-mono">{name}</Badge>
												{/each}
											</dd>
										</div>
									{/if}
								{/each}
							</dl>
						{/if}
					</li>
				{/each}
			</ul>
			<Dialog.Footer class="shrink-0">
				<Button onclick={() => (open = false)}>Done</Button>
			</Dialog.Footer>
		{:else}
			<div class="grid min-h-0 flex-1 gap-6 lg:grid-cols-[minmax(0,1fr)_18rem]">
				<section class="flex min-h-0 flex-col gap-2">
					<div class="flex shrink-0 items-center justify-between gap-2">
						<h3 class="text-sm font-semibold">
							<span class="mr-1.5 text-muted-foreground">1</span>Apps
						</h3>
						<p class="text-xs text-muted-foreground tabular-nums">
							{chosenApps.length} selected
						</p>
					</div>
					<div class="relative shrink-0">
						<AppIcon
							name="search"
							class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
						/>
						<Input
							bind:value={query}
							type="search"
							placeholder="Search workspaces and apps…"
							aria-label="Search workspaces and apps"
							class="pl-9"
						/>
					</div>
					<div class="min-h-0 flex-1 overflow-auto rounded-md border border-border">
						{#if catalog === null}
							<TableSkeleton rows={8} columns={2} label="Loading the catalog…" class="border-0" />
						{:else if loadError}
							<LoadFailed message={loadError} onRetry={load} />
						{:else if workspaces.length === 0}
							<p class="p-4 text-sm text-muted-foreground">
								{#if searching}
									No apps match “{query.trim()}”.
								{:else}
									No plugins are loaded. Load with <code>aether --load-plugins</code>.
								{/if}
							</p>
						{:else}
							{#each workspaces as ws (ws.label)}
								{@const state = workspaceState(ws.label)}
								{@const isOpen = searching || !collapsed.includes(ws.label)}
								<div class="border-b border-border last:border-b-0">
									<div
										class="sticky top-0 z-10 flex items-center gap-2 bg-muted/60 px-3 py-2 backdrop-blur"
									>
										<input
											type="checkbox"
											class="rounded-sm"
											aria-label={`Select every app in ${ws.label}`}
											checked={state === 'all'}
											indeterminate={state === 'some'}
											onchange={() => toggleWorkspace(ws.label)}
										/>
										<button
											type="button"
											class="flex min-w-0 flex-1 items-center gap-2 text-left"
											aria-expanded={isOpen}
											onclick={() => toggleCollapsed(ws.label)}
										>
											<span class="min-w-0 flex-1 truncate text-sm font-medium">{ws.label}</span>
											<span class="text-xs text-muted-foreground tabular-nums">
												{#if searching}
													{ws.plugins.length} of {(workspaceAllKeys.get(ws.label) ?? []).length}
												{:else}
													{plural(ws.plugins.length, 'app')}
												{/if}
											</span>
											<AppIcon
												name="chevron-down"
												class="size-4 text-muted-foreground transition-transform {isOpen
													? ''
													: '-rotate-90'}"
											/>
										</button>
									</div>
									{#if isOpen}
										<ul class="divide-y divide-border/60">
											{#each ws.plugins as group (group.name)}
												{@const plugin =
													group.versions.find(
														(v) => v.version === (versionByName[group.name] ?? group.latest.version)
													) ?? group.latest}
												{@const pluginKey = key(plugin)}
												{@const Icon = iconRef(plugin.icon)}
												{@const there = installedCount(plugin)}
												<li>
													<div
														class="flex items-center gap-3 px-3 py-2 hover:bg-accent/50 {chosenApps.includes(
															pluginKey
														)
															? 'bg-accent/60'
															: ''}"
													>
														<input
															type="checkbox"
															class="rounded-sm"
															aria-label={`Install ${plugin.label}`}
															checked={chosenApps.includes(pluginKey)}
															onchange={() => togglePlugin(pluginKey, ws.label)}
														/>
														<button
															type="button"
															class="flex min-w-0 flex-1 items-center gap-3 text-left"
															onclick={() => togglePlugin(pluginKey, ws.label)}
														>
															<span
																class="flex size-9 shrink-0 items-center justify-center overflow-hidden bg-primary/10 text-primary ring-1 ring-primary/15"
																aria-hidden="true"
															>
																{#if plugin.logo_url}
																	<img src={plugin.logo_url} alt="" class="size-7 object-contain" />
																{:else if Icon}
																	<AppIcon name={Icon} class="size-4" />
																{:else}
																	<span class="text-xs font-semibold"
																		>{plugin.label.charAt(0).toUpperCase()}</span
																	>
																{/if}
															</span>
															<span class="min-w-0 flex-1">
																<span class="block truncate text-sm font-medium"
																	>{plugin.label}</span
																>
																<span class="block truncate text-xs text-muted-foreground">
																	{plugin.name}
																</span>
															</span>
														</button>
														{#if plugin.load_failure}
															<Badge variant="destructive" title={plugin.load_failure.message}>
																Load failed
															</Badge>
														{/if}
														{#if chosenOrgs.length > 0 && there > 0}
															<Badge
																variant={there === chosenOrgs.length ? 'secondary' : 'outline'}
															>
																{there === chosenOrgs.length
																	? 'Installed'
																	: `Installed in ${there}/${chosenOrgs.length}`}
															</Badge>
														{/if}
														{#if group.versions.length > 1}
															<select
																class="w-32 shrink-0 rounded-sm border border-border bg-background px-1 py-1 font-mono text-xs"
																value={plugin.version}
																aria-label={`Version of ${plugin.label}`}
																onchange={(e) =>
																	setVersion(
																		group.name,
																		(e.currentTarget as HTMLSelectElement).value
																	)}
															>
																{#each group.versions as version (version.version)}
																	{@const when = formatCatalogDate(version.date_created)}
																	<option value={version.version}>
																		v{version.version}{when ? ` · ${when}` : ''}
																	</option>
																{/each}
															</select>
														{:else}
															<span
																class="w-32 shrink-0 truncate font-mono text-xs text-muted-foreground"
															>
																v{plugin.version}
															</span>
														{/if}
													</div>
												</li>
											{/each}
										</ul>
									{/if}
								</div>
							{/each}
						{/if}
					</div>
				</section>

				<section class="flex min-h-0 flex-col">
					<OrgPicker orgs={organizations} bind:selected={chosenOrgs} heading="2 · Organizations" />
				</section>
			</div>

			{#if failed}
				<p class="shrink-0 text-sm text-destructive" role="alert">{failed}</p>
			{/if}
			<Dialog.Footer class="shrink-0 items-center sm:justify-between">
				<p class="text-sm text-muted-foreground" aria-live="polite">
					{#if hint}
						{hint}
					{:else}
						{plural(chosenApps.length, 'app')} → {plural(chosenOrgs.length, 'organization')}
					{/if}
				</p>
				<div class="flex gap-2">
					<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
					<Button disabled={working || hint !== null} onclick={install}>
						{working
							? 'Installing…'
							: chosenApps.length > 0
								? `Install ${plural(chosenApps.length, 'app')}`
								: 'Install'}
					</Button>
				</div>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
