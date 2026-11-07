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
	/** Whether the change is good or bad news; a chart where lower is better turns it around. */
	tone?: 'good' | 'bad' | 'flat';
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
	/** A goal: a number card shows progress to it, a bar or line chart draws it as a line. */
	target?: number | null;
	lower_is_better?: boolean;
	/** A line over a bar, line or area chart, worked out in the browser from its first series. */
	guide?: 'average' | 'median' | 'trend' | null;
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
	/** Against the goal: met (`on_track`) or not (`behind`). */
	status?: 'on_track' | 'behind' | null;
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
	kind: 'bar' | 'stacked_bar' | 'row' | 'line' | 'area' | 'pie' | 'donut' | 'funnel' | 'map';
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
	kind: 'pivot' | 'heatmap';
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

/** One bubble per group: `x` and `y` place it, `size` (when there is a third measure) scales it. */
export interface BubbleCard extends CardBase {
	kind: 'bubble';
	points: { label: string; key: unknown; x: number; y: number; size: number | null }[];
	axes: { x: string; y: string; size: string | null };
	dimensions: (string | null)[];
	truncated?: boolean;
}

/** Flows between the values of consecutive group-by fields; `source` and `target` index `nodes`. */
export interface SankeyCard extends CardBase {
	kind: 'sankey';
	nodes: { level: number; field: string; key: unknown; name: string; value: number }[];
	links: { source: number; target: number; value: number }[];
	measure: string;
	dimensions: string[];
	truncated?: boolean;
}

/** A note block between charts. */
export interface TextCard extends CardBase {
	kind: 'text';
	text: string;
}

export type Card =
	NumberCard | SeriesCard | TableCard | PivotCard | BubbleCard | SankeyCard | TextCard;

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
