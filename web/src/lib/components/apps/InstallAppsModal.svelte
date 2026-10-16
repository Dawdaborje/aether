<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
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
	import { appIcon } from '$lib/components/apps/icons';
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
		class="flex h-[min(92vh,56rem)] w-[min(96rem,calc(100%-2rem))] max-w-[min(96rem,calc(100%-2rem))] flex-col gap-4 overflow-hidden sm:max-w-[min(96rem,calc(100%-2rem))]"
	>
		<Dialog.Header class="shrink-0 pr-10">
			<Dialog.Title>Install into organization</Dialog.Title>
			<Dialog.Description>
				Select a workspace to install every plugin in that group, or pick apps individually.
				Dependencies are installed automatically.
			</Dialog.Description>
		</Dialog.Header>

		{#if results}
			<ul class="min-h-0 flex-1 space-y-2 overflow-auto text-sm">
				{#each results as outcome (outcome.organization)}
					<li class="rounded-md border border-border p-3">
						<p class="font-medium">{orgName(outcome.organization)}</p>
						{#if outcome.error}
							<p class="text-destructive" role="alert">{outcome.error}</p>
						{:else}
							{#if outcome.installed.length > 0}
								<p class="text-muted-foreground">Installed: {outcome.installed.join(', ')}</p>
							{/if}
							{#if outcome.upgraded.length > 0}
								<p class="text-muted-foreground">Upgraded: {outcome.upgraded.join(', ')}</p>
							{/if}
							{#if outcome.already_installed.length > 0}
								<p class="text-muted-foreground">
									Already there: {outcome.already_installed.join(', ')}
								</p>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
			<Dialog.Footer class="shrink-0">
				<Button onclick={() => (open = false)}>Done</Button>
			</Dialog.Footer>
		{:else}
			<div class="relative w-full shrink-0 sm:max-w-md">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					bind:value={query}
					type="search"
					placeholder="Search workspaces and plugins…"
					aria-label="Search workspaces and plugins"
					class="pl-9"
				/>
			</div>

			<div class="grid min-h-0 flex-1 gap-6 lg:grid-cols-[minmax(0,1fr)_16rem]">
				<section class="flex min-h-0 flex-col gap-2">
					<div class="flex shrink-0 items-center justify-between gap-2">
						<h3 class="text-sm font-semibold">Workspaces &amp; apps</h3>
						{#if chosenApps.length > 0}
							<p class="text-xs text-muted-foreground">
								{chosenApps.length} selected
								{#if chosenWorkspaces.length > 0}
									· {chosenWorkspaces.length} workspace{chosenWorkspaces.length === 1
										? ''
										: 's'}
								{/if}
							</p>
						{/if}
					</div>
					<ul
						class="min-h-0 flex-1 space-y-2 overflow-auto rounded-md border border-border p-2"
					>
						{#if catalog === null}
							<li class="p-3 text-sm text-muted-foreground">Loading…</li>
						{:else if loadError}
							<li class="p-3 text-sm text-destructive" role="alert">{loadError}</li>
						{:else if workspaces.length === 0}
							<li class="p-3 text-sm text-muted-foreground">
								{#if query.trim()}
									No plugins match “{query.trim()}”.
								{:else}
									No plugins are loaded. Load with <code>aether --load-plugins</code>.
								{/if}
							</li>
						{:else}
							{#each workspaces as ws (ws.label)}
								{@const state = workspaceState(ws.label)}
								<li class="rounded-md border border-border/60">
									<label
										class="flex cursor-pointer items-center gap-2.5 rounded-t-md bg-muted/30 px-3 py-2.5 hover:bg-accent"
									>
										<input
											type="checkbox"
											class="rounded-sm"
											checked={state === 'all'}
											indeterminate={state === 'some'}
											onchange={() => toggleWorkspace(ws.label)}
										/>
										<span class="min-w-0 flex-1 text-sm font-medium">{ws.label}</span>
										<span class="text-xs text-muted-foreground tabular-nums">
											{ws.plugins.length}
											{query.trim()
												? `shown · ${(workspaceAllKeys.get(ws.label) ?? []).length} total`
												: `plugin${ws.plugins.length === 1 ? '' : 's'}`}
										</span>
									</label>
									<ul
										class="grid grid-cols-[repeat(auto-fill,minmax(13rem,1fr))] gap-1.5 p-2"
									>
										{#each ws.plugins as group (group.name)}
											{@const plugin =
												group.versions.find(
													(v) => v.version === (versionByName[group.name] ?? group.latest.version)
												) ?? group.latest}
											{@const pluginKey = key(plugin)}
											{@const Icon = appIcon(plugin.icon)}
											<li>
												<label
													class="flex h-full cursor-pointer items-start gap-2 rounded-md border border-transparent p-2 hover:border-border hover:bg-accent {chosenApps.includes(
														pluginKey
													)
														? 'bg-accent/60'
														: ''}"
												>
													<input
														type="checkbox"
														class="mt-1 rounded-sm"
														checked={chosenApps.includes(pluginKey)}
														onchange={() => togglePlugin(pluginKey, ws.label)}
													/>
													<span
														class="flex size-9 shrink-0 items-center justify-center overflow-hidden bg-primary/10 text-primary ring-1 ring-primary/15"
														aria-hidden="true"
													>
														{#if plugin.logo_url}
															<img
																src={plugin.logo_url}
																alt=""
																class="size-7 object-contain"
															/>
														{:else if Icon}
															<Icon class="size-4" />
														{:else}
															<span class="text-xs font-semibold"
																>{plugin.label.charAt(0).toUpperCase()}</span
															>
														{/if}
													</span>
													<span class="min-w-0 flex-1 text-sm">
														<span class="block truncate font-medium">{plugin.label}</span>
														<span class="block truncate text-xs text-muted-foreground">
															{plugin.name}
														</span>
														{#if group.versions.length > 1}
															<select
																class="mt-1 w-full rounded-sm border border-border bg-background px-1 py-0.5 font-mono text-xs"
																value={plugin.version}
																aria-label={`Version of ${plugin.label}`}
																onclick={(e) => e.stopPropagation()}
																onchange={(e) => {
																	e.stopPropagation();
																	setVersion(
																		group.name,
																		(e.currentTarget as HTMLSelectElement).value
																	);
																}}
															>
																{#each group.versions as version (version.version)}
																	{@const when = formatCatalogDate(version.date_created)}
																	<option value={version.version}>
																		v{version.version}{when ? ` · ${when}` : ''}
																	</option>
																{/each}
															</select>
														{:else}
															<span class="block truncate font-mono text-xs text-muted-foreground">
																v{plugin.version}
															</span>
														{/if}
													</span>
												</label>
											</li>
										{/each}
									</ul>
								</li>
							{/each}
						{/if}
					</ul>
				</section>

				<section class="flex min-h-0 flex-col">
					<OrgPicker orgs={organizations} bind:selected={chosenOrgs} />
				</section>
			</div>

			{#if failed}
				<p class="shrink-0 text-sm text-destructive" role="alert">{failed}</p>
			{/if}
			<Dialog.Footer class="shrink-0">
				<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
				<Button
					disabled={working || chosenApps.length === 0 || chosenOrgs.length === 0}
					onclick={install}
				>
					{working
						? 'Installing…'
						: chosenApps.length > 0
							? `Install (${chosenApps.length})`
							: 'Install'}
				</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
