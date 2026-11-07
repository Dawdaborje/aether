<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { loadFailure, reportError } from '$lib/errors/report';
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { choiceParam, numberParam, textParam } from '$lib/url/filterParams';
	import { syncFilters } from '$lib/url/syncFilters.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Empty } from '$lib/components/ui/empty';
	import { Input } from '$lib/components/ui/input';
	import { NativeSelect, NativeSelectOption } from '$lib/components/ui/native-select';
	import { SearchPicker, type SearchPickerItem } from '$lib/components/ui/search-picker';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import {
		fetchScheduleJobs,
		fetchScheduleTasks,
		restartSchedule,
		runScheduleNow,
		stopSchedule,
		type ScheduleJob,
		type ScheduleTask
	} from '$lib/schedules/api';
	import RunDetail from './RunDetail.svelte';
	import ScheduleDetail from './ScheduleDetail.svelte';
	import ScheduleJobTable from './ScheduleJobTable.svelte';
	import ScheduleTaskTable from './ScheduleTaskTable.svelte';

	let { db }: { db: string } = $props();

	let tasks = $state<ScheduleTask[] | null>(null);
	let failed = $state<string | null>(null);
	// The filters live in the address bar (?q=&plugin=&source=&status=&state=&within=&kind=&origin=),
	// so a refresh or a shared link shows the same view.
	const urlParams = page.url.searchParams;
	let query = $state(textParam(urlParams, 'q'));
	/** Only this plugin's schedules and jobs; empty means every plugin. */
	let pluginFilter = $state(textParam(urlParams, 'plugin'));
	let sourceFilter = $state<'all' | 'manifest' | 'runtime'>(
		choiceParam(urlParams, 'source', ['all', 'manifest', 'runtime'], 'all')
	);
	/** Outcome of the latest run: a job state, `never`, or `all`. */
	let lastState = $state<string>(textParam(urlParams, 'state', 'all'));
	/** Whether the schedule is on: `all`, `enabled` or `stopped`. */
	let enabledFilter = $state<'all' | 'enabled' | 'stopped'>(
		choiceParam(urlParams, 'status', ['all', 'enabled', 'stopped'], 'all')
	);
	/** Ran within this long ago, in hours; 0 means any time. */
	let ranWithin = $state<number>(numberParam(urlParams, 'within', [0, 1, 24, 168, 720], 0));
	let selected = $state<ScheduleTask | null>(null);
	let detailOpen = $state(false);
	let rowWorking = $state<string | null>(null);
	let rowError = $state<string | null>(null);
	let selectedKeys = $state<Set<string>>(new Set());
	let bulkWorking = $state(false);

	const JOBS_PAGE = 25;
	let jobs = $state<ScheduleJob[]>([]);
	let jobsTotal = $state(0);
	let jobsOffset = $state(0);
	let jobsLoading = $state(false);
	let jobsFailed = $state<string | null>(null);
	let jobKind = $state<'all' | 'plugin' | 'communication'>(
		choiceParam(urlParams, 'kind', ['all', 'plugin', 'communication'], 'all')
	);
	let jobOrigin = $state<'all' | 'scheduled' | 'oneoff'>(
		choiceParam(urlParams, 'origin', ['all', 'scheduled', 'oneoff'], 'all')
	);
	let jobOpen = $state(false);
	let jobId = $state<string | null>(null);

	async function load() {
		failed = null;
		try {
			tasks = await fetchScheduleTasks(db);
		} catch (error) {
			failed = loadFailure(error, 'Schedules could not be loaded.');
			tasks = [];
		}
		await loadJobs();
	}

	let jobsRequest = 0;
	async function loadJobs() {
		const request = ++jobsRequest;
		jobsLoading = true;
		jobsFailed = null;
		try {
			const page = await fetchScheduleJobs(db, {
				limit: JOBS_PAGE,
				offset: jobsOffset,
				// "Never ran" is about schedules; jobs have always run or are queued.
				state: lastState === 'all' || lastState === 'never' ? undefined : lastState,
				kind: jobKind === 'all' ? undefined : jobKind,
				plugin: pluginFilter || undefined,
				origin: jobOrigin === 'all' ? undefined : jobOrigin,
				q: query,
				sinceSecs: ranWithin * 3600
			});
			if (request !== jobsRequest) return;
			jobs = page.jobs;
			jobsTotal = page.total;
			// The last page may have emptied after a filter change or a purge.
			if (page.jobs.length === 0 && jobsOffset > 0) {
				jobsOffset = Math.max(0, Math.floor((page.total - 1) / JOBS_PAGE) * JOBS_PAGE);
				if (page.total > 0) void loadJobs();
			}
		} catch (error) {
			if (request !== jobsRequest) return;
			jobsFailed = loadFailure(error, 'Jobs could not be loaded.');
			jobs = [];
			jobsTotal = 0;
		} finally {
			if (request === jobsRequest) jobsLoading = false;
		}
	}

	function goToJobsPage(offset: number) {
		jobsOffset = offset;
		void loadJobs();
	}

	// Refetch jobs when a filter changes (the text filter waits for a pause in typing).
	let filterTimer: ReturnType<typeof setTimeout> | undefined;
	let firstFilterRun = true;
	$effect(() => {
		void [query, lastState, ranWithin, jobKind, jobOrigin, pluginFilter];
		if (firstFilterRun) {
			firstFilterRun = false;
			return;
		}
		clearTimeout(filterTimer);
		filterTimer = setTimeout(() => {
			jobsOffset = 0;
			void loadJobs();
		}, 250);
		return () => clearTimeout(filterTimer);
	});

	syncFilters(
		() => ({
			q: query,
			plugin: pluginFilter,
			source: sourceFilter,
			status: enabledFilter,
			state: lastState,
			within: ranWithin,
			kind: jobKind,
			origin: jobOrigin
		}),
		{ source: 'all', status: 'all', state: 'all', within: 0, kind: 'all', origin: 'all' }
	);

	function openJob(job: ScheduleJob) {
		jobId = job.id;
		jobOpen = true;
	}

	onMount(load);

	const filtered = $derived.by(() => {
		const q = query.trim().toLowerCase();
		return (tasks ?? []).filter((task) => {
			if (pluginFilter && task.plugin !== pluginFilter) return false;
			if (sourceFilter !== 'all' && task.source !== sourceFilter) return false;
			if (enabledFilter !== 'all' && task.enabled !== (enabledFilter === 'enabled')) return false;
			if (lastState === 'never') {
				if (task.last_run || task.last_state) return false;
			} else if (lastState !== 'all' && task.last_state !== lastState) return false;
			if (ranWithin > 0) {
				const ran = task.last_run ? new Date(task.last_run).getTime() : NaN;
				if (Number.isNaN(ran) || Date.now() - ran > ranWithin * 3_600_000) return false;
			}
			if (!q) return true;
			return `${task.plugin} ${task.name} ${task.title ?? ''} ${task.description ?? ''} ${task.function} ${task.cron ?? ''} ${task.queue}`
				.toLowerCase()
				.includes(q);
		});
	});

	/** Plugins to choose from: those with a schedule or a job on screen, and the one picked. */
	const pluginOptions = $derived.by(() => {
		const names = [
			...(tasks ?? []).map((task) => task.plugin),
			...jobs.map((job) => job.plugin ?? ''),
			pluginFilter
		].filter((name) => name !== '');
		return names
			.filter((name, index) => names.indexOf(name) === index)
			.sort((a, b) => a.localeCompare(b));
	});

	const pluginItems = $derived<SearchPickerItem[]>(
		pluginOptions.map((name) => ({ id: name, label: name }))
	);

	// A count of the state buttons follows the plugin picked, like the tables do.
	const stateCounts = $derived.by(() => {
		const counts: Record<string, number> = { never: 0 };
		for (const task of tasks ?? []) {
			if (pluginFilter && task.plugin !== pluginFilter) continue;
			const key = task.last_state ?? (task.last_run ? 'unknown' : 'never');
			counts[key] = (counts[key] ?? 0) + 1;
		}
		return counts;
	});
	const stateOptions = [
		['all', 'All'],
		['succeeded', 'Succeeded'],
		['failed', 'Failed'],
		['running', 'Running'],
		['queued', 'Pending'],
		['never', 'Never ran']
	] as const;
	const filtersActive = $derived(
		query.trim() !== '' ||
			pluginFilter !== '' ||
			sourceFilter !== 'all' ||
			lastState !== 'all' ||
			enabledFilter !== 'all' ||
			ranWithin > 0 ||
			jobKind !== 'all' ||
			jobOrigin !== 'all'
	);

	function clearFilters() {
		query = '';
		pluginFilter = '';
		sourceFilter = 'all';
		lastState = 'all';
		enabledFilter = 'all';
		ranWithin = 0;
		jobKind = 'all';
		jobOrigin = 'all';
	}

	/**
	 * What an empty table says: nothing exists (and how one would), or the filters hide what does.
	 * `kind` is the table's, so "No interval schedules match." is never shown for a plain empty list.
	 */
	function emptyMessage(kind: ScheduleTask['kind']): string {
		const all = (tasks ?? []).filter((task) => task.kind === kind);
		const inPlugin = pluginFilter ? all.filter((task) => task.plugin === pluginFilter) : all;
		const shown = (kind === 'cron' ? cronTasks : intervalTasks).length;
		if (all.length === 0) {
			return `No ${kind} schedules yet. A plugin adds one with a [[schedule]] entry in its plugin.toml, or while it runs with scheduler::register.`;
		}
		if (inPlugin.length === 0) {
			return `${pluginFilter} has no ${kind} schedules. Choose another plugin, or clear the plugin filter to see all ${all.length}.`;
		}
		if (shown === 0) {
			const hidden = inPlugin.length;
			return `${hidden} ${kind} ${hidden === 1 ? 'schedule is' : 'schedules are'} hidden by the filters above. Use Clear to show ${hidden === 1 ? 'it' : 'them'}.`;
		}
		return '';
	}

	/** The filters that apply to jobs, in words, for the empty message. */
	const jobFilterWords = $derived.by(() => {
		const words: string[] = [];
		if (pluginFilter) words.push(`plugin ${pluginFilter}`);
		if (jobKind !== 'all') words.push(jobKind === 'plugin' ? 'plugin functions' : 'email / SMS');
		if (jobOrigin !== 'all') words.push(jobOrigin === 'scheduled' ? 'from a schedule' : 'one-off');
		if (lastState !== 'all') {
			words.push(lastState === 'queued' ? 'pending' : lastState);
		}
		if (ranWithin > 0) {
			words.push(
				ranWithin === 1
					? 'in the last hour'
					: ranWithin < 48
						? `in the last ${ranWithin} hours`
						: `in the last ${ranWithin / 24} days`
			);
		}
		if (query.trim()) words.push(`text “${query.trim()}”`);
		return words;
	});

	const jobsEmptyMessage = $derived(
		jobFilterWords.length === 0
			? 'No jobs yet. Jobs appear here when a schedule fires, a plugin enqueues work or a message is sent.'
			: `No jobs match: ${jobFilterWords.join(', ')}. Use Clear to remove the filters.`
	);

	const cronTasks = $derived(filtered.filter((task) => task.kind === 'cron'));
	const intervalTasks = $derived(filtered.filter((task) => task.kind === 'interval'));

	function scheduleKey(task: ScheduleTask): string {
		return `${task.plugin}:${task.name}`;
	}

	const selectedTasks = $derived(
		(tasks ?? []).filter((task) => selectedKeys.has(scheduleKey(task)))
	);
	const allFilteredSelected = $derived(
		filtered.length > 0 && filtered.every((task) => selectedKeys.has(scheduleKey(task)))
	);

	function selectAllFiltered() {
		selectedKeys = new Set([...selectedKeys, ...filtered.map(scheduleKey)]);
	}

	async function bulk(
		action: (db: string, plugin: string, name: string) => Promise<unknown>,
		fallback: string
	) {
		bulkWorking = true;
		rowError = null;
		const failures: string[] = [];
		for (const task of selectedTasks) {
			try {
				await action(db, task.plugin, task.name);
			} catch (error) {
				failures.push(
					`${task.plugin}.${task.name}: ${error instanceof Error ? error.message : fallback}`
				);
			}
		}
		if (failures.length) rowError = failures.join('\n');
		await load();
		bulkWorking = false;
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
			reportError(error, 'Could not stop the schedule.');
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
			reportError(error, 'Could not restart the schedule.');
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
			reportError(error, 'Could not run the schedule.');
		} finally {
			rowWorking = null;
		}
	}
</script>

<div class="flex min-h-0 w-full flex-1 flex-col gap-4">
	<div class="flex flex-wrap items-center gap-2">
		<div class="relative max-w-xs min-w-[12rem] flex-1">
			<AppIcon
				name="search"
				class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
			/>
			<Input class="pl-8" placeholder="Filter schedules…" bind:value={query} />
		</div>
		<SearchPicker
			class="w-56"
			items={pluginItems}
			value={pluginFilter || null}
			placeholder="Any plugin"
			searchPlaceholder="Search plugins…"
			entityLabel="plugins"
			clearable
			onchange={(id) => (pluginFilter = id)}
		/>
		<NativeSelect
			class="w-36"
			aria-label="Source"
			value={sourceFilter}
			onchange={(event) => (sourceFilter = event.currentTarget.value as typeof sourceFilter)}
		>
			<NativeSelectOption value="all">Any source</NativeSelectOption>
			<NativeSelectOption value="manifest">Manifest</NativeSelectOption>
			<NativeSelectOption value="runtime">Runtime</NativeSelectOption>
		</NativeSelect>
		<NativeSelect
			class="w-36"
			aria-label="Schedule status"
			value={enabledFilter}
			onchange={(event) => (enabledFilter = event.currentTarget.value as typeof enabledFilter)}
		>
			<NativeSelectOption value="all">Any status</NativeSelectOption>
			<NativeSelectOption value="enabled">Enabled</NativeSelectOption>
			<NativeSelectOption value="stopped">Stopped</NativeSelectOption>
		</NativeSelect>
		<NativeSelect
			class="w-48"
			aria-label="Ran within"
			value={String(ranWithin)}
			onchange={(event) => (ranWithin = Number(event.currentTarget.value))}
		>
			<NativeSelectOption value="0">Ran any time</NativeSelectOption>
			<NativeSelectOption value="1">Ran in the last hour</NativeSelectOption>
			<NativeSelectOption value="24">Ran in the last 24 hours</NativeSelectOption>
			<NativeSelectOption value="168">Ran in the last 7 days</NativeSelectOption>
			<NativeSelectOption value="720">Ran in the last 30 days</NativeSelectOption>
		</NativeSelect>
		{#if filtersActive}
			<Button size="sm" variant="ghost" onclick={clearFilters}>Clear</Button>
		{/if}
	</div>

	<div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Last run result">
		{#each stateOptions as [id, label] (id)}
			<Button
				size="xs"
				variant={lastState === id ? 'default' : 'outline'}
				onclick={() => (lastState = id)}
			>
				{label}{id !== 'all' && stateCounts[id] ? ` · ${stateCounts[id]}` : ''}
			</Button>
		{/each}
	</div>

	{#if rowError}
		<p class="text-sm whitespace-pre-line text-destructive" role="alert">{rowError}</p>
	{/if}

	{#if tasks && tasks.length > 0}
		<div
			class="flex flex-wrap items-center gap-2 rounded-md border border-border bg-muted/40 px-3 py-2 text-sm"
			role="status"
		>
			<span class="text-muted-foreground">
				{selectedKeys.size} selected{filtered.length !== tasks.length
					? ` · ${filtered.length} match the filter`
					: ''}
			</span>
			<Button
				size="sm"
				variant="outline"
				disabled={allFilteredSelected || filtered.length === 0}
				onclick={selectAllFiltered}
			>
				Select all {filtered.length}
			</Button>
			{#if selectedKeys.size > 0}
				<Button size="sm" variant="ghost" onclick={() => (selectedKeys = new Set())}>Clear</Button>
				<div class="ml-auto flex flex-wrap gap-2">
					<Button
						size="sm"
						variant="outline"
						disabled={bulkWorking}
						onclick={() => bulk(stopSchedule, 'Could not stop.')}>Stop</Button
					>
					<Button
						size="sm"
						variant="outline"
						disabled={bulkWorking}
						onclick={() => bulk(restartSchedule, 'Could not restart.')}>Restart</Button
					>
					<Button
						size="sm"
						disabled={bulkWorking}
						onclick={() => bulk(runScheduleNow, 'Could not run.')}>Run now</Button
					>
				</div>
			{/if}
		</div>
	{/if}

	{#if tasks === null}
		<div class="space-y-2">
			<Skeleton class="h-8 w-40" />
			<Skeleton class="h-24 w-full" />
			<Skeleton class="h-8 w-40" />
			<Skeleton class="h-24 w-full" />
		</div>
	{:else if failed}
		<LoadFailed message={failed} onRetry={load} />
	{:else}
		{#if tasks.length === 0}
			<Empty
				icon="clock"
				title="No schedules"
				description="Installed plugins for this organization have not registered any recurring tasks yet."
			/>
		{/if}
		<section class="space-y-2">
			<h2 class="text-sm font-medium tracking-wide text-muted-foreground uppercase">
				Cron ({cronTasks.length})
			</h2>
			<ScheduleTaskTable
				rows={cronTasks}
				emptyLabel={emptyMessage('cron')}
				{rowWorking}
				{selectedKeys}
				onSelect={(keys) => (selectedKeys = keys)}
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
				emptyLabel={emptyMessage('interval')}
				{rowWorking}
				{selectedKeys}
				onSelect={(keys) => (selectedKeys = keys)}
				onOpen={openTask}
				{onStop}
				{onRestart}
				{onRunNow}
			/>
		</section>
		<section class="space-y-2">
			<h2 class="text-sm font-medium tracking-wide text-muted-foreground uppercase">
				Jobs ({jobsTotal})
			</h2>
			<div class="flex flex-wrap items-center gap-2">
				<NativeSelect
					class="w-44"
					aria-label="Job type"
					value={jobKind}
					onchange={(event) => (jobKind = event.currentTarget.value as typeof jobKind)}
				>
					<NativeSelectOption value="all">Any job type</NativeSelectOption>
					<NativeSelectOption value="plugin">Plugin function</NativeSelectOption>
					<NativeSelectOption value="communication">Email / SMS</NativeSelectOption>
				</NativeSelect>
				<NativeSelect
					class="w-44"
					aria-label="Job origin"
					value={jobOrigin}
					onchange={(event) => (jobOrigin = event.currentTarget.value as typeof jobOrigin)}
				>
					<NativeSelectOption value="all">Any origin</NativeSelectOption>
					<NativeSelectOption value="scheduled">From a schedule</NativeSelectOption>
					<NativeSelectOption value="oneoff">One-off</NativeSelectOption>
				</NativeSelect>
				<span class="text-xs text-muted-foreground">
					Also uses the plugin, text, result and “ran within” filters above.
				</span>
			</div>
			{#if jobsFailed}
				<LoadFailed message={jobsFailed} onRetry={loadJobs} />
			{:else}
				<ScheduleJobTable
					rows={jobs}
					total={jobsTotal}
					offset={jobsOffset}
					pageSize={JOBS_PAGE}
					loading={jobsLoading}
					emptyLabel={jobsEmptyMessage}
					onOpen={openJob}
					onPage={goToJobsPage}
				/>
			{/if}
		</section>
	{/if}
</div>

<RunDetail bind:open={jobOpen} {db} bind:jobId />
<ScheduleDetail bind:open={detailOpen} {db} bind:task={selected} {onUpdated} />
