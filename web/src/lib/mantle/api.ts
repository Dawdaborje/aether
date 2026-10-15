import { apiFetch } from '$lib/api/client';

export type TierKeyStatus = {
	key: string;
	present: boolean;
};

export type MantleSystemStats = {
	bytes_used: number;
	bytes_max: number;
	bytes_target?: number;
	entries: number;
	hits: number;
	misses: number;
	ghost_hits: number;
	evictions: number;
	backend: string;
	l2_redis?: boolean;
	kernel_bytes: number;
	plugin_bytes: number;
	kind_bytes: Record<string, number>;
	ghost_len?: number;
	session_entries?: number;
	/** Adaptive recent-list target in millis (0..=1000). */
	adaptive_p_millis?: number;
	warmup?: {
		pending: number;
		enqueued: number;
		filled: number;
		skipped_present: number;
		skipped_no_fetcher: number;
		errors: number;
	};
	tiers: {
		a: TierKeyStatus[];
		b: TierKeyStatus[];
		c: TierKeyStatus[];
	};
};

export type HotPlugin = {
	plugin: string;
	app?: string | null;
	score: number;
	hits: number;
	bytes: number;
	entries: number;
};

export type HotModel = {
	plugin: string;
	model: string;
	kind: string;
	score: number;
	hits: number;
	bytes: number;
	entries: number;
};

export type OrgMantleStats = {
	org: string;
	bytes: number;
	entries: number;
	heat_updated_at?: string | null;
	hot_plugins: HotPlugin[];
	hot_models: HotModel[];
};

export type MantleStatsResponse = {
	system: MantleSystemStats;
	orgs: OrgMantleStats[];
};

async function readError(res: Response, fallback: string): Promise<string> {
	try {
		const body = (await res.json()) as { error?: string; message?: string };
		return body.error || body.message || fallback;
	} catch {
		return fallback;
	}
}

export async function fetchMantleStats(): Promise<MantleStatsResponse> {
	const res = await apiFetch('/api/desk/mantle/stats');
	if (!res.ok) {
		throw new Error(await readError(res, `Mantle stats could not be loaded (${res.status}).`));
	}
	return (await res.json()) as MantleStatsResponse;
}

export function formatBytes(bytes: number): string {
	if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
	const units = ['B', 'KB', 'MB', 'GB'];
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit += 1;
	}
	return `${value < 10 && unit > 0 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}
