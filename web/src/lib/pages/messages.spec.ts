import { describe, expect, it } from 'vitest';
import {
	groupRows,
	prependRows,
	removeRow,
	rowKey,
	trimRows,
	typingText,
	upsertRows
} from './messages';

const f = {
	id: 'id',
	author: 'author',
	name: 'author_name',
	image: 'author_image',
	time: 'created_at',
	body: 'body',
	files: 'files'
};
const at = (min: number) => new Date(Date.UTC(2026, 9, 17, 10, min)).toISOString();
const m = (id: string, author: string, min: number) => ({
	id: `msg:${id}`,
	author,
	created_at: at(min),
	body: id
});

describe('messages', () => {
	it('reads a record key', () => {
		expect(rowKey({ id: 'chat_message:abc' })).toBe('abc');
		expect(rowKey({})).toBe('');
	});

	it('replaces an edited row in place and appends new ones once', () => {
		const rows = [m('1', 'ann', 0), m('2', 'bob', 1)];
		const next = upsertRows(rows, [
			{ ...m('1', 'ann', 0), body: 'edited' },
			m('3', 'ann', 2),
			m('3', 'ann', 2)
		]);
		expect(next.map((r) => r.body)).toEqual(['edited', '2', '3']);
		expect(rows[0].body).toBe('1');
	});

	it('puts older rows in front without repeating any', () => {
		const next = prependRows([m('3', 'a', 2), m('4', 'a', 3)], [m('1', 'a', 0), m('3', 'a', 2)]);
		expect(next.map((r) => r.body)).toEqual(['1', '3', '4']);
	});

	it('removes a row and trims from the far end', () => {
		const rows = [m('1', 'a', 0), m('2', 'a', 1), m('3', 'a', 2)];
		expect(removeRow(rows, '2').map((r) => r.body)).toEqual(['1', '3']);
		expect(trimRows(rows, 2, 'newest').map((r) => r.body)).toEqual(['2', '3']);
		expect(trimRows(rows, 2, 'oldest').map((r) => r.body)).toEqual(['1', '2']);
		expect(trimRows(rows, 5, 'newest')).toBe(rows);
	});

	it('starts a run for a new author, a long pause or a new day', () => {
		const day = (t: number) => new Date(t).toISOString().slice(0, 10);
		const rows = [
			m('1', 'ann', 0),
			m('2', 'ann', 1),
			m('3', 'bob', 2),
			m('4', 'bob', 30),
			{ ...m('5', 'bob', 31), created_at: '2026-10-18T09:00:00Z' }
		];
		const items = groupRows(rows, f, day);
		expect(items.map((i) => i.head)).toEqual([true, false, true, true, true]);
		expect(items.map((i) => i.day)).toEqual(['2026-10-17', null, null, null, '2026-10-18']);
	});

	it('reads an author that is an object', () => {
		const items = groupRows(
			[
				{ id: 'a:1', author: { name: 'Ann' } },
				{ id: 'a:2', author: { name: 'Ann' } }
			],
			f,
			() => ''
		);
		expect(items.map((i) => i.head)).toEqual([true, false]);
	});

	it('says who is typing', () => {
		expect(typingText([])).toBe('');
		expect(typingText(['Ann'])).toBe('Ann is typing…');
		expect(typingText(['Ann', 'Bob'])).toBe('Ann and Bob are typing…');
		expect(typingText(['a', 'b', 'c'])).toBe('Several people are typing…');
	});
});
