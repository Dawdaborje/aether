<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import FlameIcon from '@lucide/svelte/icons/flame';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import {
		fetchMantleStats,
		formatBytes,
		type MantleStatsResponse,
		type OrgMantleStats
	} from '$lib/mantle/api';

	let stats = $state<MantleStatsResponse | null>(null);
	let failed = $state<string | null>(null);
	let expanded = $state<string | null>(null);
	let loading = $state(false);
	let timer: ReturnType<typeof setInterval> | null = null;

	async function load() {
		loading = true;
		failed = null;
		try {
			stats = await fetchMantleStats();
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Mantle stats could not be loaded.';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void load();
		timer = setInterval(() => void load(), 5000);
	});

	onDestroy(() => {
		if (timer) clearInterval(timer);
	});

	function hitRate(system: MantleStatsResponse['system']): string {
		const total = system.hits + system.misses;
		if (total === 0) return '—';
		return `${Math.round((system.hits / total) * 100)}%`;
	}

	function toggle(org: OrgMantleStats) {
		expanded = expanded === org.org ? null : org.org;
	}

	const kindEntries = $derived(
		stats
			? Object.entries(stats.system.kind_bytes ?? {}).sort((a, b) => b[1] - a[1])
			: ([] as [string, number][])
	);
</script>

<div class="flex flex-col gap-4">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="space-y-1">
			<p class="text-sm text-muted-foreground">
				Working-set cache: system totals, kernel warm-set tiers, and per-org heat.
			</p>
		</div>
		<Button size="sm" variant="outline" onclick={() => void load()} disabled={loading}>
			<RefreshCwIcon class="size-4" />
			Refresh
		</Button>
	</div>

	{#if failed}
		<p class="text-sm text-destructive">{failed}</p>
	{:else if !stats}
		<p class="text-sm text-muted-foreground">Loading Mantle…</p>
	{:else}
		{@const system = stats.system}
		<section class="rounded-md border border-border px-4 py-3">
			<div class="mb-2 flex items-center gap-2 text-sm font-medium">
				<FlameIcon class="size-4" />
				System total
				<Badge variant="secondary">{system.backend}</Badge>
			</div>
			<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
				<div>
					<p class="text-xs text-muted-foreground">Bytes</p>
					<p class="font-mono text-sm">
						{formatBytes(system.bytes_used)} / {formatBytes(system.bytes_max)}
						{#if system.bytes_target != null && system.bytes_target !== system.bytes_max}
							<span class="text-muted-foreground">
								(target {formatBytes(system.bytes_target)})
							</span>
						{/if}
						{#if system.l2_redis}
							<span class="text-muted-foreground"> · L2</span>
						{/if}
					</p>
					<div class="mt-1 h-1.5 overflow-hidden rounded bg-muted">
						<div
							class="h-full bg-foreground/70"
							style={`width: ${system.bytes_max ? Math.min(100, (system.bytes_used / system.bytes_max) * 100) : 0}%`}
						></div>
					</div>
				</div>
				<div>
					<p class="text-xs text-muted-foreground">Entries</p>
					<p class="font-mono text-sm">{system.entries}</p>
				</div>
				<div>
					<p class="text-xs text-muted-foreground">Hit rate</p>
					<p class="font-mono text-sm">
						{hitRate(system)}
						<span class="text-muted-foreground">
							({system.hits}h / {system.misses}m / {system.ghost_hits}g)
						</span>
					</p>
				</div>
				<div>
					<p class="text-xs text-muted-foreground">Kernel vs plugin</p>
					<p class="font-mono text-sm">
						{formatBytes(system.kernel_bytes)} / {formatBytes(system.plugin_bytes)}
					</p>
					{#if system.session_entries != null}
						<p class="text-xs text-muted-foreground">
							{system.session_entries} session entries
						</p>
					{/if}
					{#if system.adaptive_p_millis != null}
						<p class="text-xs text-muted-foreground">
							adaptive p {(system.adaptive_p_millis / 10).toFixed(0)}%
						</p>
					{/if}
					{#if system.warmup}
						<p class="text-xs text-muted-foreground">
							warmup {system.warmup.pending}q / {system.warmup.filled} filled
						</p>
					{/if}
				</div>
			</div>
		</section>

		{#if kindEntries.length}
			<section class="rounded-md border border-border px-4 py-3">
				<p class="mb-2 text-sm font-medium">Kind mix</p>
				<div class="flex flex-wrap gap-2">
					{#each kindEntries as [kind, bytes]}
						<span class="rounded bg-muted px-2 py-1 font-mono text-xs">
							{kind}: {formatBytes(bytes)}
						</span>
					{/each}
				</div>
			</section>
		{/if}

		<section class="rounded-md border border-border px-4 py-3">
			<p class="mb-2 text-sm font-medium">Warm-set tiers</p>
			<div class="grid gap-3 md:grid-cols-3">
				{#each [['A', system.tiers.a], ['B', system.tiers.b], ['C', system.tiers.c]] as [label, keys]}
					<div>
						<p class="mb-1 text-xs text-muted-foreground">Tier {label}</p>
						<ul class="space-y-1">
							{#each keys as key}
								<li class="flex items-center gap-2 font-mono text-xs">
									<span
										class={`inline-block size-2 rounded-full ${key.present ? 'bg-success' : 'bg-warning'}`}
									></span>
									{key.key}
								</li>
							{:else}
								<li class="text-xs text-muted-foreground">No tracked keys</li>
							{/each}
						</ul>
					</div>
				{/each}
			</div>
		</section>

		<section class="rounded-md border border-border">
			<div class="border-b border-border px-4 py-2 text-sm font-medium">Organizations</div>
			{#if stats.orgs.length === 0}
				<p class="px-4 py-6 text-sm text-muted-foreground">
					No org heat or warm data yet. Traffic will fill this after cache::* and warm-set use.
				</p>
			{:else}
				<ul class="divide-y divide-border">
					{#each stats.orgs as org}
						<li>
							<button
								type="button"
								class="flex w-full items-center justify-between gap-3 px-4 py-3 text-left hover:bg-muted/40"
								onclick={() => toggle(org)}
							>
								<div>
									<p class="font-mono text-sm">{org.org}</p>
									{#if org.heat_updated_at}
										<p class="text-xs text-muted-foreground">
											heat {new Date(org.heat_updated_at).toLocaleString()}
										</p>
									{/if}
								</div>
								<p class="font-mono text-xs text-muted-foreground">
									{formatBytes(org.bytes)} · {org.entries} entries
								</p>
							</button>
							{#if expanded === org.org}
								<div class="grid gap-4 border-t border-border bg-muted/20 px-4 py-3 md:grid-cols-2">
									<div>
										<p
											class="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase"
										>
											Hot plugins / apps
										</p>
										{#if org.hot_plugins.length === 0}
											<p class="text-xs text-muted-foreground">No heat yet</p>
										{:else}
											<ul class="space-y-1">
												{#each org.hot_plugins as plugin}
													<li class="flex justify-between gap-2 font-mono text-xs">
														<span>{plugin.app ?? plugin.plugin}</span>
														<span class="text-muted-foreground">
															{plugin.score.toFixed(1)} · {formatBytes(plugin.bytes)}
														</span>
													</li>
												{/each}
											</ul>
										{/if}
									</div>
									<div>
										<p
											class="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase"
										>
											Hot models
										</p>
										{#if org.hot_models.length === 0}
											<p class="text-xs text-muted-foreground">No heat yet</p>
										{:else}
											<ul class="space-y-1">
												{#each org.hot_models as model}
													<li class="flex justify-between gap-2 font-mono text-xs">
														<span>{model.plugin}/{model.model}:{model.kind}</span>
														<span class="text-muted-foreground">
															{model.score.toFixed(1)} · {formatBytes(model.bytes)}
														</span>
													</li>
												{/each}
											</ul>
										{/if}
									</div>
								</div>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</div>
