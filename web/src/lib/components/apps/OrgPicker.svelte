<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';
	import XIcon from '@lucide/svelte/icons/x';
	import { Input } from '$lib/components/ui/input';

	interface OrgOption {
		db_name: string;
		name: string;
	}

	let {
		orgs,
		selected = $bindable([]),
		heading = 'Organizations',
		emptyText = 'There are no organizations yet.'
	}: {
		orgs: OrgOption[];
		selected?: string[];
		heading?: string;
		emptyText?: string;
	} = $props();

	const PAGE = 100;
	let query = $state('');
	let limit = $state(PAGE);

	const chosen = $derived(new Set(selected));
	const byDb = $derived(new Map(orgs.map((org) => [org.db_name, org])));
	const matches = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return orgs;
		return orgs.filter(
			(org) => org.name.toLowerCase().includes(q) || org.db_name.toLowerCase().includes(q)
		);
	});
	const shown = $derived(matches.slice(0, limit));
	const allMatchesChosen = $derived(
		matches.length > 0 && matches.every((org) => chosen.has(org.db_name))
	);

	$effect(() => {
		query;
		limit = PAGE;
	});

	function toggle(db: string) {
		selected = chosen.has(db) ? selected.filter((item) => item !== db) : [...selected, db];
	}

	/** Add (or remove) every organization matching the current search. */
	function toggleMatches() {
		const keys = new Set(matches.map((org) => org.db_name));
		selected = allMatchesChosen
			? selected.filter((db) => !keys.has(db))
			: [...new Set([...selected, ...keys])];
	}
</script>

<div class="flex min-h-0 flex-1 flex-col gap-2">
	<div class="flex shrink-0 items-center justify-between gap-2">
		<h3 class="text-sm font-semibold">{heading}</h3>
		<p class="text-xs text-muted-foreground tabular-nums">
			{selected.length} of {orgs.length} selected
		</p>
	</div>

	<div class="relative shrink-0">
		<SearchIcon
			class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
		/>
		<Input
			bind:value={query}
			type="search"
			placeholder="Search organizations…"
			aria-label="Search organizations"
			class="pl-9"
		/>
	</div>

	{#if selected.length > 0}
		<ul class="flex max-h-24 shrink-0 flex-wrap gap-1 overflow-auto" aria-label="Selected organizations">
			{#each selected as db (db)}
				<li>
					<button
						type="button"
						class="flex items-center gap-1 rounded-full bg-primary/10 py-0.5 pr-1 pl-2 text-xs text-primary hover:bg-primary/20"
						onclick={() => toggle(db)}
						aria-label={`Remove ${byDb.get(db)?.name ?? db}`}
					>
						<span class="max-w-40 truncate">{byDb.get(db)?.name ?? db}</span>
						<XIcon class="size-3" />
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	{#if orgs.length > 0}
		<div class="flex shrink-0 items-center justify-between text-xs">
			<button type="button" class="text-primary hover:underline" onclick={toggleMatches}>
				{allMatchesChosen ? 'Deselect' : 'Select'}
				{query.trim() ? `${matches.length} matching` : 'all'}
			</button>
			{#if selected.length > 0}
				<button
					type="button"
					class="text-muted-foreground hover:underline"
					onclick={() => (selected = [])}
				>
					Clear
				</button>
			{/if}
		</div>
	{/if}

	<ul class="min-h-0 flex-1 space-y-0.5 overflow-auto rounded-md border border-border p-1.5">
		{#if orgs.length === 0}
			<li class="p-2 text-sm text-muted-foreground">{emptyText}</li>
		{:else if matches.length === 0}
			<li class="p-2 text-sm text-muted-foreground">No organizations match “{query.trim()}”.</li>
		{:else}
			{#each shown as org (org.db_name)}
				<li>
					<label class="flex cursor-pointer items-center gap-2.5 rounded-md p-2 hover:bg-accent">
						<input
							type="checkbox"
							class="rounded-sm"
							checked={chosen.has(org.db_name)}
							onchange={() => toggle(org.db_name)}
						/>
						<span class="min-w-0">
							<span class="block truncate text-sm font-medium">{org.name}</span>
							<span class="block truncate text-xs text-muted-foreground">{org.db_name}</span>
						</span>
					</label>
				</li>
			{/each}
			{#if matches.length > shown.length}
				<li class="p-1">
					<button
						type="button"
						class="w-full rounded-md p-2 text-xs text-primary hover:bg-accent"
						onclick={() => (limit += PAGE)}
					>
						Show more ({matches.length - shown.length} hidden — refine the search)
					</button>
				</li>
			{/if}
		{/if}
	</ul>
</div>
