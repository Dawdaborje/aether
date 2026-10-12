<script lang="ts">
	import { onMount } from 'svelte';
	import PlugIcon from '@lucide/svelte/icons/plug';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Empty } from '$lib/components/ui/empty';
	import { Input } from '$lib/components/ui/input';
	import { Skeleton } from '$lib/components/ui/skeleton';
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
				<Input
					bind:value={query}
					type="search"
					placeholder="Search bridges…"
					aria-label="Search bridges"
					class="pl-9"
				/>
			</div>
		{/if}
	</div>

	{#if bridges === null}
		<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
			{#each Array(6) as _, i (i)}
				<Skeleton class="h-24 w-full" />
			{/each}
		</div>
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
								<li>
									<Card.Root size="sm" class="h-full">
										<Card.Content class="flex items-start gap-3 px-(--card-spacing) pt-(--card-spacing)">
											<span
												class="flex size-9 shrink-0 items-center justify-center bg-primary/10 text-sm font-semibold text-primary ring-1 ring-primary/15"
												aria-hidden="true">{bridge.label.charAt(0).toUpperCase()}</span
											>
											<div class="min-w-0 flex-1 space-y-1">
												<div class="flex flex-wrap items-center gap-2">
													<Card.Title class="truncate text-sm font-medium">{bridge.label}</Card.Title>
													<Badge variant={bridge.enabled_globally ? 'default' : 'secondary'}>
														{bridge.enabled_globally ? 'Enabled' : 'Not enabled'}
													</Badge>
												</div>
												{#if bridge.description && bridge.description !== bridge.label}
													<Card.Description class="line-clamp-2 text-xs">
														{bridge.description}
													</Card.Description>
												{/if}
												<p class="font-mono text-[11px] text-muted-foreground">
													{bridge.feature_key}{bridge.is_builtin ? ' · built-in' : ''}{bridge.version
														? ` · v${bridge.version}`
														: ''}
												</p>
											</div>
										</Card.Content>
									</Card.Root>
								</li>
							{/each}
						</ul>
					</section>
				{:else}
					<p class="text-sm text-muted-foreground">No bridge matches your search.</p>
				{/each}
			</main>

			<aside class="hidden w-56 shrink-0 md:block" aria-label="Categories">
				<nav class="sticky top-16 flex flex-col gap-0.5">
					<h2 class="px-3 pb-1 text-xs font-medium text-muted-foreground">Categories</h2>
					{#each [[null, bridges.length] as const, ...categories] as [name, count] (name ?? 'all')}
						<Button
							type="button"
							variant={category === name ? 'secondary' : 'ghost'}
							size="sm"
							onclick={() => (category = name)}
							aria-current={category === name ? 'true' : undefined}
							class="w-full justify-between"
						>
							<span class="truncate">{name === null ? 'All bridges' : categoryLabel(name)}</span>
							<span class="text-xs text-muted-foreground">{count}</span>
						</Button>
					{/each}
				</nav>
			</aside>
		</div>
	{/if}
</div>
