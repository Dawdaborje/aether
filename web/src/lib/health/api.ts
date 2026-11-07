import { apiFetch } from '$lib/api/client';

export type HealthLevel = 'ok' | 'warn' | 'error';

export type HealthCheck = {
	id: string;
	label: string;
	level: HealthLevel;
	detail: string;
};

export type SystemHealth = {
	status: HealthLevel;
	checks: HealthCheck[];
	server: { version: string; uptime_secs: number; development_mode: boolean };
	database: { ok: boolean; latency_ms: number; organizations: number | null };
	scheduler: {
		paused: boolean;
		nodes: {
			node_id: string;
			kind: string;
			address?: string | null;
			queues?: string[];
			version?: string;
			started_at?: string;
			heartbeat_at?: string;
		}[];
	};
	plugins: {
		budget_mb: number;
		resident_mb: number;
		plugins_ready: number;
		hits: number;
		loads: number;
		evictions: number;
		refused: number;
		queue_full: number;
		timeouts: number;
		average_load_millis: number;
		waiting_to_compile: number;
		disk_cache: boolean;
	};
	cache: {
		bytes_used: number;
		bytes_max: number;
		entries: number;
		hits: number;
		misses: number;
		evictions: number;
		backend: string;
		l2_redis: boolean;
	};
};

export async function fetchSystemHealth(): Promise<SystemHealth> {
	const res = await apiFetch('/api/desk/health');
	if (!res.ok) throw new Error(`health ${res.status}`);
	return (await res.json()) as SystemHealth;
}

/** "3d 4h", "2h 5m", "45s": how long the server has been up. */
export function formatUptime(secs: number): string {
	if (!Number.isFinite(secs) || secs < 0) return '–';
	const days = Math.floor(secs / 86400);
	const hours = Math.floor((secs % 86400) / 3600);
	const minutes = Math.floor((secs % 3600) / 60);
	if (days > 0) return `${days}d ${hours}h`;
	if (hours > 0) return `${hours}h ${minutes}m`;
	if (minutes > 0) return `${minutes}m`;
	return `${Math.floor(secs)}s`;
}

/** Share of lookups answered from the cache, as a whole percent; null when there were none. */
export function hitRatio(hits: number, misses: number): number | null {
	const total = hits + misses;
	return total === 0 ? null : Math.round((hits / total) * 100);
}

/** The headline for the page. */
export function statusSummary(health: Pick<SystemHealth, 'status' | 'checks'>): string {
	const problems = health.checks.filter((c) => c.level !== 'ok');
	if (problems.length === 0) return 'Everything is running normally.';
	const errors = problems.filter((c) => c.level === 'error').length;
	const warns = problems.length - errors;
	const parts = [
		errors > 0 ? `${errors} problem${errors === 1 ? '' : 's'}` : '',
		warns > 0 ? `${warns} warning${warns === 1 ? '' : 's'}` : ''
	].filter(Boolean);
	return parts.join(' and ');
}
