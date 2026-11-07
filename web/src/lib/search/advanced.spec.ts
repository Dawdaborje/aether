import { describe, expect, it } from 'vitest';
import { describeGroup, groupToFilter, operatorsFor, valueKind } from './advanced';
import { chipsToFilter, makeChip } from './filterCodec';
import type { FieldDef, RuleGroup } from './types';

const fields: FieldDef[] = [
	{ field: 'name', label: 'Name', type: 'text' },
	{ field: 'age', label: 'Age', type: 'integer' },
	{
		field: 'status',
		label: 'Status',
		type: 'selection',
		options: [
			{ value: 'draft', label: 'Draft' },
			{ value: 'done', label: 'Done' }
		]
	}
];

describe('advanced filter', () => {
	it('builds age >= 18 and name starting with A', () => {
		const tree: RuleGroup = {
			mode: 'and',
			items: [
				{ field: 'age', op: 'gte', value: '18' },
				{ field: 'name', op: 'starts', value: 'A' }
			]
		};
		expect(groupToFilter(tree, fields)).toEqual({
			and: [{ age: { gte: 18 } }, { name: { starts: 'A' } }]
		});
	});

	it('nests any-of groups and skips incomplete rules', () => {
		const tree: RuleGroup = {
			mode: 'and',
			items: [
				{ field: 'age', op: 'between', value: '18', value2: '65' },
				{ field: 'name', op: 'like', value: '' },
				{
					mode: 'or',
					items: [
						{ field: 'status', op: 'in', value: 'draft, done' },
						{ field: 'name', op: 'unset', value: '' }
					]
				}
			]
		};
		expect(groupToFilter(tree, fields)).toEqual({
			and: [
				{ age: { between: [18, 65] } },
				{ or: [{ status: { in: ['draft', 'done'] } }, { name: { null: true } }] }
			]
		});
	});

	it('an empty tree matches everything', () => {
		expect(groupToFilter({ mode: 'and', items: [] }, fields)).toEqual({});
	});

	it('offers operators by field kind', () => {
		expect(valueKind(fields[1])).toBe('number');
		expect(operatorsFor('number')).toContain('between');
		expect(operatorsFor('text')).toContain('starts');
		expect(valueKind(fields[2])).toBe('choice');
	});

	it('describes the rules in words', () => {
		const tree: RuleGroup = {
			mode: 'or',
			items: [
				{ field: 'age', op: 'gte', value: '18' },
				{ field: 'name', op: 'starts', value: 'A' }
			]
		};
		expect(describeGroup(tree, fields)).toBe(
			'Age is greater than or equal to 18 or Name starts with A'
		);
	});

	it('a domain chip joins the other chips', () => {
		const tree: RuleGroup = { mode: 'and', items: [{ field: 'age', op: 'gt', value: '30' }] };
		const filter = groupToFilter(tree, fields);
		const chips = [
			makeChip({ field: 'dept', op: 'eq', value: 'hr' }),
			makeChip({ field: '_adv', op: 'domain', value: { filter, tree } })
		];
		expect(chipsToFilter(chips)).toEqual({ and: [{ dept: 'hr' }, { age: { gt: 30 } }] });
	});
});
