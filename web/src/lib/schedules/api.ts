import { apiFetch } from '$lib/api/client';

export type ScheduleKind = 'cron' | 'interval';

export interface ScheduleTask {
	plugin: string;
	name: string;
	title: string | null;
	description: string | null;
	function: string;
	kind: ScheduleKind;
	cron: string | null;
	every_secs: number | null;
	timezone: string | null;
	queue: string;
	max_attempts: number;
	source: 'manifest' | 'runtime' | string;
	enabled: boolean;
	catch_up: string;
	next_run: string | null;
	last_run: string | null;
	/** State of the latest job: `queued`, `running`, `succeeded` or `failed`; null if it never ran. */
	last_state?: string | null;
}

/** The label to show for a task: its title, or its key made readable ("nightly-report" → "Nightly report"). */
export function scheduleTitle(task: Pick<ScheduleTask, 'name' | 'title'>): string {
	const title = task.title?.trim();
	if (title) return title;
	const words = task.name.replace(/[_\-.]+/g, ' ').trim();
	return words ? words.charAt(0).toUpperCase() + words.slice(1) : task.name;
}

export interface ScheduleRun {
	id: string;
	state: string;
	attempts: number;
	max_attempts: number;
	last_error: string | null;
	date_started: string | null;
	date_finished: string | null;
	run_at: string | null;
	date_created: string | null;
	duration_ms: number | null;
}

export interface ScheduleJob {
	id: string;
	kind: string;
	plugin: string | null;
	function: string | null;
	task: string | null;
	queue: string;
	state: string;
	attempts: number;
	max_attempts: number;
	last_error: string | null;
	date_started: string | null;
	date_finished: string | null;
	run_at: string | null;
	date_created: string | null;
	duration_ms: number | null;
}

export interface PluginCallLog {
	plugin: string;
	function: string;
	status: number;
	date_created: string | null;
}

export interface DataAccessLog {
	plugin: string;
	function: string;
	model: string | null;
	table: string | null;
	operation: string;
	record_count: number;
	statement: string | null;
	date_created: string | null;
}

export interface JobDetail {
	job: ScheduleJob;
	logs: {
		plugin_calls: PluginCallLog[];
		data_access: DataAccessLog[];
	};
}

async function readError(res: Response, fallback: string): Promise<string> {
	const body = await res.json().catch(() => ({}));
	return typeof body.error === 'string' ? body.error : fallback;
}

function base(db: string): string {
	return `/api/ui/organizations/${encodeURIComponent(db)}/schedules`;
}

export async function fetchScheduleTasks(db: string): Promise<ScheduleTask[]> {
	const res = await apiFetch(`${base(db)}/tasks`);
	if (!res.ok) {
		throw new Error(await readError(res, `Schedules could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { tasks: ScheduleTask[] };
	return body.tasks ?? [];
}

export async function fetchScheduleTask(
	db: string,
	plugin: string,
	name: string
): Promise<{ task: ScheduleTask; run_counts: { state: string; n: number }[] }> {
	const res = await apiFetch(
		`${base(db)}/tasks/${encodeURIComponent(plugin)}/${encodeURIComponent(name)}`
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Schedule could not be loaded (${res.status}).`));
	}
	return (await res.json()) as { task: ScheduleTask; run_counts: { state: string; n: number }[] };
}

export async function fetchScheduleRuns(
	db: string,
	plugin: string,
	name: string,
	limit = 100,
	offset = 0
): Promise<ScheduleRun[]> {
	const res = await apiFetch(
		`${base(db)}/tasks/${encodeURIComponent(plugin)}/${encodeURIComponent(name)}/runs?limit=${limit}&offset=${offset}`
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Runs could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { runs: ScheduleRun[] };
	return body.runs ?? [];
}

export async function fetchScheduleJob(db: string, id: string): Promise<JobDetail> {
	const res = await apiFetch(`${base(db)}/jobs/${encodeURIComponent(id)}`);
	if (!res.ok) {
		throw new Error(await readError(res, `Job could not be loaded (${res.status}).`));
	}
	return (await res.json()) as JobDetail;
}

export interface JobsQuery {
	limit?: number;
	offset?: number;
	state?: string;
	kind?: string;
	/** Only jobs of this plugin. */
	plugin?: string;
	/** `scheduled` (made by a recurring task) or `oneoff`. */
	origin?: string;
	q?: string;
	/** Only jobs created within this many seconds. */
	sinceSecs?: number;
}

/** One page of the organization's jobs (recurring runs, one-off `enqueue` calls and messages), newest first. */
export async function fetchScheduleJobs(
	db: string,
	query: JobsQuery = {}
): Promise<{ jobs: ScheduleJob[]; counts: { state: string; n: number }[]; total: number }> {
	const params = new URLSearchParams();
	params.set('limit', String(query.limit ?? 25));
	params.set('offset', String(query.offset ?? 0));
	if (query.state) params.set('state', query.state);
	if (query.kind) params.set('kind', query.kind);
	if (query.plugin) params.set('plugin', query.plugin);
	if (query.origin) params.set('origin', query.origin);
	if (query.q?.trim()) params.set('q', query.q.trim());
	if (query.sinceSecs && query.sinceSecs > 0) params.set('since_secs', String(query.sinceSecs));
	const res = await apiFetch(`${base(db)}/jobs?${params}`);
	if (!res.ok) {
		throw new Error(await readError(res, `Jobs could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as {
		jobs: ScheduleJob[];
		counts: { state: string; n: number }[];
		total: number;
	};
	return { jobs: body.jobs ?? [], counts: body.counts ?? [], total: body.total ?? 0 };
}

export async function stopSchedule(db: string, plugin: string, name: string): Promise<void> {
	const res = await apiFetch(
		`${base(db)}/tasks/${encodeURIComponent(plugin)}/${encodeURIComponent(name)}/stop`,
		{ method: 'POST' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Could not stop the schedule (${res.status}).`));
	}
}

export async function restartSchedule(db: string, plugin: string, name: string): Promise<void> {
	const res = await apiFetch(
		`${base(db)}/tasks/${encodeURIComponent(plugin)}/${encodeURIComponent(name)}/restart`,
		{ method: 'POST' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Could not restart the schedule (${res.status}).`));
	}
}

export async function runScheduleNow(
	db: string,
	plugin: string,
	name: string
): Promise<{ job: string; created: boolean }> {
	const res = await apiFetch(
		`${base(db)}/tasks/${encodeURIComponent(plugin)}/${encodeURIComponent(name)}/run`,
		{ method: 'POST' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Could not run the schedule (${res.status}).`));
	}
	return (await res.json()) as { job: string; created: boolean };
}

/** Format a duration in milliseconds for the desk UI. */
export function formatDuration(ms: number | null | undefined): string {
	if (ms == null || Number.isNaN(ms)) return '—';
	if (ms < 1000) return `${ms}ms`;
	const seconds = ms / 1000;
	if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)}s`;
	const minutes = Math.floor(seconds / 60);
	const rest = Math.round(seconds % 60);
	return rest ? `${minutes}m ${rest}s` : `${minutes}m`;
}

/** Human-readable interval from every_secs. */
export function formatEvery(secs: number | null | undefined): string {
	if (secs == null) return '—';
	if (secs % 86400 === 0) return `${secs / 86400}d`;
	if (secs % 3600 === 0) return `${secs / 3600}h`;
	if (secs % 60 === 0) return `${secs / 60}m`;
	return `${secs}s`;
}
