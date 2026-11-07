import { describe, expect, it } from 'vitest';
import { chipsOf } from './reactions';

describe('reactions', () => {
	it('counts people and notices the viewer', () => {
		const chips = chipsOf(
			{ '👍': ['users:ann', 'users:bob'], '🎉': ['users:ann'], '😮': [] },
			'ann'
		);
		expect(chips.map((c) => [c.emoji, c.count, c.mine])).toEqual([
			['👍', 2, true],
			['🎉', 1, true]
		]);
		expect(chipsOf({ '👍': ['users:bob'] }, 'ann')[0].mine).toBe(false);
	});
	it('reads nothing from rubbish', () => {
		expect(chipsOf(null, 'a')).toEqual([]);
		expect(chipsOf([], 'a')).toEqual([]);
		expect(chipsOf({ '👍': 'x' }, 'a')).toEqual([]);
	});
});
