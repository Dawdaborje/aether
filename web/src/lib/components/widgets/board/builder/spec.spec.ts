import { describe, expect, it } from 'vitest';
import {
	defaultName,
	formatDimensions,
	formatFilter,
	formatMeasures,
	kindInfo,
	parseDimensions,
	parseFilter,
	parseMeasures,
	problems
} from './spec';

describe('dimensions', () => {
	it('round-trip with their grain', () => {
		const dims = parseDimensions('stage, closed:month');
		expect(dims).toEqual([
			{ field: 'stage', grain: '' },
			{ field: 'closed', grain: 'month' }
		]);
		expect(formatDimensions(dims)).toBe('stage, closed:month');
		expect(parseDimensions('')).toEqual([]);
	});
});

describe('measures', () => {
	it('become pickers when they are simple and stay text when they are not', () => {
		const measures = parseMeasures(
			'deals = count\nvalue = sum(amount)\nrate = ratio(won, deals)\nmax(age)'
		);
		expect(measures.map((m) => m.fn)).toEqual(['count', 'sum', 'custom', 'max']);
		expect(measures[1]).toMatchObject({ name: 'value', field: 'amount' });
		expect(measures[3].name).toBe('');
		expect(formatMeasures(measures)).toBe(
			'deals = count\nvalue = sum(amount)\nrate = ratio(won, deals)\nmax(age)'
		);
	});
	it('leave out a row with no field yet', () => {
		expect(formatMeasures([{ name: 'x', fn: 'sum', field: '', expr: '' }])).toBe('');
	});
	it('are named so that a chart reads well', () => {
		expect(defaultName('count', '', [])).toBe('records');
		expect(defaultName('sum', 'amount', [])).toBe('total_amount');
		expect(defaultName('sum', 'amount', ['total_amount'])).toBe('total_amount_2');
	});
});

describe('filters', () => {
	it('read as rows', () => {
		const filter = parseFilter('{"state":"won","amount":{"gte":100},"closed":{"null":false}}');
		expect(filter.raw).toBeNull();
		expect(filter.conditions).toEqual([
			{ field: 'state', op: 'eq', value: 'won' },
			{ field: 'amount', op: 'gte', value: '100' },
			{ field: 'closed', op: 'null', value: 'false' }
		]);
	});
	it('are kept as written when they have more in them than rows can show', () => {
		for (const text of [
			'{"or":[{"a":1},{"b":2}]}',
			'{"a":{"gte":1,"lt":5}}',
			'{"a":{"in":[1,2]}}',
			'not json'
		]) {
			expect(parseFilter(text).raw).toBe(text);
		}
	});
	it('write numbers as numbers and join two limits on one field', () => {
		const text = formatFilter(
			{
				raw: null,
				conditions: [
					{ field: 'amount', op: 'gte', value: '10' },
					{ field: 'amount', op: 'lt', value: '99' },
					{ field: 'name', op: 'eq', value: '7' },
					{ field: 'active', op: 'eq', value: 'true' },
					{ field: 'closed', op: 'null', value: 'true' }
				]
			},
			{ amount: 'decimal', name: 'string', active: 'bool' }
		);
		expect(JSON.parse(text)).toEqual({
			amount: { gte: 10, lt: 99 },
			name: '7',
			active: true,
			closed: { null: true }
		});
	});
	it('is empty without conditions', () => {
		expect(formatFilter({ conditions: [], raw: null }, {})).toBe('');
	});
});

describe('what a chart type needs', () => {
	it('is told in plain words', () => {
		expect(problems('bar', [], [])).toHaveLength(2);
		expect(
			problems(
				'bar',
				[{ field: 'stage', grain: '' }],
				[{ name: 'n', fn: 'count', field: '', expr: '' }]
			)
		).toEqual([]);
		expect(problems('number', [], [{ name: 'n', fn: 'count', field: '', expr: '' }])).toEqual([]);
		expect(
			problems(
				'sankey',
				[{ field: 'a', grain: '' }],
				[{ name: 'n', fn: 'count', field: '', expr: '' }]
			)[0]
		).toMatch(/at least 2/);
		expect(kindInfo('nope').value).toBe('bar');
	});
});
