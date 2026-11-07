<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { TableSkeleton, DetailSkeleton } from '$lib/components/ui/skeleton';
	import { Loading } from '$lib/components/ui/loading';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { loadFailure } from '$lib/errors/report';
	import { onDestroy, onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import { Input } from '$lib/components/ui/input';
	import { NativeSelect, NativeSelectOption } from '$lib/components/ui/native-select';
	import {
		fetchMantleOrg,
		fetchMantleOrgs,
		fetchMantleStats,
		formatBytes,
		type MantleOrgPage,
		type MantleOrgSort,
		type MantleStatsResponse,
		type OrgMantleStats
	} from '$lib/mantle/api';

	const PAGE = 25;

	let stats = $state<MantleStatsResponse | null>(null);
	let failed = $state<string | null>(null);
	let loading = $state(false);

	let page = $state<MantleOrgPage | null>(null);
	let pageError = $state<string | null>(null);
	let query = $state('');
	let sort = $state<MantleOrgSort>('bytes');
	let offset = $state(0);

	let expanded = $state<string | null>(null);
	let detail = $state<OrgMantleStats | null>(null);
	let detailError = $state<string | null>(null);

	let timer: ReturnType<typeof setInterval> | null = null;
	let searchTimer: ReturnType<typeof setTimeout> | null = null;

	async function loadStats() {
		try {
			stats = await fetchMantleStats();
			failed = null;
		} catch (error) {
			failed = loadFailure(error, 'Mantle stats could not be loaded.');
		}
	}

	async function loadOrgs() {
		try {
			page = await fetchMantleOrgs({ q: query, sort, offset, limit: PAGE });
			pageError = null;
		} catch (error) {
			pageError = loadFailure(error, 'The organizations could not be loaded.');
		}
	}

	async function loadDetail(org: string) {
		try {
			const next = await fetchMantleOrg(org);
			if (expanded === org) detail = next;
			detailError = null;
		} catch (error) {
			detailError = loadFailure(error, 'The organization could not be loaded.');
		}
	}

	async function refresh() {
		loading = true;
		await Promise.all([loadStats(), loadOrgs(), expanded ? loadDetail(expanded) : undefined]);
		loading = false;
	}

	onMount(() => {
		void refresh();
		timer = setInterval(() => void refresh(), 10000);
	});

	onDestroy(() => {
		if (timer) clearInterval(timer);
		if (searchTimer) clearTimeout(searchTimer);
	});

	/** Search as you type, but wait for a pause: each search is a request. */
	function onSearch() {
		if (searchTimer) clearTimeout(searchTimer);
		searchTimer = setTimeout(() => {
			offset = 0;
			void loadOrgs();
		}, 250);
	}

	function onSort(value: MantleOrgSort) {
		sort = value;
		offset = 0;
		void loadOrgs();
	}

	function go(next: number) {
		offset = Math.max(0, next);
		void loadOrgs();
	}

	function toggle(org: string) {
		if (expanded === org) {
			expanded = null;
			detail = null;
			return;
		}
		expanded = org;
		detail = null;
		detailError = null;
		void loadDetail(org);
	}

	function percent(used: number, max: number): number {
		return max ? Math.min(100, (used / max) * 100) : 0;
	}

	function hitRate(system: MantleStatsResponse['system']): string {
		const total = system.hits + system.misses;
		return total === 0 ? '—' : `${Math.round((system.hits / total) * 100)}%`;
	}

	const kindEntries = $derived(
		stats ? Object.entries(stats.system.kind_bytes ?? {}).sort((a, b) => b[1] - a[1]) : []
	);
	const kindTotal = $derived(kindEntries.reduce((sum, [, bytes]) => sum + bytes, 0));
	const KIND_SHADES = [
		'bg-primary',
		'bg-primary/70',
		'bg-primary/50',
		'bg-primary/35',
		'bg-primary/20'
	];

	const first = $derived(page && page.total > 0 ? page.offset + 1 : 0);
	const last = $derived(page ? Math.min(page.offset + page.limit, page.total) : 0);
</script>

<div class="flex flex-col gap-4">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<p class="min-w-0 flex-1 text-sm text-muted-foreground">
			The working-set cache: how full it is, how well it is doing, and which organizations use it.
		</p>
		<Button size="sm" variant="outline" onclick={() => void refresh()} disabled={loading}>
			<AppIcon name="refresh" class="size-4 {loading ? 'animate-spin' : ''}" />
			Refresh
		</Button>
	</div>

	{#if failed && !stats}
		<LoadFailed message={failed} onRetry={refresh} retrying={loading} />
	{:else if !stats}
		<DetailSkeleton fields={6} label="Loading Mantle…" />
	{:else}
		{@const system = stats.system}
		{@const used = percent(system.bytes_used, system.bytes_max)}

		<section class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label="Summary">
			<div class="rounded-xl border border-border bg-card p-4">
				<div class="flex items-center justify-between gap-2">
					<p class="text-xs text-muted-foreground">Memory</p>
					<Badge variant="secondary">{system.backend}{system.l2_redis ? ' + L2' : ''}</Badge>
				</div>
				<p class="mt-1 text-2xl font-semibold tabular-nums">{formatBytes(system.bytes_used)}</p>
				<div
					class="mt-2 h-1.5 overflow-hidden rounded-full bg-muted"
					role="progressbar"
					aria-valuenow={Math.round(used)}
					aria-valuemin={0}
					aria-valuemax={100}
					aria-label="Cache memory in use"
				>
					<div
						class="h-full rounded-full {used > 90 ? 'bg-destructive' : 'bg-primary'}"
						style={`width: ${used}%`}
					></div>
				</div>
				<p class="mt-1.5 text-xs text-muted-foreground">
					{Math.round(used)}% of {formatBytes(system.bytes_max)}
					{#if system.bytes_target != null && system.bytes_target !== system.bytes_max}
						· target {formatBytes(system.bytes_target)}
					{/if}
				</p>
			</div>
			<div class="rounded-xl border border-border bg-card p-4">
				<p class="text-xs text-muted-foreground">Hit rate</p>
				<p class="mt-1 text-2xl font-semibold tabular-nums">{hitRate(system)}</p>
				<p class="mt-3 text-xs text-muted-foreground tabular-nums">
					{system.hits.toLocaleString()} hits · {system.misses.toLocaleString()} misses · {system.ghost_hits.toLocaleString()}
					ghost
				</p>
			</div>
			<div class="rounded-xl border border-border bg-card p-4">
				<p class="text-xs text-muted-foreground">Entries</p>
				<p class="mt-1 text-2xl font-semibold tabular-nums">{system.entries.toLocaleString()}</p>
				<p class="mt-3 text-xs text-muted-foreground tabular-nums">
					{system.evictions.toLocaleString()} evictions
					{#if system.session_entries != null}
						· {system.session_entries.toLocaleString()} sessions
					{/if}
				</p>
			</div>
			<div class="rounded-xl border border-border bg-card p-4">
				<p class="text-xs text-muted-foreground">Kernel / plugins</p>
				<p class="mt-1 text-2xl font-semibold tabular-nums">
					{formatBytes(system.kernel_bytes)}
					<span class="text-base font-normal text-muted-foreground">
						/ {formatBytes(system.plugin_bytes)}
					</span>
				</p>
				<p class="mt-3 text-xs text-muted-foreground tabular-nums">
					{#if system.warmup}
						Warm-up: {system.warmup.pending} queued · {system.warmup.filled} filled
					{:else}
						{stats.org_count.toLocaleString()} organizations
					{/if}
				</p>
			</div>
		</section>

		{#if kindEntries.length}
			<section class="rounded-xl border border-border bg-card p-4">
				<h2 class="mb-3 text-sm font-medium">What the cache holds</h2>
				<div class="flex h-2.5 overflow-hidden rounded-full bg-muted">
					{#each kindEntries as [kind, bytes], i (kind)}
						<div
							class={KIND_SHADES[Math.min(i, KIND_SHADES.length - 1)]}
							style={`width: ${kindTotal ? (bytes / kindTotal) * 100 : 0}%`}
							title={`${kind}: ${formatBytes(bytes)}`}
						></div>
					{/each}
				</div>
				<ul class="mt-3 flex flex-wrap gap-x-5 gap-y-1.5 text-xs">
					{#each kindEntries as [kind, bytes], i (kind)}
						<li class="flex items-center gap-1.5">
							<span class="size-2 rounded-full {KIND_SHADES[Math.min(i, KIND_SHADES.length - 1)]}"
							></span>
							<span class="font-medium">{kind}</span>
							<span class="text-muted-foreground tabular-nums">{formatBytes(bytes)}</span>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		<section class="rounded-xl border border-border bg-card p-4">
			<h2 class="mb-3 text-sm font-medium">Warm-set tiers</h2>
			<div class="grid gap-4 md:grid-cols-3">
				{#each [['A', system.tiers.a], ['B', system.tiers.b], ['C', system.tiers.c]] as const as [label, keys] (label)}
					{@const present = keys.filter((key) => key.present).length}
					<div>
						<p class="mb-1.5 flex items-center justify-between text-xs text-muted-foreground">
							<span>Tier {label}</span>
							<span class="tabular-nums">{present}/{keys.length} warm</span>
						</p>
						<ul class="space-y-1">
							{#each keys as key (key.key)}
								<li class="flex items-center gap-2 font-mono text-xs">
									<span
										class="inline-block size-2 shrink-0 rounded-full {key.present
											? 'bg-success'
											: 'bg-warning'}"
										title={key.present ? 'In the cache' : 'Not loaded yet'}
									></span>
									<span class="truncate">{key.key}</span>
								</li>
							{:else}
								<li class="text-xs text-muted-foreground">No tracked keys</li>
							{/each}
						</ul>
					</div>
				{/each}
			</div>
		</section>

		<section class="rounded-xl border border-border bg-card">
			<div class="flex flex-wrap items-center gap-3 border-b border-border p-3">
				<h2 class="text-sm font-medium">
					Organizations
					<span class="ml-1 font-normal text-muted-foreground tabular-nums">
						{stats.org_count.toLocaleString()}
					</span>
				</h2>
				<div class="ml-auto flex flex-wrap items-center gap-2">
					<div class="relative w-56">
						<AppIcon
							name="search"
							class="pointer-events-none absolute top-1/2 left-2.5 z-10 size-4 -translate-y-1/2 text-muted-foreground"
						/>
						<Input
							bind:value={query}
							oninput={onSearch}
							type="search"
							placeholder="Find an organization…"
							aria-label="Find an organization"
							class="pl-8"
						/>
					</div>
					<NativeSelect
						class="w-40"
						aria-label="Sort organizations"
						value={sort}
						onchange={(event) => onSort(event.currentTarget.value as MantleOrgSort)}
					>
						<NativeSelectOption value="bytes">Largest first</NativeSelectOption>
						<NativeSelectOption value="entries">Most entries</NativeSelectOption>
						<NativeSelectOption value="name">Name</NativeSelectOption>
					</NativeSelect>
				</div>
			</div>

			{#if pageError && !page}
				<LoadFailed message={pageError} onRetry={loadOrgs} />
			{:else if !page}
				<TableSkeleton rows={5} columns={3} label="Loading organizations…" class="border-0" />
			{:else if page.orgs.length === 0}
				<p class="px-4 py-8 text-center text-sm text-muted-foreground">
					{#if query.trim()}
						No organization matches “{query.trim()}”.
					{:else}
						No organization has used the cache yet. Traffic fills this in.
					{/if}
				</p>
			{:else}
				<ul class="divide-y divide-border">
					{#each page.orgs as org (org.org)}
						{@const open = expanded === org.org}
						<li>
							<button
								type="button"
								class="flex w-full items-center gap-3 px-4 py-2.5 text-left hover:bg-muted/40"
								aria-expanded={open}
								onclick={() => toggle(org.org)}
							>
								<AppIcon
									name="chevron-down"
									class="size-4 shrink-0 text-muted-foreground transition-transform {open
										? ''
										: '-rotate-90'}"
								/>
								<span class="min-w-0 flex-1 truncate font-mono text-sm">{org.org}</span>
								<span class="shrink-0 text-xs text-muted-foreground tabular-nums">
									{formatBytes(org.bytes)} · {org.entries.toLocaleString()} entries
								</span>
							</button>
							{#if open}
								<div class="border-t border-border bg-muted/20 px-4 py-3">
									{#if detailError}
										<p class="text-sm text-destructive" role="alert">{detailError}</p>
									{:else if !detail}
										<Loading label="Loading…" class="py-4" />
									{:else}
										<div class="grid gap-4 md:grid-cols-2">
											<div>
												<h3
													class="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase"
												>
													Hot plugins
												</h3>
												{#if detail.hot_plugins.length === 0}
													<p class="text-xs text-muted-foreground">Nothing is hot yet.</p>
												{:else}
													<ul class="space-y-1">
														{#each detail.hot_plugins as plugin (plugin.plugin)}
															<li class="flex justify-between gap-2 font-mono text-xs">
																<span class="truncate">{plugin.app ?? plugin.plugin}</span>
																<span class="shrink-0 text-muted-foreground tabular-nums">
																	{plugin.score.toFixed(1)} · {formatBytes(plugin.bytes)}
																</span>
															</li>
														{/each}
													</ul>
												{/if}
											</div>
											<div>
												<h3
													class="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase"
												>
													Hot models
												</h3>
												{#if detail.hot_models.length === 0}
													<p class="text-xs text-muted-foreground">Nothing is hot yet.</p>
												{:else}
													<ul class="space-y-1">
														{#each detail.hot_models as model (`${model.plugin}/${model.model}:${model.kind}`)}
															<li class="flex justify-between gap-2 font-mono text-xs">
																<span class="truncate">
																	{model.plugin}/{model.model}:{model.kind}
																</span>
																<span class="shrink-0 text-muted-foreground tabular-nums">
																	{model.score.toFixed(1)} · {formatBytes(model.bytes)}
																</span>
															</li>
														{/each}
													</ul>
												{/if}
											</div>
										</div>
										{#if detail.heat_updated_at}
											<p class="mt-3 text-xs text-muted-foreground">
												Heat updated {new Date(detail.heat_updated_at).toLocaleString()}
											</p>
										{/if}
									{/if}
								</div>
							{/if}
						</li>
					{/each}
				</ul>
				<div class="flex items-center justify-between gap-2 border-t border-border px-4 py-2.5">
					<p class="text-xs text-muted-foreground tabular-nums">
						{first.toLocaleString()}–{last.toLocaleString()} of {page.total.toLocaleString()}
					</p>
					<div class="flex gap-2">
						<Button
							size="sm"
							variant="outline"
							disabled={page.offset === 0}
							onclick={() => go(page!.offset - PAGE)}
						>
							Previous
						</Button>
						<Button
							size="sm"
							variant="outline"
							disabled={last >= page.total}
							onclick={() => go(page!.offset + PAGE)}
						>
							Next
						</Button>
					</div>
				</div>
			{/if}
		</section>
	{/if}
</div>
