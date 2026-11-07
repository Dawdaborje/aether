/**
 * The arithmetic of a virtual list whose rows have different heights. Heights not measured yet are guessed
 * (`estimate`); once a row is on screen its real height replaces the guess. Only the rows near the visible part
 * are drawn, and spacers stand in for the rest.
 */
export interface Window {
	/** First row to draw. */
	start: number;
	/** One past the last row to draw. */
	end: number;
	/** Height of the space above the first drawn row. */
	before: number;
	/** Height of the space below the last drawn row. */
	after: number;
	/** Height of the whole list. */
	total: number;
}

/** `offsets[i]` is the top of row `i`; the last entry is the total height. */
export function offsetsOf(heights: number[]): number[] {
	const offsets = new Array<number>(heights.length + 1);
	offsets[0] = 0;
	for (let i = 0; i < heights.length; i++) offsets[i + 1] = offsets[i] + heights[i];
	return offsets;
}

/** The first row whose bottom is below `y` (binary search over the offsets). */
export function rowAt(offsets: number[], y: number): number {
	const count = offsets.length - 1;
	if (count <= 0) return 0;
	let low = 0;
	let high = count - 1;
	while (low < high) {
		const mid = (low + high) >> 1;
		if (offsets[mid + 1] <= y) low = mid + 1;
		else high = mid;
	}
	return low;
}

/** The rows to draw for a viewport of `height` pixels scrolled to `top`, with `overscan` pixels drawn beyond each edge. */
export function windowOf(offsets: number[], top: number, height: number, overscan = 400): Window {
	const count = offsets.length - 1;
	const total = offsets[count] ?? 0;
	if (count <= 0) return { start: 0, end: 0, before: 0, after: 0, total };
	const start = rowAt(offsets, Math.max(0, top - overscan));
	let end = rowAt(offsets, top + height + overscan) + 1;
	end = Math.min(count, Math.max(end, start + 1));
	return { start, end, before: offsets[start], after: total - offsets[end], total };
}
