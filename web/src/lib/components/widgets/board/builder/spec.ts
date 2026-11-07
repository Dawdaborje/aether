/**
 * The chart builder's choices and the text a chart is stored as. A chart keeps its group-by
 * fields as `stage, closed:month`, its measures as lines of `name = sum(amount)` and its filter as
 * JSON; the builder shows them as pickers and writes them back, keeping whatever it cannot show
 * as a picker (a ratio, an `or` in a filter) exactly as it was.
 */

export const GRAINS = ['day', 'week', 'month', 'quarter', 'year'] as const;

export interface Dim {
	field: string;
	grain: string;
}

export interface Measure {
	name: string;
	/** `count`, `sum`, `avg`, `min` or `max` have a picker; `custom` is an expression kept as written. */
	fn: 'count' | 'sum' | 'avg' | 'min' | 'max' | 'custom';
	field: string;
	expr: string;
}

export interface Condition {
	field: string;
	op: 'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte' | 'like' | 'null';
	value: string;
}

export interface Filter {
	conditions: Condition[];
	/** The filter as written, when it has more in it than the rows can show. */
	raw: string | null;
}

export const OPERATORS: { op: Condition['op']; label: string }[] = [
	{ op: 'eq', label: 'is' },
	{ op: 'ne', label: 'is not' },
	{ op: 'gt', label: 'is more than' },
	{ op: 'gte', label: 'is at least' },
	{ op: 'lt', label: 'is less than' },
	{ op: 'lte', label: 'is at most' },
	{ op: 'like', label: 'contains' },
	{ op: 'null', label: 'is empty / has a value' }
];

export function parseDimensions(text: string | null | undefined): Dim[] {
	return (text ?? '')
		.split(',')
		.map((part) => part.trim())
		.filter(Boolean)
		.map((part) => {
			const [field, grain] = part.split(':').map((piece) => piece.trim());
			return { field, grain: grain ?? '' };
		});
}

export function formatDimensions(dims: Dim[]): string {
	return dims
		.filter((dim) => dim.field)
		.map((dim) => (dim.grain ? `${dim.field}:${dim.grain}` : dim.field))
		.join(', ');
}

const SIMPLE = /^(sum|avg|min|max)\(\s*([A-Za-z0-9_]+)\s*\)$/;

export function parseMeasures(text: string | null | undefined): Measure[] {
	return (text ?? '')
		.split(/\n|;/)
		.map((line) => line.trim())
		.filter(Boolean)
		.map((line) => {
			const match = /^([A-Za-z0-9_]+)\s*=\s*(.*)$/.exec(line);
			const expr = (match ? match[2] : line).trim();
			const name = match ? match[1] : '';
			if (expr === 'count') return { name, fn: 'count', field: '', expr };
			const simple = SIMPLE.exec(expr);
			if (simple) return { name, fn: simple[1] as Measure['fn'], field: simple[2], expr };
			return { name, fn: 'custom', field: '', expr };
		});
}

export function measureExpression(measure: Measure): string {
	if (measure.fn === 'count') return 'count';
	if (measure.fn === 'custom') return measure.expr.trim();
	return measure.field ? `${measure.fn}(${measure.field})` : '';
}

export function formatMeasures(measures: Measure[]): string {
	return measures
		.map((measure) => {
			const expr = measureExpression(measure);
			if (!expr) return '';
			return measure.name ? `${measure.name} = ${expr}` : expr;
		})
		.filter(Boolean)
		.join('\n');
}

/** A name for a new measure that reads well on a chart: `total_amount`, `average_age`, `records`. */
export function defaultName(fn: Measure['fn'], field: string, taken: string[]): string {
	const base =
		fn === 'count'
			? 'records'
			: fn === 'custom' || !field
				? 'value'
				: `${{ sum: 'total', avg: 'average', min: 'lowest', max: 'highest' }[fn]}_${field}`;
	let name = base;
	for (let n = 2; taken.includes(name); n += 1) name = `${base}_${n}`;
	return name;
}

const OPS = new Set<string>(OPERATORS.map((o) => o.op));

function scalar(value: unknown): string | null {
	if (typeof value === 'string') return value;
	if (typeof value === 'number' || typeof value === 'boolean') return String(value);
	return null;
}

/** Rows for a filter made only of `field: value` and `field: { op: value }`; anything else is kept as it was. */
export function parseFilter(text: string | null | undefined): Filter {
	const trimmed = (text ?? '').trim();
	if (!trimmed || trimmed === '{}') return { conditions: [], raw: null };
	let parsed: unknown;
	try {
		parsed = JSON.parse(trimmed);
	} catch {
		return { conditions: [], raw: trimmed };
	}
	if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
		return { conditions: [], raw: trimmed };
	}
	const conditions: Condition[] = [];
	for (const [field, value] of Object.entries(parsed as Record<string, unknown>)) {
		if (['and', 'or', 'not'].includes(field)) return { conditions: [], raw: trimmed };
		const plain = scalar(value);
		if (plain !== null) {
			conditions.push({ field, op: 'eq', value: plain });
			continue;
		}
		const ops =
			value && typeof value === 'object' && !Array.isArray(value) ? Object.entries(value) : [];
		if (ops.length !== 1 || !OPS.has(ops[0][0])) return { conditions: [], raw: trimmed };
		const [op, operand] = ops[0];
		const shown = scalar(operand);
		if (shown === null) return { conditions: [], raw: trimmed };
		conditions.push({ field, op: op as Condition['op'], value: shown });
	}
	return { conditions, raw: null };
}

function typed(value: string, kind: string): unknown {
	if (kind === 'bool') return value === 'true';
	if (
		['int', 'float', 'decimal'].includes(kind) &&
		value.trim() !== '' &&
		!Number.isNaN(Number(value))
	) {
		return Number(value);
	}
	return value;
}

/**
 * The filter as JSON. `kinds` maps a field to its type, so `5` is a number for an amount and text for a
 * name. Two conditions on one field join into one object (`{ "amount": { "gte": 1, "lt": 9 } }`).
 */
export function formatFilter(filter: Filter, kinds: Record<string, string>): string {
	if (filter.raw !== null) return filter.raw;
	const out: Record<string, unknown> = {};
	for (const { field, op, value } of filter.conditions) {
		if (!field) continue;
		const kind = kinds[field] ?? 'string';
		const operand = op === 'null' ? value !== 'false' : typed(value, kind);
		const current = out[field];
		if (op === 'eq' && current === undefined) {
			out[field] = operand;
			continue;
		}
		const ops: Record<string, unknown> =
			current && typeof current === 'object'
				? (current as Record<string, unknown>)
				: current === undefined
					? {}
					: { eq: current };
		ops[op] = operand;
		out[field] = ops;
	}
	return Object.keys(out).length ? JSON.stringify(out) : '';
}

/** What each chart type wants from the pickers. */
export interface KindInfo {
	value: string;
	label: string;
	/** Group-by fields it takes: at least, at most. */
	dims: [number, number];
	/** Measures it shows: at least, at most. */
	measures: [number, number];
	hint: string;
	/** Names the group-by slots, in order. */
	dimLabels: string[];
	measureLabels?: string[];
}

export const KINDS: KindInfo[] = [
	{
		value: 'number',
		label: 'Number',
		dims: [0, 0],
		measures: [1, 4],
		dimLabels: [],
		hint: 'One or more figures, with the change from the period before and a goal.'
	},
	{
		value: 'bar',
		label: 'Bar',
		dims: [1, 2],
		measures: [1, 4],
		dimLabels: ['Across', 'Split by'],
		hint: 'Compare groups. A second field splits each bar.'
	},
	{
		value: 'stacked_bar',
		label: 'Stacked bar',
		dims: [1, 2],
		measures: [1, 4],
		dimLabels: ['Across', 'Stack by'],
		hint: 'Bars made of parts.'
	},
	{
		value: 'row',
		label: 'Row',
		dims: [1, 2],
		measures: [1, 4],
		dimLabels: ['Down', 'Split by'],
		hint: 'Bars lying down, for long names.'
	},
	{
		value: 'line',
		label: 'Line',
		dims: [1, 2],
		measures: [1, 4],
		dimLabels: ['Along', 'Split by'],
		hint: 'A trend. Group by a date and pick a grain.'
	},
	{
		value: 'area',
		label: 'Area',
		dims: [1, 2],
		measures: [1, 4],
		dimLabels: ['Along', 'Stack by'],
		hint: 'A trend, filled in.'
	},
	{
		value: 'pie',
		label: 'Pie',
		dims: [1, 1],
		measures: [1, 1],
		dimLabels: ['Slices'],
		hint: 'Shares of a whole, for few groups.'
	},
	{
		value: 'donut',
		label: 'Donut',
		dims: [1, 1],
		measures: [1, 1],
		dimLabels: ['Slices'],
		hint: 'Shares of a whole, with room for the total.'
	},
	{
		value: 'funnel',
		label: 'Funnel',
		dims: [1, 1],
		measures: [1, 1],
		dimLabels: ['Stages'],
		hint: 'Stages from the largest down, with the share kept.'
	},
	{
		value: 'map',
		label: 'Map',
		dims: [1, 1],
		measures: [1, 1],
		dimLabels: ['Country'],
		hint: 'A bubble per country. The field holds country names or codes.'
	},
	{
		value: 'bubble',
		label: 'Bubble',
		dims: [1, 1],
		measures: [2, 3],
		dimLabels: ['One bubble per'],
		measureLabels: ['Across', 'Up', 'Size'],
		hint: 'Two measures place each bubble; a third sets its size.'
	},
	{
		value: 'sankey',
		label: 'Flow',
		dims: [2, 4],
		measures: [1, 1],
		dimLabels: ['From', 'Then', 'Then', 'Then'],
		hint: 'How a quantity flows from one field to the next. Use count or sum.'
	},
	{
		value: 'table',
		label: 'Table',
		dims: [0, 4],
		measures: [1, 6],
		dimLabels: ['Row', 'Then', 'Then', 'Then'],
		hint: 'The figures in a grid, with totals.'
	},
	{
		value: 'pivot',
		label: 'Pivot',
		dims: [1, 2],
		measures: [1, 1],
		dimLabels: ['Rows', 'Columns'],
		hint: 'One figure across two fields, with totals.'
	},
	{
		value: 'heatmap',
		label: 'Heatmap',
		dims: [1, 2],
		measures: [1, 1],
		dimLabels: ['Rows', 'Columns'],
		hint: 'A pivot where every cell is shaded by its size.'
	}
];

export function kindInfo(kind: string): KindInfo {
	return KINDS.find((k) => k.value === kind) ?? KINDS[1];
}

/** What is wrong with the choices, in plain words; empty when the chart can run. */
export function problems(kind: string, dims: Dim[], measures: Measure[]): string[] {
	const info = kindInfo(kind);
	const out: string[] = [];
	const chosenDims = dims.filter((d) => d.field).length;
	const chosenMeasures = measures.filter((m) => measureExpression(m)).length;
	if (chosenDims < info.dims[0]) {
		out.push(
			info.dims[0] === 1
				? `Pick the field to group by (${info.dimLabels[0].toLowerCase()}).`
				: `Pick at least ${info.dims[0]} fields to group by.`
		);
	}
	if (chosenMeasures < info.measures[0]) {
		out.push(
			info.measures[0] === 1
				? 'Add a measure, such as a count or a sum.'
				: `This chart needs ${info.measures[0]} measures.`
		);
	}
	return out;
}
