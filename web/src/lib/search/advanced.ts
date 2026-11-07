import type { FieldDef, Rule, RuleGroup } from './types';

/** The operators the builder offers, with the words shown for them. */
export const OP_LABELS: Record<string, string> = {
	eq: 'is',
	ne: 'is not',
	like: 'contains',
	nlike: 'does not contain',
	starts: 'starts with',
	ends: 'ends with',
	in: 'is any of',
	nin: 'is none of',
	gt: 'is greater than',
	gte: 'is greater than or equal to',
	lt: 'is less than',
	lte: 'is less than or equal to',
	between: 'is between',
	set: 'is set',
	unset: 'is not set'
};

const NUMBER_KINDS = new Set(['integer', 'int', 'float', 'number', 'decimal', 'currency']);
const DATE_KINDS = new Set(['date', 'datetime']);
const BOOL_KINDS = new Set(['boolean', 'bool']);

export type ValueKind = 'text' | 'number' | 'date' | 'datetime' | 'boolean' | 'choice';

/** How a field's value is typed in the builder. */
export function valueKind(def: FieldDef | undefined): ValueKind {
	if (!def) return 'text';
	const type = def.type.trim().toLowerCase();
	if (BOOL_KINDS.has(type)) return 'boolean';
	if (def.options && def.options.length > 0) return 'choice';
	if (NUMBER_KINDS.has(type)) return 'number';
	if (type === 'datetime') return 'datetime';
	if (DATE_KINDS.has(type)) return 'date';
	return 'text';
}

/** Operator keys for a kind of field (`set` / `unset` are written as `null` rules). */
export function operatorsFor(kind: ValueKind): string[] {
	switch (kind) {
		case 'boolean':
			return ['eq', 'ne'];
		case 'choice':
			return ['eq', 'ne', 'in', 'nin', 'set', 'unset'];
		case 'number':
		case 'date':
		case 'datetime':
			return ['eq', 'ne', 'gt', 'gte', 'lt', 'lte', 'between', 'set', 'unset'];
		default:
			return ['like', 'nlike', 'eq', 'ne', 'starts', 'ends', 'set', 'unset'];
	}
}

/** `set` and `unset` have no value to type. */
export function takesNoValue(op: string): boolean {
	return op === 'set' || op === 'unset';
}

export function isGroup(item: Rule | RuleGroup): item is RuleGroup {
	return 'items' in item;
}

export function emptyGroup(): RuleGroup {
	return { mode: 'and', items: [] };
}

export function newRule(fields: FieldDef[]): Rule {
	const first = fields[0];
	return { field: first?.field ?? '', op: operatorsFor(valueKind(first))[0]!, value: '' };
}

function typed(raw: string, kind: ValueKind): unknown {
	const text = raw.trim();
	if (kind === 'number') {
		const n = Number(text);
		return text !== '' && Number.isFinite(n) ? n : text;
	}
	if (kind === 'boolean') return text === 'true';
	return text;
}

function list(raw: string, kind: ValueKind): unknown[] {
	return raw
		.split(',')
		.map((part) => part.trim())
		.filter(Boolean)
		.map((part) => typed(part, kind));
}

/** One rule as a kernel filter, or null while it is incomplete. */
export function ruleToFilter(rule: Rule, fields: FieldDef[]): Record<string, unknown> | null {
	if (!rule.field) return null;
	const kind = valueKind(fields.find((f) => f.field === rule.field));
	const { field } = rule;
	if (rule.op === 'set') return { [field]: { null: false } };
	if (rule.op === 'unset') return { [field]: { null: true } };
	if (rule.op === 'between') {
		if (rule.value.trim() === '' || (rule.value2 ?? '').trim() === '') return null;
		return { [field]: { between: [typed(rule.value, kind), typed(rule.value2 ?? '', kind)] } };
	}
	if (rule.op === 'in' || rule.op === 'nin') {
		const values = list(rule.value, kind);
		return values.length === 0 ? null : { [field]: { [rule.op]: values } };
	}
	if (rule.value.trim() === '' && kind !== 'boolean') return null;
	const value = typed(rule.value, kind);
	if (rule.op === 'eq') return { [field]: value };
	return { [field]: { [rule.op]: value } };
}

/**
 * A group as a kernel filter: its complete rules joined by and / or. Incomplete rules and empty
 * groups are skipped, so a half-written rule never narrows the list. `{}` matches everything.
 */
export function groupToFilter(group: RuleGroup, fields: FieldDef[]): Record<string, unknown> {
	const parts: Record<string, unknown>[] = [];
	for (const item of group.items) {
		const part = isGroup(item) ? groupToFilter(item, fields) : ruleToFilter(item, fields);
		if (part && Object.keys(part).length > 0) parts.push(part);
	}
	if (parts.length === 0) return {};
	if (parts.length === 1) return parts[0]!;
	return { [group.mode]: parts };
}

/** A short sentence for the chip: “age ≥ 18 and name starts with “A””. */
export function describeGroup(group: RuleGroup, fields: FieldDef[]): string {
	const parts: string[] = [];
	for (const item of group.items) {
		if (isGroup(item)) {
			const inner = describeGroup(item, fields);
			if (inner) parts.push(`(${inner})`);
		} else if (ruleToFilter(item, fields)) {
			const label = fields.find((f) => f.field === item.field)?.label ?? item.field;
			const op = OP_LABELS[item.op] ?? item.op;
			const value = takesNoValue(item.op)
				? ''
				: item.op === 'between'
					? ` ${item.value} and ${item.value2 ?? ''}`
					: ` ${item.value}`;
			parts.push(`${label} ${op}${value}`);
		}
	}
	return parts.join(group.mode === 'or' ? ' or ' : ' and ');
}
