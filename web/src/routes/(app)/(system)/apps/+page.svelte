<script lang="ts">
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import PackageOpenIcon from '@lucide/svelte/icons/package-open';
	import SearchXIcon from '@lucide/svelte/icons/search-x';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { Empty } from '$lib/components/ui/empty';
	import AppTile from '$lib/components/apps/AppTile.svelte';
	import CategoryFolderTile from '$lib/components/apps/CategoryFolderTile.svelte';
	import type { AppTileData } from '$lib/components/apps/types';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { categoryLabel } from '$lib/apps/categories';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { resolve } from '$app/paths';

	let query = $state('');
	let openCategory = $state<string | null>(null);

	type Entry = AppTileData & { group: string };

	const entries = $derived<Entry[]>(
		(appsStore.apps ?? []).map((app) => ({ ...app, group: categoryLabel(app.category) }))
	);

	const categories = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const entry of entries) counts.set(entry.group, (counts.get(entry.group) ?? 0) + 1);
		return [...counts]
			.sort(([a], [b]) => a.localeCompare(b))
			.map(([label, count]) => ({ label, count }));
	});

	// A category that no longer exists (an organization switch) falls back to folders.
	const folder = $derived(
		categories.some((category) => category.label === openCategory) ? openCategory : null
	);

	const searching = $derived(query.trim().length > 0);

	const shown = $derived(
		entries.filter((entry) => {
			if (folder && entry.group !== folder) return false;
			if (!searching) return Boolean(folder);
			return `${entry.label} ${entry.description ?? ''} ${entry.group}`
				.toLowerCase()
				.includes(query.trim().toLowerCase());
		})
	);

	const showFolders = $derived(!folder && !searching && categories.length > 0);

	// Reload whenever the organization changes.
	$effect(() => {
		void orgStore.current;
		void appsStore.load();
		openCategory = null;
		query = '';
	});

	function openFolder(label: string) {
		openCategory = label;
		query = '';
	}

	function closeFolder() {
		openCategory = null;
	}
</script>

<div class="flex w-full flex-1 flex-col gap-4 px-4 py-4 md:px-8">
	<header class="flex flex-wrap items-center gap-3">
		{#if folder}
			<div class="flex min-w-0 items-center gap-1">
				<Button type="button" size="sm" variant="ghost" class="-ml-2 gap-1" onclick={closeFolder}>
					<ChevronLeftIcon class="size-4" />
					Apps
				</Button>
				<span class="text-muted-foreground" aria-hidden="true">/</span>
				<h1 class="truncate text-lg font-semibold tracking-tight">{folder}</h1>
			</div>
		{:else if searching}
			<h1 class="text-lg font-semibold tracking-tight">Search</h1>
		{/if}
		<div class="relative ml-auto w-full max-w-xs">
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
	</header>

	<main class="min-w-0 flex-1">
		{#if appsStore.apps === null}
			<div class="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-2">
				{#each Array(8) as _, i (i)}
					<Skeleton class="h-36 w-full" />
				{/each}
			</div>
		{:else if appsStore.problem === 'failed'}
			<p class="text-sm text-destructive" role="alert">The apps could not be loaded.</p>
		{:else if entries.length === 0}
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
		{:else if showFolders}
			<ul class="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-2">
				{#each categories as category (category.label)}
					<li>
						<CategoryFolderTile
							label={category.label}
							count={category.count}
							onclick={() => openFolder(category.label)}
						/>
					</li>
				{/each}
			</ul>
		{:else if shown.length === 0}
			<Empty
				icon={SearchXIcon}
				title="No matching apps"
				description={searching
					? 'Try a different search.'
					: 'This category has no apps.'}
			/>
		{:else}
			<ul class="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-2">
				{#each shown as app (app.plugin)}
					<li><AppTile {app} /></li>
				{/each}
			</ul>
		{/if}
	</main>
</div>
