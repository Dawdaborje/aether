import type { ListSearchPayload, SearchChip, SearchOp } from './types';

let chipSeq = 0;

export function newChipId(): string {
	chipSeq += 1;
	return `c${chipSeq}-${Math.random().toString(36).slice(2, 8)}`;
}

/** Free-text chip field reserved for `db::search` `q`. */
export const Q_FIELD = '_q';

export function makeChip(
	partial: Omit<SearchChip, 'id' | 'label'> & { label?: string; id?: string }
): SearchChip {
	const label =
		partial.label ??
		(partial.field === Q_FIELD
			? String(partial.value ?? '')
			: formatChipLabel(partial.field, partial.op, partial.value));
	return {
		id: partial.id ?? newChipId(),
		field: partial.field,
		op: partial.op,
		value: partial.value,
		label
	};
}

export function formatChipLabel(
	field: string,
	op: SearchOp,
	value: SearchChip['value']
): string {
	if (field === Q_FIELD) return String(value ?? '');
	const name = field.replaceAll('_', ' ');
	if (op === 'null') return value === false ? `${name} is set` : `${name} is empty`;
	if (op === 'like') return `${name} contains “${value}”`;
	if (op === 'in') {
		const list = Array.isArray(value) ? value.join(', ') : String(value ?? '');
		return `${name} is ${list}`;
	}
	if (op === 'ne') return `${name} ≠ ${value}`;
	if (op === 'gt') return `${name} > ${value}`;
	if (op === 'gte') return `${name} ≥ ${value}`;
	if (op === 'lt') return `${name} < ${value}`;
	if (op === 'lte') return `${name} ≤ ${value}`;
	return `${name} is ${value}`;
}

/** Encode chips into a kernel filter object (AND of clauses). Free-text `_q` is omitted. */
export function chipsToFilter(chips: SearchChip[]): Record<string, unknown> {
	const clauses: Record<string, unknown>[] = [];
	for (const chip of chips) {
		if (chip.field === Q_FIELD) continue;
		const clause = chipToClause(chip);
		if (clause) clauses.push(clause);
	}
	if (clauses.length === 0) return {};
	if (clauses.length === 1) return clauses[0] ?? {};
	return { and: clauses };
}

function chipToClause(chip: SearchChip): Record<string, unknown> | null {
	const { field, op, value } = chip;
	if (!field || field === Q_FIELD) return null;
	switch (op) {
		case 'eq':
			return { [field]: value ?? null };
		case 'ne':
			return { [field]: { ne: value ?? null } };
		case 'like':
			return { [field]: { like: String(value ?? '') } };
		case 'in': {
			const list = Array.isArray(value)
				? value
				: String(value ?? '')
						.split(',')
						.map((part) => part.trim())
						.filter(Boolean);
			return { [field]: { in: list } };
		}
		case 'null':
			return { [field]: { null: value !== false } };
		case 'gt':
		case 'gte':
		case 'lt':
		case 'lte':
			return { [field]: { [op]: value } };
		default:
			return { [field]: value ?? null };
	}
}

/** Free-text needle from a `_q` chip, if any. */
export function chipsToQ(chips: SearchChip[]): string | undefined {
	const text = chips
		.filter((chip) => chip.field === Q_FIELD)
		.map((chip) => String(chip.value ?? '').trim())
		.filter(Boolean)
		.join(' ')
		.trim();
	return text || undefined;
}

export function buildListSearchPayload(
	chips: SearchChip[],
	searchFields: string[],
	groupBy: string[]
): ListSearchPayload {
	const filter = chipsToFilter(chips);
	const q = chipsToQ(chips);
	const payload: ListSearchPayload = { filter };
	if (q) {
		payload.q = q;
		if (searchFields.length > 0) payload.search_fields = searchFields;
	}
	if (groupBy.length > 0) payload.group_by = groupBy;
	return payload;
}

/**
 * Best-effort decode of a simple kernel filter into chips.
 * Supports flat equalities and a top-level `and` of simple clauses.
 */
export function filterToChips(filter: Record<string, unknown> | null | undefined): SearchChip[] {
	if (!filter || Object.keys(filter).length === 0) return [];
	if (Array.isArray(filter.and)) {
		return filter.and.flatMap((part) =>
			part && typeof part === 'object' && !Array.isArray(part)
				? filterToChips(part as Record<string, unknown>)
				: []
		);
	}
	const chips: SearchChip[] = [];
	for (const [field, raw] of Object.entries(filter)) {
		if (field === 'and' || field === 'or' || field === 'not') continue;
		if (raw !== null && typeof raw === 'object' && !Array.isArray(raw)) {
			const ops = raw as Record<string, unknown>;
			if ('like' in ops) {
				chips.push(makeChip({ field, op: 'like', value: String(ops.like ?? '') }));
			} else if ('in' in ops && Array.isArray(ops.in)) {
				chips.push(makeChip({ field, op: 'in', value: ops.in.map(String) }));
			} else if ('null' in ops) {
				chips.push(makeChip({ field, op: 'null', value: Boolean(ops.null) }));
			} else if ('ne' in ops) {
				chips.push(makeChip({ field, op: 'ne', value: String(ops.ne ?? '') }));
			} else {
				for (const op of ['gt', 'gte', 'lt', 'lte'] as const) {
					if (op in ops) {
						chips.push(makeChip({ field, op, value: String(ops[op] ?? '') }));
						break;
					}
				}
			}
		} else {
			chips.push(makeChip({ field, op: 'eq', value: raw == null ? null : String(raw) }));
		}
	}
	return chips;
}
