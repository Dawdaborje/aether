<script lang="ts">
	import { onMount } from 'svelte';
	import PackageOpenIcon from '@lucide/svelte/icons/package-open';
	import SearchXIcon from '@lucide/svelte/icons/search-x';
	import { Empty } from '$lib/components/ui/empty';
	import SearchIcon from '@lucide/svelte/icons/search';
	import AppTile from '$lib/components/apps/AppTile.svelte';
	import InstallAppsModal from '$lib/components/apps/InstallAppsModal.svelte';
	import type { AppTileData } from '$lib/components/apps/types';
	import { Button } from '$lib/components/ui/button';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { categoryLabel } from '$lib/apps/categories';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { currentSystemNavParent } from '$lib/systemStore';

	let query = $state('');
	let category = $state<string | null>(null);
	let installing = $state(false);

	type Entry = AppTileData & { group: string };

	const entries = $derived<Entry[]>(
		(appsStore.apps ?? []).map((app) => ({ ...app, group: categoryLabel(app.category) }))
	);

	const categories = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const entry of entries) counts.set(entry.group, (counts.get(entry.group) ?? 0) + 1);
		return [...counts].sort(([a], [b]) => a.localeCompare(b));
	});

	// A category that no longer exists (an organization switch) falls back to all.
	const selected = $derived(categories.some(([name]) => name === category) ? category : null);

	const shown = $derived(
		entries.filter(
			(entry) =>
				(selected === null || entry.group === selected) &&
				`${entry.label} ${entry.description ?? ''}`
					.toLowerCase()
					.includes(query.trim().toLowerCase())
		)
	);

	onMount(() => {
		currentSystemNavParent.set('apps');
	});

	// Reload whenever the organization changes.
	$effect(() => {
		void orgStore.current;
		void appsStore.load();
	});
</script>

<div class="mx-auto flex w-full max-w-[90rem] flex-1 flex-col gap-4 px-4 py-4 md:px-6">
	<header class="flex flex-wrap items-center gap-3">
		<div class="min-w-0">
			<h1 class="text-lg font-semibold tracking-tight">Apps</h1>
			{#if orgStore.currentName}
				<p class="text-xs text-muted-foreground">{orgStore.currentName}</p>
			{/if}
		</div>
		<div class="relative ml-auto w-full max-w-xs">
			<SearchIcon
				class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
			/>
			<input
				bind:value={query}
				type="search"
				placeholder="Search apps…"
				aria-label="Search apps"
				class="h-9 w-full rounded-md border border-border bg-card pr-3 pl-9 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/20"
			/>
		</div>
		{#if authSession.isDeveloper}
			<Button onclick={() => (installing = true)}>Install apps</Button>
		{/if}
	</header>

	{#if categories.length > 0}
		<nav class="flex gap-1.5 overflow-x-auto md:hidden" aria-label="Categories">
			{#each [null, ...categories.map(([name]) => name)] as name (name ?? 'all')}
				<button
					type="button"
					onclick={() => (category = name)}
					aria-current={selected === name ? 'true' : undefined}
					class="shrink-0 rounded-full border border-border px-3 py-1 text-xs aria-[current=true]:bg-accent aria-[current=true]:font-medium"
				>
					{name ?? 'All'}
				</button>
			{/each}
		</nav>
	{/if}

	<div class="flex min-h-0 flex-1 gap-6">
		{#if categories.length > 0}
			<aside class="hidden w-52 shrink-0 md:block" aria-label="Categories">
				<nav class="sticky top-16 space-y-0.5">
					{#each [null, ...categories.map(([name]) => name)] as name (name ?? 'all')}
						<button
							type="button"
							onclick={() => (category = name)}
							aria-current={selected === name ? 'true' : undefined}
							class="flex w-full items-center rounded-md px-3 py-1.5 text-left text-sm transition hover:bg-accent aria-[current=true]:bg-accent aria-[current=true]:font-medium aria-[current=true]:text-accent-foreground"
						>
							<span class="truncate">{name ?? 'All'}</span>
						</button>
					{/each}
				</nav>
			</aside>
		{/if}

		<main class="min-w-0 flex-1">
			{#if appsStore.apps === null}
				<p class="text-sm text-muted-foreground">Loading apps…</p>
			{:else if appsStore.problem === 'failed'}
				<p class="text-sm text-destructive" role="alert">The apps could not be loaded.</p>
			{:else if entries.length === 0}
				<Empty
					icon={PackageOpenIcon}
					title="No apps loaded in"
					description="Loaded apps will show up here."
				/>
			{:else if shown.length === 0}
				<Empty
					icon={SearchXIcon}
					title="No matching apps"
					description="Try a different search or category."
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
</div>

<InstallAppsModal bind:open={installing} />
