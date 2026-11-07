import { apiFetch } from '$lib/api/client';

export type AuditTable = 'data_access' | 'plugin_calls' | 'page_visits';

export const AUDIT_TABLES: { id: AuditTable; label: string; hint: string }[] = [
	{ id: 'data_access', label: 'Data access', hint: 'Every database read and write a plugin made' },
	{ id: 'plugin_calls', label: 'Plugin calls', hint: 'Who called which plugin function' },
	{ id: 'page_visits', label: 'Page visits', hint: 'Who opened which page, including refusals' }
];

export type AuditRow = {
	id: string;
	date_created: string;
	request_id: string;
	actor_type: string;
	actor_id?: string | null;
	plugin?: string | null;
	function_name?: string | null;
	model?: string | null;
	table_name?: string | null;
	operation?: string | null;
	record_ids?: string[];
	record_count?: number;
	statement?: string | null;
	route?: string | null;
	path?: string | null;
	method?: string | null;
	status?: number | null;
	ip?: string | null;
	user_agent?: string | null;
	chain_seq?: number | null;
};

export type AuditQuery = {
	table: AuditTable;
	limit?: number;
	offset?: number;
	actor?: string;
	plugin?: string;
	request?: string;
	operation?: string;
	failed?: boolean;
	q?: string;
	/** Only rows from this many hours back; 0 or missing means any time. */
	withinHours?: number;
};

export type AuditPage = {
	table: AuditTable;
	total: number;
	offset: number;
	limit: number;
	rows: AuditRow[];
};

export type ChainHead = { chain: string; seq: number; hash: string; base_seq?: number };

export type ChainFailure = { seq?: number | null; rule: string; detail: string };
export type ChainReport = {
	chain: string;
	ok: boolean;
	rows_checked: number;
	head_seq: number;
	unsealed: number;
	checkpoints_checked: number;
	failures: ChainFailure[];
	more_failures: number;
};

export type AuditChainStatus = {
	heads: ChainHead[];
	last_verification: { ok: boolean; at: string; report?: { chains?: ChainReport[] } } | null;
};

function base(db: string): string {
	return `/api/ui/organizations/${encodeURIComponent(db)}/audit`;
}

async function readError(res: Response, fallback: string): Promise<string> {
	const body = await res.json().catch(() => ({}));
	return typeof body.error === 'string' ? body.error : fallback;
}

/** The query string for a list request; empty filters are left out. */
export function auditParams(query: AuditQuery, now: Date = new Date()): URLSearchParams {
	const params = new URLSearchParams();
	params.set('table', query.table);
	params.set('limit', String(query.limit ?? 50));
	params.set('offset', String(query.offset ?? 0));
	for (const key of ['actor', 'plugin', 'request', 'operation', 'q'] as const) {
		const value = query[key]?.trim();
		if (value) params.set(key, value);
	}
	if (query.failed) params.set('failed', 'true');
	if (query.withinHours && query.withinHours > 0) {
		params.set('from', new Date(now.getTime() - query.withinHours * 3_600_000).toISOString());
	}
	return params;
}

export async function fetchAudit(db: string, query: AuditQuery): Promise<AuditPage> {
	const res = await apiFetch(`${base(db)}?${auditParams(query)}`);
	if (!res.ok)
		throw new Error(await readError(res, `The audit log could not be loaded (${res.status}).`));
	return (await res.json()) as AuditPage;
}

export async function fetchAuditChain(db: string): Promise<AuditChainStatus> {
	const res = await apiFetch(`${base(db)}/chain`);
	if (!res.ok)
		throw new Error(await readError(res, `The audit chain could not be loaded (${res.status}).`));
	return (await res.json()) as AuditChainStatus;
}

export async function verifyAuditChain(
	db: string
): Promise<{ ok: boolean; chains: ChainReport[] }> {
	const res = await apiFetch(`${base(db)}/verify`, { method: 'POST' });
	if (!res.ok) throw new Error(await readError(res, `The check could not run (${res.status}).`));
	return (await res.json()) as { ok: boolean; chains: ChainReport[] };
}

/** Who did it, as one short phrase. */
export function actorLabel(row: Pick<AuditRow, 'actor_type' | 'actor_id'>): string {
	if (row.actor_type === 'anonymous') return 'Anonymous';
	return row.actor_id ? `${row.actor_type} ${row.actor_id}` : row.actor_type;
}

/** What happened, as one short phrase, for the table's main column. */
export function summarize(table: AuditTable, row: AuditRow): string {
	if (table === 'data_access') {
		const what = row.model ?? row.table_name ?? 'raw query';
		const count = row.record_count ?? 0;
		return `${row.operation ?? 'access'} ${what}${count > 0 ? ` (${count})` : ''}`;
	}
	if (table === 'plugin_calls') return `${row.plugin ?? '?'}.${row.function_name ?? '?'}`;
	return `${row.method ?? 'GET'} ${row.path ?? ''}`.trim();
}

/** A request that was refused or failed (status 400 and up). */
export function isRefused(row: Pick<AuditRow, 'status'>): boolean {
	return typeof row.status === 'number' && row.status >= 400;
}

/** A sentence about the chain's last check, for the banner. */
export function chainSummary(status: AuditChainStatus | null): {
	ok: boolean | null;
	text: string;
} {
	const last = status?.last_verification;
	if (!last) return { ok: null, text: 'This organization’s audit chain has not been checked yet.' };
	const when = new Date(last.at);
	const at = Number.isNaN(when.getTime()) ? last.at : when.toLocaleString();
	if (last.ok) return { ok: true, text: `Audit chain intact when last checked, ${at}.` };
	const broken = (last.report?.chains ?? []).filter((c) => !c.ok).map((c) => c.chain);
	return {
		ok: false,
		text: `Audit chain failed its check on ${at}${broken.length ? `: ${broken.join(', ')}` : ''}. Rows may have been changed or removed.`
	};
}
