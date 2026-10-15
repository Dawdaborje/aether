<script lang="ts">
	import { onMount } from 'svelte';
	import ClockIcon from '@lucide/svelte/icons/clock';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { Button } from '$lib/components/ui/button';
	import { Empty } from '$lib/components/ui/empty';
	import { Input } from '$lib/components/ui/input';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import {
		fetchScheduleTasks,
		restartSchedule,
		runScheduleNow,
		stopSchedule,
		type ScheduleTask
	} from '$lib/schedules/api';
	import ScheduleDetail from './ScheduleDetail.svelte';
	import ScheduleTaskTable from './ScheduleTaskTable.svelte';

	let { db }: { db: string } = $props();

	let tasks = $state<ScheduleTask[] | null>(null);
	let failed = $state<string | null>(null);
	let query = $state('');
	let sourceFilter = $state<'all' | 'manifest' | 'runtime'>('all');
	let selected = $state<ScheduleTask | null>(null);
	let detailOpen = $state(false);
	let rowWorking = $state<string | null>(null);
	let rowError = $state<string | null>(null);

	async function load() {
		failed = null;
		try {
			tasks = await fetchScheduleTasks(db);
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Schedules could not be loaded.';
			tasks = [];
		}
	}

	onMount(load);

	const filtered = $derived.by(() => {
		const q = query.trim().toLowerCase();
		return (tasks ?? []).filter((task) => {
			if (sourceFilter !== 'all' && task.source !== sourceFilter) return false;
			if (!q) return true;
			return `${task.plugin} ${task.name} ${task.function} ${task.cron ?? ''} ${task.queue}`
				.toLowerCase()
				.includes(q);
		});
	});

	const cronTasks = $derived(filtered.filter((task) => task.kind === 'cron'));
	const intervalTasks = $derived(filtered.filter((task) => task.kind === 'interval'));

	function scheduleKey(task: ScheduleTask): string {
		return `${task.plugin}:${task.name}`;
	}

	function openTask(task: ScheduleTask) {
		selected = task;
		detailOpen = true;
	}

	function onUpdated(updated: ScheduleTask) {
		if (!tasks) return;
		tasks = tasks.map((task) =>
			task.plugin === updated.plugin && task.name === updated.name ? updated : task
		);
		selected = updated;
	}

	async function onStop(task: ScheduleTask, event: MouseEvent) {
		event.stopPropagation();
		rowWorking = scheduleKey(task);
		rowError = null;
		try {
			await stopSchedule(db, task.plugin, task.name);
			await load();
		} catch (error) {
			rowError = error instanceof Error ? error.message : 'Could not stop the schedule.';
		} finally {
			rowWorking = null;
		}
	}

	async function onRestart(task: ScheduleTask, event: MouseEvent) {
		event.stopPropagation();
		rowWorking = scheduleKey(task);
		rowError = null;
		try {
			await restartSchedule(db, task.plugin, task.name);
			await load();
		} catch (error) {
			rowError = error instanceof Error ? error.message : 'Could not restart the schedule.';
		} finally {
			rowWorking = null;
		}
	}

	async function onRunNow(task: ScheduleTask, event: MouseEvent) {
		event.stopPropagation();
		rowWorking = scheduleKey(task);
		rowError = null;
		try {
			await runScheduleNow(db, task.plugin, task.name);
			await load();
		} catch (error) {
			rowError = error instanceof Error ? error.message : 'Could not run the schedule.';
		} finally {
			rowWorking = null;
		}
	}
</script>

<div class="space-y-4">
	<div class="flex flex-wrap items-center gap-3">
		<div class="relative min-w-[12rem] flex-1 max-w-xs">
			<SearchIcon
				class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
			/>
			<Input class="pl-8" placeholder="Filter schedules…" bind:value={query} />
		</div>
		<div class="flex flex-wrap gap-1">
			{#each [['all', 'All'], ['manifest', 'Manifest'], ['runtime', 'Runtime']] as [id, label] (id)}
				<Button
					size="xs"
					variant={sourceFilter === id ? 'default' : 'outline'}
					onclick={() => (sourceFilter = id as typeof sourceFilter)}
				>
					{label}
				</Button>
			{/each}
		</div>
	</div>

	{#if rowError}
		<p class="text-sm text-destructive" role="alert">{rowError}</p>
	{/if}

	{#if tasks === null}
		<div class="space-y-2">
			<Skeleton class="h-8 w-40" />
			<Skeleton class="h-24 w-full" />
			<Skeleton class="h-8 w-40" />
			<Skeleton class="h-24 w-full" />
		</div>
	{:else if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{:else if tasks.length === 0}
		<Empty
			icon={ClockIcon}
			title="No schedules"
			description="Installed plugins for this organization have not registered any recurring tasks yet."
		/>
	{:else}
		<section class="space-y-2">
			<h2 class="text-sm font-medium tracking-wide text-muted-foreground uppercase">
				Cron ({cronTasks.length})
			</h2>
			<ScheduleTaskTable
				rows={cronTasks}
				emptyLabel="No cron schedules match."
				{rowWorking}
				onOpen={openTask}
				{onStop}
				{onRestart}
				{onRunNow}
			/>
		</section>
		<section class="space-y-2">
			<h2 class="text-sm font-medium tracking-wide text-muted-foreground uppercase">
				Interval ({intervalTasks.length})
			</h2>
			<ScheduleTaskTable
				rows={intervalTasks}
				emptyLabel="No interval schedules match."
				{rowWorking}
				onOpen={openTask}
				{onStop}
				{onRestart}
				{onRunNow}
			/>
		</section>
	{/if}
</div>

<ScheduleDetail bind:open={detailOpen} {db} bind:task={selected} {onUpdated} />
