import type { PageNode } from '$lib/dsl/types';

export const UNASSIGNED = 'Unassigned';

export interface KanbanColumn {
	key: string;
	label: string;
	rows: Record<string, unknown>[];
}

export interface ProgressSegment {
	key: string;
	count: number;
	color: string;
}

export interface ColumnProgress {
	segments: ProgressSegment[];
	total: number;
	sum: number | null;
}

/** Group-by value for a row; blank → Unassigned. */
export function groupKeyOf(row: Record<string, unknown>, field: string): string {
	if (!field) return UNASSIGNED;
	const raw = row[field];
	if (raw === null || raw === undefined || raw === '') return UNASSIGNED;
	if (typeof raw === 'object' && raw !== null && 'name' in raw) {
		const name = (raw as { name?: unknown }).name;
		if (name !== null && name !== undefined && String(name) !== '') return String(name);
	}
	return String(raw);
}

/** Stable columns from rows, preserving first-seen order. */
export function groupRows(rows: Record<string, unknown>[], field: string): KanbanColumn[] {
	const order: string[] = [];
	const map = new Map<string, Record<string, unknown>[]>();
	for (const row of rows) {
		const key = groupKeyOf(row, field);
		if (!map.has(key)) {
			map.set(key, []);
			order.push(key);
		}
		map.get(key)!.push(row);
	}
	return order.map((key) => ({
		key,
		label: key,
		rows: map.get(key) ?? []
	}));
}

/** Parse progressbar `colors='{"done":"success"}'` JSON attribute. */
export function parseColors(raw: unknown): Record<string, string> {
	if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
		const out: Record<string, string> = {};
		for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
			if (typeof v === 'string') out[k] = v;
		}
		return out;
	}
	if (typeof raw !== 'string' || !raw.trim()) return {};
	try {
		const parsed = JSON.parse(raw) as unknown;
		return parseColors(parsed);
	} catch {
		return {};
	}
}

/** Map Odoo-ish color tokens to Tailwind background classes. */
export function progressColorClass(token: string): string {
	const t = token.toLowerCase();
	if (t === 'danger' || t === 'red') return 'bg-destructive';
	if (t === 'success' || t === 'green') return 'bg-emerald-500';
	if (t === 'warning' || t === 'orange') return 'bg-amber-500';
	if (t === 'info' || t === 'blue') return 'bg-sky-500';
	if (t === 'primary') return 'bg-primary';
	if (/^\d+$/.test(t)) return 'bg-muted-foreground/40';
	return 'bg-muted-foreground/50';
}

export function columnProgress(
	rows: Record<string, unknown>[],
	field: string,
	colors: Record<string, string>,
	sumField?: string
): ColumnProgress {
	const counts = new Map<string, number>();
	for (const row of rows) {
		const key = groupKeyOf(row, field);
		counts.set(key, (counts.get(key) ?? 0) + 1);
	}
	const segments: ProgressSegment[] = [];
	for (const [key, count] of counts) {
		segments.push({
			key,
			count,
			color: progressColorClass(colors[key] ?? '200')
		});
	}
	segments.sort((a, b) => b.count - a.count);
	let sum: number | null = null;
	if (sumField) {
		sum = rows.reduce((acc, row) => {
			const v = row[sumField];
			return acc + (typeof v === 'number' ? v : Number(v) || 0);
		}, 0);
	}
	return { segments, total: rows.length, sum };
}

export function formatCell(value: unknown, kind: string): string {
	if (value === null || value === undefined) return '';
	if (kind === 'boolean') return value ? '✓' : '';
	if (kind === 'date' || kind === 'datetime') {
		const date = new Date(String(value));
		return Number.isNaN(date.getTime())
			? String(value)
			: kind === 'date'
				? date.toLocaleDateString()
				: date.toLocaleString();
	}
	if (kind === 'currency' && typeof value === 'number') {
		return value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
	}
	if (typeof value === 'object' && value !== null && 'name' in value) {
		return String((value as { name?: unknown }).name ?? '');
	}
	return typeof value === 'object' ? JSON.stringify(value) : String(value);
}

export function initialsOf(value: unknown): string {
	const text = formatCell(value, '');
	if (!text) return '?';
	const parts = text.trim().split(/\s+/).slice(0, 2);
	return parts.map((p) => p[0]?.toUpperCase() ?? '').join('') || '?';
}

/** Card field nodes (excluding footer). */
export function cardBodyFields(card: PageNode | undefined): PageNode[] {
	return (card?.children ?? []).filter((c) => c.type !== 'footer');
}

export function cardFooter(card: PageNode | undefined): PageNode | undefined {
	return (card?.children ?? []).find((c) => c.type === 'footer');
}

/** Title field: first non-footer field name, or `title` / `name`. */
export function cardTitleField(card: PageNode | undefined, row: Record<string, unknown>): string {
	const declared = cardBodyFields(card);
	if (declared.length > 0) {
		return String(declared[0].name ?? declared[0].field ?? '');
	}
	if (typeof row.title === 'string') return 'title';
	if (typeof row.name === 'string') return 'name';
	const key = Object.keys(row).find((k) => k !== 'id' && typeof row[k] === 'string');
	return key ?? '';
}

/** Parse board `layout="2-1"` into fractional column weights. */
export function parseBoardLayout(layout: unknown): number[] {
	if (typeof layout !== 'string' || !layout.trim()) return [];
	const parts = layout
		.trim()
		.split(/[-:,\s]+/)
		.map((p) => Number(p))
		.filter((n) => Number.isFinite(n) && n > 0);
	return parts.length > 0 ? parts : [];
}

export function boardGridTemplate(weights: number[]): string | undefined {
	if (weights.length === 0) return undefined;
	return weights.map((w) => `${w}fr`).join(' ');
}
