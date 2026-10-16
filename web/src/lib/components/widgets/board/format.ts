/** Numbers as people read them, and the geometry the SVG charts need. */

export type Format = 'number' | 'compact' | 'currency' | 'percent' | 'days';

/** A figure in the chart's format; an em dash for no figure. */
export function formatValue(
	value: unknown,
	format: string | null | undefined = 'number',
	currency?: string | null
): string {
	if (value === null || value === undefined || value === '') return '—';
	const n = typeof value === 'number' ? value : Number(value);
	if (!Number.isFinite(n)) return String(value);
	switch (format) {
		case 'compact':
			return new Intl.NumberFormat(undefined, {
				notation: 'compact',
				maximumFractionDigits: 1
			}).format(n);
		case 'currency':
			try {
				return new Intl.NumberFormat(undefined, {
					style: 'currency',
					currency: currency || 'USD',
					maximumFractionDigits: 2
				}).format(n);
			} catch {
				return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(n);
			}
		case 'percent':
			return new Intl.NumberFormat(undefined, {
				style: 'percent',
				maximumFractionDigits: 1
			}).format(n);
		case 'days':
			return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(n)} d`;
		default:
			return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(n);
	}
}

/** `+12.5%` / `−3%`; an em dash without a change. */
export function formatChange(change: number | null | undefined): string {
	if (change === null || change === undefined || !Number.isFinite(change)) return '—';
	const pct = Math.round(Math.abs(change) * 1000) / 10;
	if (pct === 0) return '0%';
	return `${change > 0 ? '+' : '−'}${pct}%`;
}

function niceNumber(x: number): number {
	const exponent = Math.floor(Math.log10(x));
	const fraction = x / Math.pow(10, exponent);
	const nice =
		fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 2.5 ? 2.5 : fraction <= 5 ? 5 : 10;
	return nice * Math.pow(10, exponent);
}

/** Round axis bounds and the ticks between them, so gridlines fall on friendly numbers. */
export function niceScale(
	min: number,
	max: number,
	ticks = 5
): { min: number; max: number; ticks: number[] } {
	const low = Math.min(0, min);
	let high = Math.max(0, max);
	if (high === low) high = low + 1;
	const step = niceNumber((high - low) / Math.max(1, ticks - 1));
	const start = Math.floor(low / step) * step;
	const end = Math.ceil(high / step) * step;
	const out: number[] = [];
	for (let value = start; value <= end + step / 2; value += step) {
		out.push(Math.round(value / step) * step);
	}
	return { min: start, max: end, ticks: out };
}

/** The colour of the i-th series: the theme's five, then lighter mixes of them. */
export function seriesColor(index: number): string {
	const base = `var(--chart-${(index % 5) + 1})`;
	if (index < 5) return base;
	return `color-mix(in oklab, ${base} 60%, var(--background))`;
}

/** An SVG path for a pie slice or a donut segment from angle `start` to `end` (radians, 0 at the top). */
export function arcPath(
	cx: number,
	cy: number,
	radius: number,
	inner: number,
	start: number,
	end: number
): string {
	const sweep = Math.min(end - start, Math.PI * 2 - 0.0001);
	const point = (r: number, angle: number) =>
		`${(cx + r * Math.sin(angle)).toFixed(2)} ${(cy - r * Math.cos(angle)).toFixed(2)}`;
	const large = sweep > Math.PI ? 1 : 0;
	const last = start + sweep;
	if (inner <= 0) {
		return `M ${cx} ${cy} L ${point(radius, start)} A ${radius} ${radius} 0 ${large} 1 ${point(radius, last)} Z`;
	}
	return (
		`M ${point(radius, start)} A ${radius} ${radius} 0 ${large} 1 ${point(radius, last)} ` +
		`L ${point(inner, last)} A ${inner} ${inner} 0 ${large} 0 ${point(inner, start)} Z`
	);
}

/** Shorten a label to `max` characters with an ellipsis. */
export function clip(label: string, max: number): string {
	return label.length <= max ? label : `${label.slice(0, Math.max(1, max - 1))}…`;
}
