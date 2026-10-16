/** What a `board` view's source answers, and what each card holds. */

export interface BoardFilterOption {
	value: unknown;
	label: string;
	count?: number | null;
}

export interface BoardFilter {
	name: string;
	label: string;
	/** `period` (a named period or a from/to range), `select` (pick values) or `text`. */
	kind: 'period' | 'select' | 'text';
	value: unknown;
	options?: BoardFilterOption[];
}

export interface Reading {
	name: string;
	label: string;
	value: number | null;
	previous: number | null;
	/** `(now - previous) / previous`, or null without a previous figure. */
	change: number | null;
}

export interface TrendPoint {
	label: string;
	value: number;
}

export interface CardBase {
	id?: string | null;
	kind: string;
	title: string;
	description?: string | null;
	info?: string | null;
	format?: string | null;
	currency?: string | null;
	meta?: {
		source?: string;
		applied?: string[];
		skipped?: { field: string; reason: string }[];
	};
}

export interface NumberCard extends CardBase {
	kind: 'number';
	readings: Reading[];
	target?: number | null;
	progress?: number | null;
	compared?: boolean;
	trend?: TrendPoint[];
}

export interface Series {
	name: string;
	key?: unknown;
	measure: string;
	values: number[];
}

export interface SeriesCard extends CardBase {
	kind: 'bar' | 'stacked_bar' | 'row' | 'line' | 'area' | 'pie' | 'donut';
	categories: string[];
	keys: unknown[];
	series: Series[];
	/** The fields the categories and the series are grouped by. */
	dimensions: (string | null)[];
	truncated?: boolean;
}

export interface TableColumn {
	key: string;
	label: string;
	type: 'dimension' | 'measure';
}

export interface TableCard extends CardBase {
	kind: 'table';
	columns: TableColumn[];
	rows: { cells: Record<string, unknown>; keys: Record<string, unknown> }[];
	totals?: Record<string, unknown> | null;
	truncated?: boolean;
}

export interface PivotCard extends CardBase {
	kind: 'pivot';
	rows: string[];
	row_keys: unknown[];
	columns: string[];
	column_keys: unknown[];
	matrix: number[][];
	row_totals: number[];
	column_totals: number[];
	grand: number | null;
	measure: string;
	dimensions: (string | null)[];
}

export type Card = NumberCard | SeriesCard | TableCard | PivotCard;

export interface BoardItem {
	id: string;
	chart?: string;
	width?: number;
	title?: string | null;
	card?: Card;
	error?: string;
}

export interface BoardData {
	title?: string | null;
	description?: string | null;
	refresh_secs?: number;
	filters: BoardFilter[];
	items: BoardItem[];
}

/** The records behind a bar, slice or cell. */
export interface DrillData {
	columns: { key: string; label: string }[];
	rows: { id?: string; cells: Record<string, unknown> }[];
	total: number;
	limit: number;
	offset: number;
}
