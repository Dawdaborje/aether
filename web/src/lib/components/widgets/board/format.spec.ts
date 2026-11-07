import { describe, expect, it } from 'vitest';
import { arcPath, clip, formatChange, formatValue, niceScale, seriesColor } from './format';

describe('formatValue', () => {
	it('shows an em dash for no figure', () => {
		expect(formatValue(null)).toBe('—');
		expect(formatValue(undefined)).toBe('—');
		expect(formatValue('')).toBe('—');
	});

	it('formats each kind', () => {
		expect(formatValue(1234.567, 'number')).toMatch(/1.234.57/);
		expect(formatValue(1500, 'compact')).toMatch(/1\.5K|1,5K/i);
		expect(formatValue(0.256, 'percent')).toMatch(/25\.6|25,6/);
		expect(formatValue(3.25, 'days')).toMatch(/3[.,]3? d|3[.,]2 d/);
		expect(formatValue('12.5', 'number')).toMatch(/12.5/);
		expect(formatValue(9, 'currency', 'EUR')).toMatch(/9/);
	});

	it('survives a bad currency code', () => {
		expect(formatValue(9, 'currency', 'not-a-code')).toMatch(/9/);
	});
});

describe('formatChange', () => {
	it('shows the direction and rounds to a tenth of a percent', () => {
		expect(formatChange(0.125)).toBe('+12.5%');
		expect(formatChange(-0.03)).toBe('−3%');
		expect(formatChange(0)).toBe('0%');
		expect(formatChange(null)).toBe('—');
	});
});

describe('niceScale', () => {
	it('rounds the top up to a friendly number', () => {
		const scale = niceScale(0, 947);
		expect(scale.min).toBe(0);
		expect(scale.max).toBe(1000);
		expect(scale.ticks).toEqual([0, 250, 500, 750, 1000]);
	});

	it('always includes zero and copes with all-zero data', () => {
		expect(niceScale(5, 9).min).toBe(0);
		const flat = niceScale(0, 0);
		expect(flat.max).toBeGreaterThan(flat.min);
	});

	it('handles negative values', () => {
		const scale = niceScale(-40, 90);
		expect(scale.min).toBeLessThanOrEqual(-40);
		expect(scale.max).toBeGreaterThanOrEqual(90);
		expect(scale.ticks).toContain(0);
	});
});

describe('seriesColor', () => {
	it('uses the theme palette, then lighter mixes', () => {
		expect(seriesColor(0)).toBe('var(--chart-1)');
		expect(seriesColor(4)).toBe('var(--chart-5)');
		expect(seriesColor(5)).toContain('color-mix');
	});
});

describe('arcPath', () => {
	it('draws a closed pie slice and a donut segment', () => {
		expect(arcPath(50, 50, 40, 0, 0, Math.PI / 2)).toMatch(/^M 50 50 L .* A 40 40 0 0 1 .* Z$/);
		expect(arcPath(50, 50, 40, 20, 0, Math.PI)).toContain('A 20 20');
	});

	it('marks arcs over half a turn as large', () => {
		expect(arcPath(50, 50, 40, 0, 0, Math.PI * 1.5)).toContain('0 1 1');
	});

	it('does not collapse a full circle', () => {
		const path = arcPath(50, 50, 40, 0, 0, Math.PI * 2);
		expect(path).not.toContain('NaN');
	});
});

describe('clip', () => {
	it('adds an ellipsis only when needed', () => {
		expect(clip('short', 10)).toBe('short');
		expect(clip('a very long label', 8)).toBe('a very …');
	});
});
