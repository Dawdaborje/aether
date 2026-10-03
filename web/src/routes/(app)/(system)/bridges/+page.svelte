<script lang="ts">
	import { onMount } from 'svelte';
	import PlugIcon from '@lucide/svelte/icons/plug';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { Empty } from '$lib/components/ui/empty';
	import { categoryLabel } from '$lib/apps/categories';
	import { fetchBridges, type Bridge } from '$lib/bridges/api';

	let bridges = $state<Bridge[] | null>(null);
	let failed = $state<string | null>(null);
	let query = $state('');
	let category = $state<string | null>(null);

	onMount(async () => {
		try {
			bridges = await fetchBridges();
		} catch (error) {
			failed = error instanceof Error ? error.message : 'The bridges could not be loaded.';
			bridges = [];
		}
	});

	const categories = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const bridge of bridges ?? []) {
			counts.set(bridge.category, (counts.get(bridge.category) ?? 0) + 1);
		}
		return [...counts].sort(([a], [b]) => a.localeCompare(b));
	});

	const shown = $derived(
		(bridges ?? []).filter(
			(bridge) =>
				(category === null || bridge.category === category) &&
				`${bridge.label} ${bridge.description ?? ''}`
					.toLowerCase()
					.includes(query.trim().toLowerCase())
		)
	);

	const grouped = $derived.by(() => {
		const groups = new Map<string, Bridge[]>();
		for (const bridge of shown) groups.set(bridge.category, [...(groups.get(bridge.category) ?? []), bridge]);
		return [...groups];
	});
</script>

<ControlPanel crumbs={[{ label: 'Bridges' }]} />
<div class="flex w-full flex-1 flex-col gap-4 px-4 py-4 md:px-6">
	<div class="flex flex-wrap items-center gap-3">
		<p class="min-w-0 flex-1 text-sm text-muted-foreground">
			Bridges connect Aether to third-party tools: payments, messaging, identity, storage and more.
		</p>
		{#if bridges && bridges.length > 0}
			<div class="relative w-full max-w-xs">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<input
					bind:value={query}
					type="search"
					placeholder="Search bridges…"
					aria-label="Search bridges"
					class="h-9 w-full rounded-md border border-border bg-card pr-3 pl-9 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/20"
				/>
			</div>
		{/if}
	</div>

	{#if bridges === null}
		<p class="text-sm text-muted-foreground">Loading bridges…</p>
	{:else if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{:else if bridges.length === 0}
		<Empty
			icon={PlugIcon}
			title="No bridges loaded in"
			description="Run aether --seed to load the bridge catalog."
		/>
	{:else}
		<div class="flex min-h-0 flex-1 gap-6">
			<main class="min-w-0 flex-1 space-y-6">
				{#each grouped as [group, items] (group)}
					<section class="space-y-2">
						<h2 class="text-sm font-semibold">{categoryLabel(group)}</h2>
						<ul class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
							{#each items as bridge (bridge.feature_key)}
								<li class="flex items-start gap-3 rounded-lg border border-border bg-card p-3">
									<span
										class="flex size-9 shrink-0 items-center justify-center rounded-lg border border-primary/15 bg-primary/10 text-sm font-semibold text-primary"
										aria-hidden="true">{bridge.label.charAt(0).toUpperCase()}</span
									>
									<div class="min-w-0 flex-1">
										<div class="flex items-center gap-2">
											<p class="truncate text-sm font-medium">{bridge.label}</p>
											<span
												class="rounded-full px-2 py-0.5 text-[11px] font-medium {bridge.enabled_globally
													? 'bg-primary/10 text-primary'
													: 'bg-muted text-muted-foreground'}"
											>
												{bridge.enabled_globally ? 'Enabled' : 'Not enabled'}
											</span>
										</div>
										{#if bridge.description && bridge.description !== bridge.label}
											<p class="line-clamp-2 text-xs text-muted-foreground">{bridge.description}</p>
										{/if}
										<p class="mt-1 font-mono text-[11px] text-muted-foreground">
											{bridge.feature_key}{bridge.is_builtin ? ' · built-in' : ''}{bridge.version
												? ` · v${bridge.version}`
												: ''}
										</p>
									</div>
								</li>
							{/each}
						</ul>
					</section>
				{:else}
					<p class="text-sm text-muted-foreground">No bridge matches your search.</p>
				{/each}
			</main>

			<aside class="hidden w-56 shrink-0 md:block" aria-label="Categories">
				<nav class="sticky top-16 space-y-0.5">
					<h2 class="px-3 pb-1 text-xs font-medium text-muted-foreground">Categories</h2>
					{#each [[null, bridges.length] as const, ...categories] as [name, count] (name ?? 'all')}
						<button
							type="button"
							onclick={() => (category = name)}
							aria-current={category === name ? 'true' : undefined}
							class="flex w-full items-center justify-between rounded-md px-3 py-1.5 text-left text-sm transition hover:bg-accent aria-[current=true]:bg-accent aria-[current=true]:font-medium aria-[current=true]:text-accent-foreground"
						>
							<span class="truncate">{name === null ? 'All bridges' : categoryLabel(name)}</span>
							<span class="text-xs text-muted-foreground">{count}</span>
						</button>
					{/each}
				</nav>
			</aside>
		</div>
	{/if}
</div>
