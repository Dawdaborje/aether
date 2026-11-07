<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { onDestroy, onMount } from 'svelte';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { loadFailure } from '$lib/errors/report';
	import {
		fetchSystemHealth,
		formatUptime,
		hitRatio,
		statusSummary,
		type HealthLevel,
		type SystemHealth
	} from '$lib/health/api';
	import { formatBytes } from '$lib/mantle/api';

	const REFRESH_MS = 10_000;

	let health = $state<SystemHealth | null>(null);
	let failed = $state<string | null>(null);
	let loading = $state(false);
	let checkedAt = $state<Date | null>(null);
	let timer: ReturnType<typeof setInterval> | null = null;

	async function refresh() {
		loading = true;
		try {
			health = await fetchSystemHealth();
			failed = null;
			checkedAt = new Date();
		} catch (error) {
			failed = loadFailure(error, 'The system health could not be loaded.');
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void refresh();
		// Do not poll a tab nobody is looking at.
		timer = setInterval(() => {
			if (!document.hidden) void refresh();
		}, REFRESH_MS);
	});
	onDestroy(() => {
		if (timer) clearInterval(timer);
	});

	const TONE: Record<HealthLevel, string> = {
		ok: 'text-emerald-600 dark:text-emerald-400',
		warn: 'text-amber-600 dark:text-amber-400',
		error: 'text-destructive'
	};
	const WORD: Record<HealthLevel, string> = {
		ok: 'Healthy',
		warn: 'Needs attention',
		error: 'Problem'
	};

	const cachePercent = $derived(
		health && health.cache.bytes_max > 0
			? Math.min(100, Math.round((health.cache.bytes_used / health.cache.bytes_max) * 100))
			: 0
	);
	const pluginPercent = $derived(
		health && health.plugins.budget_mb > 0
			? Math.min(100, Math.round((health.plugins.resident_mb / health.plugins.budget_mb) * 100))
			: 0
	);
</script>

{#snippet icon(level: HealthLevel)}
	{#if level === 'ok'}
		<AppIcon name="check-circle-filled" class="size-5 shrink-0 {TONE.ok}" aria-hidden="true" />
	{:else if level === 'warn'}
		<AppIcon name="warning-filled" class="size-5 shrink-0 {TONE.warn}" aria-hidden="true" />
	{:else}
		<AppIcon name="error-filled" class="size-5 shrink-0 {TONE.error}" aria-hidden="true" />
	{/if}
{/snippet}

{#snippet meter(percent: number, level: HealthLevel)}
	<div
		class="h-1.5 w-full overflow-hidden rounded-full bg-muted"
		role="progressbar"
		aria-valuenow={percent}
		aria-valuemin="0"
		aria-valuemax="100"
	>
		<div
			class="h-full {level === 'ok'
				? 'bg-emerald-500'
				: level === 'warn'
					? 'bg-amber-500'
					: 'bg-destructive'}"
			style="width: {percent}%"
		></div>
	</div>
{/snippet}

<div class="space-y-4" data-testid="system-health">
	{#if failed && !health}
		<LoadFailed message={failed} onRetry={refresh} retrying={loading} />
	{:else if !health}
		<Skeleton class="h-20 w-full" />
		<div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
			{#each [1, 2, 3, 4, 5] as n (n)}
				<Skeleton class="h-32 w-full" />
			{/each}
		</div>
	{:else}
		<div class="flex flex-wrap items-center gap-3 rounded-md border border-border bg-card p-4">
			{@render icon(health.status)}
			<div class="min-w-0">
				<p class="font-semibold {TONE[health.status]}">{WORD[health.status]}</p>
				<p class="text-sm text-muted-foreground">{statusSummary(health)}</p>
			</div>
			<div class="ml-auto flex items-center gap-3 text-xs text-muted-foreground">
				<span>
					Aether {health.server.version} · up {formatUptime(health.server.uptime_secs)}
					{#if checkedAt}· checked {checkedAt.toLocaleTimeString()}{/if}
				</span>
				<Button size="sm" variant="outline" onclick={refresh} disabled={loading}>
					<AppIcon name="refresh" class="size-3.5" />
					Refresh
				</Button>
			</div>
		</div>

		{#if failed}
			<p class="text-sm text-destructive" role="alert">
				{failed} Showing the last result.
			</p>
		{/if}

		<div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
			{#each health.checks as check (check.id)}
				<section class="space-y-2 rounded-md border border-border bg-card p-4">
					<header class="flex items-center gap-2">
						{@render icon(check.level)}
						<h2 class="font-semibold">{check.label}</h2>
					</header>
					<p class="text-sm text-muted-foreground">{check.detail}</p>

					{#if check.id === 'database'}
						<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
							<dt class="text-muted-foreground">Response time</dt>
							<dd>{health.database.latency_ms} ms</dd>
							<dt class="text-muted-foreground">Organizations</dt>
							<dd>{health.database.organizations ?? '–'}</dd>
						</dl>
					{:else if check.id === 'scheduler'}
						{#if health.scheduler.nodes.length === 0}
							<p class="text-sm">No node is reporting.</p>
						{:else}
							<ul class="space-y-1 text-sm">
								{#each health.scheduler.nodes as node (node.node_id)}
									<li class="flex flex-wrap gap-x-2">
										<span class="font-mono text-xs">{node.node_id}</span>
										<span class="text-muted-foreground">
											{node.kind}{node.queues?.length ? ` · ${node.queues.join(', ')}` : ''}
										</span>
									</li>
								{/each}
							</ul>
						{/if}
					{:else if check.id === 'plugins'}
						{@render meter(pluginPercent, pluginPercent >= 90 ? 'warn' : 'ok')}
						<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
							<dt class="text-muted-foreground">Memory</dt>
							<dd>{health.plugins.resident_mb.toFixed(0)} of {health.plugins.budget_mb} MB</dd>
							<dt class="text-muted-foreground">Ready</dt>
							<dd>{health.plugins.plugins_ready}</dd>
							<dt class="text-muted-foreground">Average load</dt>
							<dd>{health.plugins.average_load_millis} ms</dd>
							<dt class="text-muted-foreground">Evictions</dt>
							<dd>{health.plugins.evictions}</dd>
						</dl>
					{:else if check.id === 'cache'}
						{@render meter(cachePercent, cachePercent >= 90 ? 'warn' : 'ok')}
						<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
							<dt class="text-muted-foreground">Used</dt>
							<dd>
								{formatBytes(health.cache.bytes_used)} of {formatBytes(health.cache.bytes_max)}
							</dd>
							<dt class="text-muted-foreground">Hit ratio</dt>
							<dd>
								{hitRatio(health.cache.hits, health.cache.misses) ?? '–'}{hitRatio(
									health.cache.hits,
									health.cache.misses
								) === null
									? ''
									: '%'}
							</dd>
							<dt class="text-muted-foreground">Backend</dt>
							<dd>{health.cache.backend}{health.cache.l2_redis ? ' + Redis' : ''}</dd>
						</dl>
					{/if}
				</section>
			{/each}
		</div>
		<p class="text-xs text-muted-foreground">
			Refreshes every {REFRESH_MS / 1000} seconds while this tab is open.
		</p>
	{/if}
</div>
