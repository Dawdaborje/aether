import { describe, expect, it } from 'vitest';
import { fit, initial, peopleFor, readPeople, userKey } from './presence';

describe('presence helpers', () => {
	it('compares ids by their key', () => {
		expect(userKey('users:abc')).toBe('abc');
		expect(userKey('abc')).toBe('abc');
		expect(userKey(null)).toBe('');
	});

	it('names the people it knows and shows the end of the id for the others, once each', () => {
		const known = readPeople([
			{ id: 'users:ann', name: 'Ann Lee' },
			{ id: 'bob', display_name: 'Bob' }
		]);
		const people = peopleFor(['users:ann', 'users:zzzzzzzzzz', 'bob', 'ann'], known);
		expect(people.map((p) => p.name)).toEqual(['Ann Lee', 'Bob', 'zzzzzz']);
	});

	it('reads both shapes of answer and ignores rubbish', () => {
		expect(readPeople({ people: [{ id: '1', name: 'A' }] }).get('1')?.name).toBe('A');
		expect(readPeople('x').size).toBe(0);
		expect(readPeople([null, 3, { name: 'no id' }]).size).toBe(0);
	});

	it('says how many avatars fit and how many are left', () => {
		const people = ['a', 'b', 'c', 'd'].map((id) => ({ id, name: id }));
		expect(fit(people, 3)).toEqual({ shown: people.slice(0, 3), more: 1 });
		expect(fit(people, 10).more).toBe(0);
		expect(fit(people, 0).shown).toHaveLength(1);
		expect(initial(' ann')).toBe('A');
		expect(initial('')).toBe('?');
	});
});
