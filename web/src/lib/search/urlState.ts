import type { SearchChip } from './types';
import { Q_FIELD, filterToChips, makeChip } from './filterCodec';

export interface SearchUrlState {
	chips: SearchChip[];
	groupBy: string[];
}

type CompactChip = {
	f: string;
	o: string;
	v?: string | string[] | boolean | null;
};

/** Encode chips + group_by into URL search params (`f`, `g`). */
export function encodeSearchParams(chips: SearchChip[], groupBy: string[]): URLSearchParams {
	const params = new URLSearchParams();
	const compact: CompactChip[] = chips.map((chip) => {
		const row: CompactChip = { f: chip.field, o: chip.op };
		if (chip.op !== 'null' || chip.value === false) row.v = chip.value ?? null;
		else if (chip.op === 'null') row.v = true;
		else row.v = chip.value ?? null;
		return row;
	});
	if (compact.length > 0) {
		params.set('f', JSON.stringify(compact));
	}
	if (groupBy.length > 0) {
		params.set('g', groupBy.join(','));
	}
	return params;
}

/** Read chips + group_by from the current URL (or a params object). */
export function decodeSearchParams(params: URLSearchParams): SearchUrlState {
	const groupBy = (params.get('g') ?? '')
		.split(',')
		.map((part) => part.trim())
		.filter(Boolean);
	const raw = params.get('f');
	if (!raw) return { chips: [], groupBy };
	try {
		const parsed = JSON.parse(raw) as unknown;
		if (!Array.isArray(parsed)) return { chips: [], groupBy };
		const chips: SearchChip[] = [];
		for (const row of parsed) {
			if (!row || typeof row !== 'object') continue;
			const f = String((row as CompactChip).f ?? '').trim();
			const o = String((row as CompactChip).o ?? 'eq').trim();
			if (!f) continue;
			chips.push(
				makeChip({
					field: f === 'q' ? Q_FIELD : f,
					op: (o as SearchChip['op']) || 'eq',
					value: (row as CompactChip).v
				})
			);
		}
		return { chips, groupBy };
	} catch {
		return { chips: [], groupBy };
	}
}

/** Apply encoded state onto an existing URL, preserving unrelated params. */
export function mergeSearchIntoUrl(
	url: URL,
	chips: SearchChip[],
	groupBy: string[]
): URL {
	const next = new URL(url);
	next.searchParams.delete('f');
	next.searchParams.delete('g');
	const encoded = encodeSearchParams(chips, groupBy);
	for (const [key, value] of encoded) {
		next.searchParams.set(key, value);
	}
	return next;
}

/** Hydrate chips from a kernel filter JSON string (favorites). */
export function chipsFromFilterJson(filter: unknown, q?: string | null): SearchChip[] {
	const base =
		filter && typeof filter === 'object' && !Array.isArray(filter)
			? filterToChips(filter as Record<string, unknown>)
			: [];
	const needle = q?.trim();
	if (needle) {
		return [...base, makeChip({ field: Q_FIELD, op: 'like', value: needle })];
	}
	return base;
}
