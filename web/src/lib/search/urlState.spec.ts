import { describe, expect, it } from 'vitest';
import { Q_FIELD, makeChip } from './filterCodec';
import { decodeSearchParams, encodeSearchParams, mergeSearchIntoUrl } from './urlState';

describe('urlState', () => {
	it('round-trips chips and group_by', () => {
		const chips = [
			makeChip({ field: 'status', op: 'eq', value: 'active' }),
			makeChip({ field: Q_FIELD, op: 'like', value: 'ann' })
		];
		const params = encodeSearchParams(chips, ['department_id']);
		const decoded = decodeSearchParams(params);
		expect(decoded.groupBy).toEqual(['department_id']);
		expect(decoded.chips.map((c) => ({ field: c.field, op: c.op, value: c.value }))).toEqual([
			{ field: 'status', op: 'eq', value: 'active' },
			{ field: Q_FIELD, op: 'like', value: 'ann' }
		]);
	});

	it('preserves unrelated query params when merging', () => {
		const url = new URL('http://localhost/web/hr/employees?tab=list');
		const next = mergeSearchIntoUrl(
			url,
			[makeChip({ field: 'status', op: 'eq', value: 'active' })],
			[]
		);
		expect(next.searchParams.get('tab')).toBe('list');
		expect(next.searchParams.get('f')).toBeTruthy();
	});
});
