<script lang="ts">
	import PackageOpenIcon from '@lucide/svelte/icons/package-open';
	import SearchXIcon from '@lucide/svelte/icons/search-x';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { Empty } from '$lib/components/ui/empty';
	import AppTile from '$lib/components/apps/AppTile.svelte';
	import CategoryAppsModal from '$lib/components/apps/CategoryAppsModal.svelte';
	import CategoryFolderTile from '$lib/components/apps/CategoryFolderTile.svelte';
	import type { AppCategory, AppTileData } from '$lib/components/apps/types';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { buildHomeEntries } from '$lib/apps/homeEntries';
	import { authSession } from '$lib/auth/session.svelte';
	import { categoryLabel } from '$lib/apps/categories';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { resolve } from '$app/paths';

	let query = $state('');
	let openFolderId = $state<string | null>(null);
	let modalOpen = $state(false);

	const apps = $derived(appsStore.apps ?? []);

	const homeEntries = $derived(buildHomeEntries(apps, appsStore.categories));

	const searching = $derived(query.trim().length > 0);

	const searchHits = $derived(
		apps.filter((app) => {
			const group = categoryLabel(app.category);
			const folder = app.folder?.trim() ?? '';
			return `${app.label} ${app.description ?? ''} ${group} ${folder}`
				.toLowerCase()
				.includes(query.trim().toLowerCase());
		})
	);

	const openFolder = $derived.by(() => {
		if (!openFolderId) return null;
		const entry = homeEntries.find((e) => e.kind === 'folder' && e.id === openFolderId);
		if (!entry || entry.kind !== 'folder') return null;
		const category: AppCategory = {
			id: entry.id,
			label: entry.label,
			icon: entry.icon,
			logo_url: entry.logo_url
		};
		return { category, apps: entry.apps as AppTileData[] };
	});

	// Reload whenever the organization changes.
	$effect(() => {
		void orgStore.current;
		void appsStore.load();
		openFolderId = null;
		modalOpen = false;
		query = '';
	});

	$effect(() => {
		if (!modalOpen) openFolderId = null;
	});

	function openFolderModal(id: string) {
		openFolderId = id;
		modalOpen = true;
		query = '';
	}
</script>

<div class="mx-auto flex w-full max-w-6xl flex-1 flex-col gap-6 px-4 py-6 md:px-8">
	<header class="flex flex-wrap items-center justify-center gap-3 sm:justify-between">
		<div class="min-w-0 text-center sm:text-left">
			<h1 class="text-xl font-semibold tracking-tight">Apps</h1>
			<p class="text-sm text-muted-foreground">
				Open an app. Related modules share a folder; small modules join the host sidebar. Plugins
				outside a workspace stay standalone.
			</p>
		</div>
		<div class="flex w-full flex-wrap items-center justify-center gap-2 sm:w-auto sm:justify-end">
			<div class="relative w-full max-w-xs">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					bind:value={query}
					type="search"
					placeholder="Search apps…"
					aria-label="Search apps"
					class="pl-9"
				/>
			</div>
			{#if authSession.isDeveloper}
				<Button size="sm" variant="outline" href={resolve('/desk/apps')}>Manage apps</Button>
			{/if}
		</div>
	</header>

	<main class="flex min-w-0 flex-1 flex-col items-center">
		{#if appsStore.apps === null}
			<ul
				class="grid w-full grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2 sm:grid-cols-[repeat(auto-fill,minmax(10rem,1fr))]"
			>
				{#each Array(8) as _, i (i)}
					<li><Skeleton class="h-36 w-full rounded-xl" /></li>
				{/each}
			</ul>
		{:else if appsStore.problem === 'failed'}
			<p class="text-sm text-destructive" role="alert">The apps could not be loaded.</p>
		{:else if apps.length === 0}
			<Empty
				icon={PackageOpenIcon}
				title="No apps yet"
				description={authSession.isDeveloper
					? 'Install apps for this organization from the desk.'
					: 'Ask an administrator to install apps for this organization.'}
			>
				{#if authSession.isDeveloper}
					<Button size="sm" href={resolve('/desk/apps')}>Manage apps</Button>
				{/if}
			</Empty>
		{:else if searching}
			{#if searchHits.length === 0}
				<Empty icon={SearchXIcon} title="No matching apps" description="Try a different search." />
			{:else}
				<ul
					class="grid w-full grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2 sm:grid-cols-[repeat(auto-fill,minmax(10rem,1fr))]"
				>
					{#each searchHits as app (app.plugin)}
						<li><AppTile {app} /></li>
					{/each}
				</ul>
			{/if}
		{:else}
			<ul
				class="grid w-full grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2 sm:grid-cols-[repeat(auto-fill,minmax(10rem,1fr))]"
			>
				{#each homeEntries as entry (entry.kind === 'app' ? entry.app.plugin : entry.id)}
					<li>
						{#if entry.kind === 'app'}
							<AppTile app={entry.app} />
						{:else}
							<CategoryFolderTile
								category={{
									id: entry.id,
									label: entry.label,
									icon: entry.icon,
									logo_url: entry.logo_url
								}}
								count={entry.apps.length}
								apps={entry.apps}
								onclick={() => openFolderModal(entry.id)}
							/>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</main>
</div>

<CategoryAppsModal
	bind:open={modalOpen}
	category={openFolder?.category ?? null}
	apps={openFolder?.apps ?? []}
/>
