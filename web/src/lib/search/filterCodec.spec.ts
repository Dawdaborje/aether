import { describe, expect, it } from 'vitest';
import {
	Q_FIELD,
	buildListSearchPayload,
	chipsToFilter,
	chipsToQ,
	filterToChips,
	makeChip
} from './filterCodec';

describe('filterCodec', () => {
	it('encodes equality chips as a flat filter', () => {
		const chips = [makeChip({ field: 'status', op: 'eq', value: 'active' })];
		expect(chipsToFilter(chips)).toEqual({ status: 'active' });
	});

	it('ands multiple chips and keeps free-text out of the filter', () => {
		const chips = [
			makeChip({ field: 'status', op: 'eq', value: 'active' }),
			makeChip({ field: 'name', op: 'like', value: 'ann' }),
			makeChip({ field: Q_FIELD, op: 'like', value: 'smith' })
		];
		expect(chipsToFilter(chips)).toEqual({
			and: [{ status: 'active' }, { name: { like: 'ann' } }]
		});
		expect(chipsToQ(chips)).toBe('smith');
	});

	it('builds a list payload with search_fields when q is set', () => {
		const chips = [
			makeChip({ field: 'status', op: 'eq', value: 'active' }),
			makeChip({ field: Q_FIELD, op: 'like', value: 'ann' })
		];
		expect(buildListSearchPayload(chips, ['name', 'email'], ['department_id'])).toEqual({
			filter: { status: 'active' },
			q: 'ann',
			search_fields: ['name', 'email'],
			group_by: ['department_id']
		});
	});

	it('round-trips a simple filter through chips', () => {
		const filter = { status: 'active', name: { like: 'ann' } };
		const chips = filterToChips(filter);
		expect(chipsToFilter(chips)).toEqual({
			and: [{ status: 'active' }, { name: { like: 'ann' } }]
		});
	});
});
