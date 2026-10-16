<script lang="ts">
	import { reportError } from '$lib/errors/report';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import {
		fetchScheduleRuns,
		fetchScheduleTask,
		formatDuration,
		formatEvery,
		scheduleTitle,
		restartSchedule,
		runScheduleNow,
		stopSchedule,
		type ScheduleRun,
		type ScheduleTask
	} from '$lib/schedules/api';
	import RunDetail from './RunDetail.svelte';

	let {
		open = $bindable(false),
		db,
		task = $bindable(null),
		onUpdated
	}: {
		open?: boolean;
		db: string;
		task?: ScheduleTask | null;
		onUpdated?: (task: ScheduleTask) => void;
	} = $props();

	let detail = $state<ScheduleTask | null>(null);
	const PAGE_SIZE = 20;
	let runs = $state<ScheduleRun[] | null>(null);
	let hasMore = $state(false);
	let loadingMore = $state(false);
	let failed = $state<string | null>(null);
	let loading = $state(false);
	let working = $state(false);
	let actionError = $state<string | null>(null);
	let runOpen = $state(false);
	let selectedRunId = $state<string | null>(null);

	async function reload() {
		if (!task) return;
		loading = true;
		failed = null;
		try {
			const want = Math.max(PAGE_SIZE, runs?.length ?? 0);
			const [{ task: next }, nextRuns] = await Promise.all([
				fetchScheduleTask(db, task.plugin, task.name),
				fetchScheduleRuns(db, task.plugin, task.name, want, 0)
			]);
			detail = next;
			task = next;
			runs = nextRuns;
			hasMore = nextRuns.length === want;
			onUpdated?.(next);
		} catch (error) {
			failed = reportError(error, 'The schedule could not be loaded.');
			runs = runs ?? [];
		} finally {
			loading = false;
		}
	}

	async function loadMore() {
		if (!task || !runs || loadingMore) return;
		loadingMore = true;
		actionError = null;
		try {
			const next = await fetchScheduleRuns(db, task.plugin, task.name, PAGE_SIZE, runs.length);
			const seen = new Set(runs.map((r) => r.id));
			runs = [...runs, ...next.filter((r) => !seen.has(r.id))];
			hasMore = next.length === PAGE_SIZE;
		} catch (error) {
			reportError(error, 'Could not load more runs.');
		} finally {
			loadingMore = false;
		}
	}

	let loadKey = $state<string | null>(null);
	$effect(() => {
		if (!open || !task) {
			detail = null;
			runs = null;
			hasMore = false;
			failed = null;
			actionError = null;
			loadKey = null;
			return;
		}
		const key = `${db}:${task.plugin}:${task.name}`;
		if (loadKey === key) return;
		loadKey = key;
		void reload();
	});

	function formatWhen(iso: string | null | undefined): string {
		if (!iso) return '—';
		const date = new Date(iso);
		return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
	}

	function scheduleLabel(row: ScheduleTask): string {
		if (row.kind === 'cron') {
			return row.timezone ? `${row.cron} (${row.timezone})` : (row.cron ?? '—');
		}
		return `every ${formatEvery(row.every_secs)}`;
	}

	async function onStop() {
		if (!task) return;
		working = true;
		actionError = null;
		try {
			await stopSchedule(db, task.plugin, task.name);
			await reload();
		} catch (error) {
			reportError(error, 'Could not stop the schedule.');
		} finally {
			working = false;
		}
	}

	async function onRestart() {
		if (!task) return;
		working = true;
		actionError = null;
		try {
			await restartSchedule(db, task.plugin, task.name);
			await reload();
		} catch (error) {
			reportError(error, 'Could not restart the schedule.');
		} finally {
			working = false;
		}
	}

	async function onRunNow() {
		if (!task) return;
		working = true;
		actionError = null;
		try {
			await runScheduleNow(db, task.plugin, task.name);
			await reload();
		} catch (error) {
			reportError(error, 'Could not run the schedule.');
		} finally {
			working = false;
		}
	}

	function openRun(id: string) {
		selectedRunId = id;
		runOpen = true;
	}

	function stateVariant(state: string): 'default' | 'secondary' | 'outline' | 'destructive' {
		if (state === 'succeeded') return 'default';
		if (state === 'failed') return 'destructive';
		if (state === 'running') return 'secondary';
		return 'outline';
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="flex h-[90vh] max-h-[90vh] w-full max-w-5xl flex-col sm:max-w-5xl gap-0 overflow-hidden p-0">
		{#if task}
			<Dialog.Header class="border-b border-border py-4 pr-16 pl-6">
				<div class="flex flex-wrap items-start justify-between gap-3">
					<div class="min-w-0 space-y-1">
						<Dialog.Title class="truncate">{scheduleTitle(detail ?? task)}</Dialog.Title>
						<Dialog.Description>
							{task.plugin} · {scheduleLabel(detail ?? task)}
						</Dialog.Description>
					</div>
					<div class="flex flex-wrap gap-2">
						{#if (detail ?? task).enabled}
							<Button size="sm" variant="outline" disabled={working} onclick={onStop}>
								Stop
							</Button>
						{:else}
							<Button size="sm" variant="outline" disabled={working} onclick={onRestart}>
								Restart
							</Button>
						{/if}
						<Button size="sm" disabled={working} onclick={onRunNow}>Run now</Button>
					</div>
				</div>
			</Dialog.Header>

			<div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
				{#if actionError}
					<p class="text-sm text-destructive" role="alert">{actionError}</p>
				{/if}

				{#if loading && !detail}
					<div class="space-y-2">
						<Skeleton class="h-4 w-1/3" />
						<Skeleton class="h-4 w-1/2" />
						<Skeleton class="h-24 w-full" />
					</div>
				{:else if failed && !detail}
					<p class="text-sm text-destructive" role="alert">{failed}</p>
				{:else if detail}
					<div class="flex flex-wrap gap-2">
						<Badge variant={detail.enabled ? 'default' : 'secondary'}>
							{detail.enabled ? 'Enabled' : 'Stopped'}
						</Badge>
						<Badge variant="outline">{detail.kind}</Badge>
						<Badge variant="outline">{detail.source}</Badge>
						<Badge variant="outline">queue {detail.queue}</Badge>
					</div>
					{#if detail.description}
						<p class="text-sm text-muted-foreground">{detail.description}</p>
					{/if}
					<dl class="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
						<div>
							<dt class="text-xs text-muted-foreground">Plugin</dt>
							<dd class="font-mono text-xs">{detail.plugin}</dd>
						</div>
						<div>
							<dt class="text-xs text-muted-foreground">Function</dt>
							<dd class="font-mono text-xs">{detail.function}</dd>
						</div>
						<div>
							<dt class="text-xs text-muted-foreground">Next run</dt>
							<dd>{formatWhen(detail.next_run)}</dd>
						</div>
						<div>
							<dt class="text-xs text-muted-foreground">Last run</dt>
							<dd>{formatWhen(detail.last_run)}</dd>
						</div>
					</dl>
				{/if}

				<section class="space-y-2">
					<h3 class="text-sm font-medium">Runs</h3>
					{#if runs === null}
						<div class="space-y-2">
							<Skeleton class="h-10 w-full" />
							<Skeleton class="h-10 w-full" />
						</div>
					{:else if runs.length === 0}
						<p class="text-sm text-muted-foreground">No runs yet for this schedule.</p>
					{:else}
						<ul class="divide-y divide-border rounded-md border border-border">
							{#each runs as run (run.id)}
								<li>
									<button
										type="button"
										class="flex w-full flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-left text-sm transition hover:bg-muted/60"
										onclick={() => openRun(run.id)}
									>
										<div class="min-w-0 space-y-0.5">
											<p class="font-medium">
												{formatWhen(run.date_started ?? run.run_at ?? run.date_created)}
											</p>
											<p class="text-xs text-muted-foreground">
												{formatWhen(run.date_finished)} · {formatDuration(run.duration_ms)}
											</p>
										</div>
										<Badge variant={stateVariant(run.state)}>{run.state}</Badge>
									</button>
								</li>
							{/each}
						</ul>
						{#if hasMore}
							<div class="flex justify-center pt-2">
								<Button variant="outline" size="sm" disabled={loadingMore} onclick={loadMore}>
									{loadingMore ? 'Loading…' : 'Load more'}
								</Button>
							</div>
						{/if}
					{/if}
				</section>
			</div>
		{/if}
	</Dialog.Content>
</Dialog.Root>

<RunDetail bind:open={runOpen} {db} bind:jobId={selectedRunId} />
