import { describe, expect, it } from 'vitest';
import { interpolate, keyOf, rowsOf, text, totalOf } from './pageContext.svelte';

describe('rowsOf', () => {
	it('takes a list, or one held under rows, items or data', () => {
		expect(rowsOf([{ a: 1 }])).toEqual([{ a: 1 }]);
		expect(rowsOf({ rows: [{ a: 1 }] })).toEqual([{ a: 1 }]);
		expect(rowsOf({ items: [{ a: 2 }] })).toEqual([{ a: 2 }]);
		expect(rowsOf({ data: [{ a: 3 }] })).toEqual([{ a: 3 }]);
	});
	it('is empty for anything else, and skips what is not a record', () => {
		expect(rowsOf(null)).toEqual([]);
		expect(rowsOf('text')).toEqual([]);
		expect(rowsOf({ nothing: [] })).toEqual([]);
		expect(rowsOf([1, null, { a: 1 }])).toEqual([{ a: 1 }]);
	});
});

describe('totalOf', () => {
	it('reads total or count from an object answer', () => {
		expect(totalOf({ rows: [], total: 12 })).toBe(12);
		expect(totalOf({ rows: [], count: 3 })).toBe(3);
		expect(totalOf([{ a: 1 }])).toBeUndefined();
		expect(totalOf({ rows: [] })).toBeUndefined();
	});
});

describe('record ids', () => {
	it('uses the key after the table name', () => {
		expect(keyOf('notes_note:abc')).toBe('abc');
		expect(keyOf('abc')).toBe('abc');
		expect(keyOf(undefined)).toBe('');
	});
	it('fills route placeholders from a row, with the id as its key', () => {
		expect(interpolate('/notes/{id}', { id: 'notes_note:abc' })).toBe('/notes/abc');
		expect(interpolate('/c/{channel}/{id}', { id: 't:1', channel: 'a b' })).toBe('/c/a%20b/1');
		expect(interpolate('/x/{missing}', {})).toBe('/x/');
	});
});

describe('text', () => {
	it('is the attribute when it is a non-empty string', () => {
		expect(text('hi')).toBe('hi');
		expect(text('  ')).toBeUndefined();
		expect(text(true)).toBeUndefined();
	});
});
