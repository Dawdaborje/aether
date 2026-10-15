<script lang="ts">
	import { onMount } from 'svelte';
	import PackageOpenIcon from '@lucide/svelte/icons/package-open';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { Empty } from '$lib/components/ui/empty';
	import AppTile from '$lib/components/apps/AppTile.svelte';
	import InstallAppsModal from '$lib/components/apps/InstallAppsModal.svelte';
	import UpgradeAppsModal from '$lib/components/apps/UpgradeAppsModal.svelte';
	import PluginDetailModal from '$lib/components/apps/PluginDetailModal.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { OTHER_CATEGORY, categoryLabel } from '$lib/apps/categories';
	import {
		fetchCatalog,
		reloadCatalog,
		uploadCatalogZip,
		type CatalogPlugin
	} from '$lib/apps/catalogApi';
	import {
		groupCatalogByName,
		type CatalogPluginGroup
	} from '$lib/apps/catalogGroup';
	import { matchPlugin } from '$lib/apps/matchPlugin';
	import { currentSystemNavParent } from '$lib/systemStore';
	import { resolve } from '$app/paths';
	import type { AppTileData } from '$lib/components/apps/types';

	let installing = $state(false);
	let installSeed = $state<string[]>([]);
	let installWorkspaces = $state<string[]>([]);
	let catalog = $state<CatalogPlugin[] | null>(null);
	let loadError = $state(false);
	/** Category keys currently checked. Empty = show every plugin (Odoo-style default). */
	let selectedCategories = $state<string[]>([]);
	let query = $state('');
	let detailVersions = $state<CatalogPlugin[]>([]);
	let detailOpen = $state(false);
	let reloading = $state(false);
	let uploading = $state(false);
	let actionMessage = $state<string | null>(null);
	let actionError = $state<string | null>(null);
	let zipInput: HTMLInputElement | undefined = $state();

	onMount(() => {
		currentSystemNavParent.set('apps');
		void load();
	});

	async function load() {
		catalog = null;
		loadError = false;
		try {
			catalog = await fetchCatalog();
		} catch {
			loadError = true;
			catalog = [];
		}
	}

	function summarizeLoad(
		loaded: { name: string; created: boolean }[],
		applied: { ok: boolean; plugin: string; organization: string; error: string | null }[] = []
	): string {
		const created = loaded.filter((row) => row.created).length;
		const unchanged = loaded.length - created;
		const parts: string[] = [];
		if (created) parts.push(`${created} new revision${created === 1 ? '' : 's'}`);
		if (unchanged) parts.push(`${unchanged} unchanged`);
		if (parts.length === 0) return 'No packages loaded.';
		let text = `Loaded ${loaded.length} package${loaded.length === 1 ? '' : 's'} (${parts.join(', ')}).`;
		const moved = applied.filter((row) => row.ok).length;
		if (moved) text += ` Upgraded ${moved} installed module${moved === 1 ? '' : 's'} in your organizations.`;
		const failed = applied.filter((row) => !row.ok);
		if (failed.length) {
			text += ` Could not upgrade ${failed
				.map((row) => `${row.plugin} in ${row.organization}: ${row.error}`)
				.join('; ')}.`;
		}
		return text;
	}

	async function onReload() {
		reloading = true;
		actionMessage = null;
		actionError = null;
		try {
			const result = await reloadCatalog();
			actionMessage = summarizeLoad(result.loaded, result.applied);
			await load();
		} catch (error) {
			actionError = error instanceof Error ? error.message : 'Could not reload the catalog';
		} finally {
			reloading = false;
		}
	}

	async function onZipChosen(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		uploading = true;
		actionMessage = null;
		actionError = null;
		try {
			const result = await uploadCatalogZip(file);
			actionMessage = summarizeLoad(result.loaded, result.applied);
			await load();
		} catch (error) {
			actionError = error instanceof Error ? error.message : 'Could not upload the zip';
		} finally {
			uploading = false;
		}
	}

	const plugins = $derived(catalog ?? []);
	const groups = $derived(groupCatalogByName(plugins));

	const categories = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const group of groups) {
			const key = categoryLabel(group.latest.category);
			counts.set(key, (counts.get(key) ?? 0) + 1);
		}
		return [...counts.entries()]
			.sort(([a], [b]) => {
				if (a === OTHER_CATEGORY) return 1;
				if (b === OTHER_CATEGORY) return -1;
				return a.localeCompare(b);
			})
			.map(([label, count]) => ({ label, count }));
	});

	const visible = $derived.by(() => {
		const chosen =
			selectedCategories.length === 0 ? null : new Set(selectedCategories);
		return groups.filter((group) => {
			if (chosen && !chosen.has(categoryLabel(group.latest.category))) return false;
			return group.versions.some((plugin) => matchPlugin(plugin, query));
		});
	});

	function toggleCategory(label: string) {
		selectedCategories = selectedCategories.includes(label)
			? selectedCategories.filter((item) => item !== label)
			: [...selectedCategories, label];
	}

	function tile(group: CatalogPluginGroup): AppTileData {
		const plugin = group.latest;
		const desc = plugin.description?.trim() ?? '';
		const versionNote =
			group.versions.length > 1 ? `${group.versions.length} versions` : '';
		return {
			plugin: plugin.name,
			category: plugin.category,
			label: plugin.label,
			icon: plugin.icon,
			logo_url: plugin.logo_url,
			route: '',
			description: desc
				? versionNote
					? `${desc} · ${versionNote}`
					: desc
				: versionNote || null
		};
	}

	function openDetail(group: CatalogPluginGroup) {
		detailVersions = group.versions;
		detailOpen = true;
	}

	let upgrading = $state(false);

	function startInstall(plugin?: CatalogPlugin) {
		installSeed = plugin ? [`${plugin.name}@${plugin.version}`] : [];
		installWorkspaces = [];
		installing = true;
	}

	/** Install every plugin in the given workspaces (or the sidebar filter selection). */
	function startInstallWorkspaces(labels?: string[]) {
		const workspaces = labels ?? selectedCategories;
		if (workspaces.length === 0) {
			startInstall();
			return;
		}
		installSeed = [];
		installWorkspaces = [...workspaces];
		installing = true;
	}
</script>

<div class="flex min-h-0 w-full flex-1 flex-col">
	<header
		class="flex flex-wrap items-end justify-between gap-3 border-b border-border px-4 py-4 md:px-6"
	>
		<div class="min-w-0">
			<h1 class="text-lg font-semibold tracking-tight">Apps</h1>
			<p class="text-sm text-muted-foreground">
				All plugins loaded into the kernel. Filter by category on the left; open a tile for
				details. Members launch installed apps from
				<a class="underline underline-offset-2" href={resolve('/apps')}>the app launcher</a>.
			</p>
		</div>
		<div class="flex w-full flex-wrap items-center justify-end gap-2 sm:w-auto">
			<div class="relative w-full max-w-xs">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					bind:value={query}
					type="search"
					placeholder="Search plugins…"
					aria-label="Search plugins"
					class="pl-9"
				/>
			</div>
			{#if selectedCategories.length > 0}
				<Button size="sm" variant="outline" onclick={() => startInstallWorkspaces()}>
					Install {selectedCategories.length === 1
						? selectedCategories[0]
						: `${selectedCategories.length} workspaces`}
				</Button>
			{/if}
			<input
				bind:this={zipInput}
				type="file"
				accept=".zip,application/zip"
				class="sr-only"
				onchange={onZipChosen}
			/>
			<Button
				size="sm"
				variant="outline"
				disabled={reloading || uploading}
				onclick={() => void onReload()}
			>
				{reloading ? 'Reloading…' : 'Reload from disk'}
			</Button>
			<Button
				size="sm"
				variant="outline"
				disabled={reloading || uploading}
				onclick={() => zipInput?.click()}
			>
				{uploading ? 'Uploading…' : 'Upload zip'}
			</Button>
			<Button size="sm" variant="outline" onclick={() => (upgrading = true)}>Upgrade modules</Button>
			<Button size="sm" onclick={() => startInstall()}>Install into org</Button>
		</div>
		{#if actionMessage}
			<p class="basis-full text-sm text-muted-foreground" role="status">{actionMessage}</p>
		{/if}
		{#if actionError}
			<p class="basis-full text-sm text-destructive" role="alert">{actionError}</p>
		{/if}
	</header>

	<div class="flex min-h-0 flex-1">
		<aside
			class="hidden w-56 shrink-0 flex-col border-r border-border bg-muted/20 md:flex"
			aria-label="Categories"
		>
			<div class="flex items-center justify-between gap-2 px-4 py-3">
				<h2 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
					Categories
				</h2>
				{#if selectedCategories.length > 0}
					<button
						type="button"
						class="text-xs text-primary hover:underline"
						onclick={() => (selectedCategories = [])}
					>
						Clear
					</button>
				{/if}
			</div>
			<ul class="flex-1 space-y-0.5 overflow-y-auto px-2 pb-4">
				{#each categories as category (category.label)}
					<li class="group flex items-center gap-1 rounded-md hover:bg-accent">
						<label class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 px-2 py-1.5 text-sm">
							<input
								type="checkbox"
								class="rounded-sm"
								checked={selectedCategories.includes(category.label)}
								onchange={() => toggleCategory(category.label)}
							/>
							<span class="min-w-0 flex-1 truncate">{category.label}</span>
							<span class="text-xs text-muted-foreground tabular-nums">{category.count}</span>
						</label>
						<button
							type="button"
							class="mr-1 shrink-0 rounded px-1.5 py-0.5 text-xs font-medium text-primary opacity-0 hover:bg-primary/10 group-hover:opacity-100 focus-visible:opacity-100"
							title={`Install all ${category.label} plugins`}
							onclick={() => startInstallWorkspaces([category.label])}
						>
							Install
						</button>
					</li>
				{/each}
				{#if categories.length === 0 && catalog !== null}
					<li class="px-2 py-1.5 text-xs text-muted-foreground">No categories yet.</li>
				{/if}
			</ul>
		</aside>

		<div class="min-w-0 flex-1 overflow-y-auto px-4 py-4 md:px-6">
			<!-- Mobile category filters -->
			<div class="mb-4 flex flex-wrap gap-2 md:hidden">
				{#each categories as category (category.label)}
					<label
						class="inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs {selectedCategories.includes(
							category.label
						)
							? 'bg-accent'
							: ''}"
					>
						<input
							type="checkbox"
							class="rounded-sm"
							checked={selectedCategories.includes(category.label)}
							onchange={() => toggleCategory(category.label)}
						/>
						{category.label}
						<span class="text-muted-foreground">({category.count})</span>
					</label>
				{/each}
			</div>

			{#if catalog === null}
				<div class="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-4">
					{#each Array(12) as _, i (i)}
						<Skeleton class="h-44 w-full" />
					{/each}
				</div>
			{:else if loadError}
				<p class="text-sm text-destructive" role="alert">The catalog could not be loaded.</p>
			{:else if groups.length === 0}
				<Empty
					icon={PackageOpenIcon}
					title="No plugins loaded"
					description="Use Reload from disk (plugin_paths in aether.toml), Upload zip, or aether --load-plugins."
				/>
			{:else if visible.length === 0}
				<p class="text-sm text-muted-foreground">
					No plugins match
					{#if query.trim()}
						“{query.trim()}”
					{:else}
						the selected categories
					{/if}.
					<button
						type="button"
						class="text-primary underline underline-offset-2"
						onclick={() => {
							selectedCategories = [];
							query = '';
						}}
					>
						Clear filters
					</button>
				</p>
			{:else}
				<p class="mb-3 text-xs text-muted-foreground">
					{visible.length} of {groups.length} plugin{groups.length === 1 ? '' : 's'}
					{#if plugins.length !== groups.length}
						· {plugins.length} revisions
					{/if}
					{#if selectedCategories.length > 0 || query.trim()}
						· filtered
					{/if}
				</p>
				<ul class="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-4">
					{#each visible as group (group.name)}
						<li>
							<AppTile app={tile(group)} href={null} onclick={() => openDetail(group)} />
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</div>
</div>

<PluginDetailModal
	bind:open={detailOpen}
	versions={detailVersions}
	onInstall={(plugin) => startInstall(plugin)}
/>
<UpgradeAppsModal bind:open={upgrading} />
<InstallAppsModal
	bind:open={installing}
	preselected={installSeed}
	preselectedWorkspaces={installWorkspaces}
/>
