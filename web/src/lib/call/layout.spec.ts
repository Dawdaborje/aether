import { describe, expect, it } from 'vitest';
import { columnsFor, initials, orderTiles } from './layout';
import type { Participant } from './client';

const person = (user: string, since: number, extra: Partial<Participant> = {}): Participant => ({
	sid: `s-${user}`,
	user,
	name: user,
	role: 'member',
	audio: true,
	video: true,
	screen: false,
	since,
	...extra
});

describe('columnsFor', () => {
	it('grows with the square root and stops at the maximum', () => {
		expect([0, 1, 2, 4, 5, 9, 10, 40].map((n) => columnsFor(n))).toEqual([1, 1, 2, 2, 3, 3, 4, 4]);
		expect(columnsFor(40, 6)).toBe(6);
	});
});

describe('orderTiles', () => {
	const roster = [person('cy', 3), person('ann', 1), person('bob', 2)];

	it('keeps the order people joined in', () => {
		expect(orderTiles(roster, 'zed', new Set(), null).map((t) => t.person.user)).toEqual(['ann', 'bob', 'cy']);
	});

	it('puts the pinned person first, then speakers, then you', () => {
		const tiles = orderTiles(roster, 'cy', new Set(['s-bob']), 'ann');
		expect(tiles.map((t) => t.person.user)).toEqual(['ann', 'bob', 'cy']);
		const spoken = orderTiles(roster, 'ann', new Set(['s-cy']), null);
		expect(spoken.map((t) => t.person.user)).toEqual(['cy', 'ann', 'bob']);
	});

	it('marks which tile is you', () => {
		expect(orderTiles(roster, 'bob', new Set(), null).filter((t) => t.you).map((t) => t.person.user)).toEqual(['bob']);
	});
});

describe('initials', () => {
	it('takes the first and last word', () => {
		expect(initials('Ann Marie Lee')).toBe('AL');
		expect(initials('ann')).toBe('AN');
		expect(initials('  ')).toBe('?');
	});
});
