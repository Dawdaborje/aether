import { describe, expect, it } from 'vitest';
import { offsetsOf, rowAt, windowOf } from './virtual';

describe('virtual list arithmetic', () => {
	const offsets = offsetsOf([10, 20, 30, 40]); // 0 10 30 60 100

	it('adds heights up', () => {
		expect(offsets).toEqual([0, 10, 30, 60, 100]);
	});

	it('finds the row at a position', () => {
		expect([0, 9, 10, 29, 30, 59, 60, 99].map((y) => rowAt(offsets, y))).toEqual([
			0, 0, 1, 1, 2, 2, 3, 3
		]);
		expect(rowAt(offsets, 5000)).toBe(3);
		expect(rowAt([0], 10)).toBe(0);
	});

	it('draws only the rows near the view and spaces out the rest', () => {
		const many = offsetsOf(Array.from({ length: 10_000 }, () => 50));
		const w = windowOf(many, 250_000, 600, 200);
		expect(w.end - w.start).toBeLessThan(25);
		expect(w.before).toBe(w.start * 50);
		expect(w.before + (w.end - w.start) * 50 + w.after).toBe(w.total);
		expect(w.total).toBe(500_000);
	});

	it('draws at least one row, and handles an empty list and the ends', () => {
		expect(windowOf([0], 0, 500)).toEqual({ start: 0, end: 0, before: 0, after: 0, total: 0 });
		const top = windowOf(offsets, 0, 5, 0);
		expect(top.start).toBe(0);
		expect(top.end).toBeGreaterThanOrEqual(1);
		const bottom = windowOf(offsets, 99999, 5, 0);
		expect(bottom.end).toBe(4);
		expect(bottom.after).toBe(0);
	});
});
