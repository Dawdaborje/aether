import { describe, expect, it } from 'vitest';
import {
	addDays,
	addMonths,
	daysInMonth,
	formatDate,
	formatDateTime,
	monthGrid,
	parseTyped,
	parseValue,
	toDayNumber,
	fromDayNumber,
	weekday
} from './calendar-math';

describe('calendar math', () => {
	it('knows month lengths and leap years', () => {
		expect(daysInMonth(2024, 2)).toBe(29);
		expect(daysInMonth(2026, 2)).toBe(28);
		expect(daysInMonth(1900, 2)).toBe(28);
		expect(daysInMonth(2000, 2)).toBe(29);
		expect(daysInMonth(2026, 4)).toBe(30);
	});

	it('finds the weekday', () => {
		expect(weekday(1970, 1, 1)).toBe(4); // Thursday
		expect(weekday(2026, 10, 15)).toBe(4); // Thursday
		expect(weekday(2000, 1, 1)).toBe(6); // Saturday
		expect(weekday(1969, 12, 31)).toBe(3); // Wednesday, before the epoch
	});

	it('round-trips day numbers', () => {
		for (const day of [-800000, -1, 0, 1, 19000, 2932896]) {
			expect(toDayNumber(fromDayNumber(day))).toBe(day);
		}
	});

	it('adds days across month and year ends', () => {
		expect(addDays({ y: 2026, m: 12, d: 31 }, 1)).toEqual({ y: 2027, m: 1, d: 1 });
		expect(addDays({ y: 2024, m: 3, d: 1 }, -1)).toEqual({ y: 2024, m: 2, d: 29 });
	});

	it('adds months and clamps the day', () => {
		expect(addMonths({ y: 2026, m: 1, d: 31 }, 1)).toEqual({ y: 2026, m: 2, d: 28 });
		expect(addMonths({ y: 2026, m: 11, d: 15 }, 3)).toEqual({ y: 2027, m: 2, d: 15 });
		expect(addMonths({ y: 2026, m: 1, d: 15 }, -2)).toEqual({ y: 2025, m: 11, d: 15 });
	});

	it('parses and formats values', () => {
		expect(parseValue('2026-10-15')).toEqual({ y: 2026, m: 10, d: 15, hh: 0, mm: 0 });
		expect(parseValue('2026-10-15T09:30')).toEqual({ y: 2026, m: 10, d: 15, hh: 9, mm: 30 });
		expect(parseValue('2026-02-30')).toBeNull();
		expect(parseValue('nonsense')).toBeNull();
		expect(formatDate({ y: 2026, m: 3, d: 5 })).toBe('2026-03-05');
		expect(formatDateTime({ y: 2026, m: 3, d: 5, hh: 7, mm: 4 })).toBe('2026-03-05T07:04');
	});

	it('builds a six-week grid starting on the chosen weekday', () => {
		const grid = monthGrid(2026, 10, 1); // Monday first
		expect(grid).toHaveLength(42);
		expect(grid[0]).toMatchObject({ y: 2026, m: 9, d: 28, inMonth: false });
		expect(grid.find((c) => c.inMonth)).toMatchObject({ d: 1 });
		const sunday = monthGrid(2026, 10, 0);
		expect(sunday[0]).toMatchObject({ m: 9, d: 27 });
	});

	it('reads typed dates', () => {
		expect(parseTyped('2026-10-15')).toEqual({ y: 2026, m: 10, d: 15 });
		expect(parseTyped('15/10/2026')).toEqual({ y: 2026, m: 10, d: 15 });
		expect(parseTyped('10/25/2026')).toEqual({ y: 2026, m: 10, d: 25 });
		expect(parseTyped('15 Oct 2026')).toEqual({ y: 2026, m: 10, d: 15 });
		expect(parseTyped('Oct 15, 2026')).toEqual({ y: 2026, m: 10, d: 15 });
		expect(parseTyped('31/02/2026')).toBeNull();
		expect(parseTyped('')).toBeNull();
	});
});
