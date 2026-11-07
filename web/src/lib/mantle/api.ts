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
	/** How many organizations the cache knows; the list itself is paged (`fetchMantleOrgs`). */
	org_count: number;
};

export type OrgSummary = { org: string; bytes: number; entries: number };

export type MantleOrgSort = 'bytes' | 'entries' | 'name';

export type MantleOrgPage = {
	total: number;
	offset: number;
	limit: number;
	orgs: OrgSummary[];
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

export async function fetchMantleOrgs(params: {
	q: string;
	sort: MantleOrgSort;
	offset: number;
	limit: number;
}): Promise<MantleOrgPage> {
	const query = new URLSearchParams({
		q: params.q,
		sort: params.sort,
		offset: String(params.offset),
		limit: String(params.limit)
	});
	const res = await apiFetch(`/api/desk/mantle/orgs?${query}`);
	if (!res.ok) {
		throw new Error(await readError(res, `The organizations could not be loaded (${res.status}).`));
	}
	return (await res.json()) as MantleOrgPage;
}

/** Hot plugins and models of one organization; loaded when its row is opened. */
export async function fetchMantleOrg(org: string): Promise<OrgMantleStats> {
	const res = await apiFetch(`/api/desk/mantle/orgs/${encodeURIComponent(org)}`);
	if (!res.ok) {
		throw new Error(await readError(res, `The organization could not be loaded (${res.status}).`));
	}
	return (await res.json()) as OrgMantleStats;
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
