import { describe, expect, it } from 'vitest';
import {
	UNASSIGNED,
	boardGridTemplate,
	columnProgress,
	groupKeyOf,
	groupRows,
	parseBoardLayout,
	parseColors,
	progressColorClass
} from './kanban';

describe('groupRows', () => {
	it('groups by field and uses Unassigned for blanks', () => {
		const cols = groupRows(
			[
				{ id: 1, stage: 'new' },
				{ id: 2, stage: 'done' },
				{ id: 3, stage: 'new' },
				{ id: 4, stage: '' },
				{ id: 5 }
			],
			'stage'
		);
		expect(cols.map((c) => c.key)).toEqual(['new', 'done', UNASSIGNED]);
		expect(cols[0].rows).toHaveLength(2);
		expect(cols[2].rows).toHaveLength(2);
	});

	it('reads nested name objects', () => {
		expect(groupKeyOf({ stage: { name: 'Backlog' } }, 'stage')).toBe('Backlog');
	});
});

describe('progress', () => {
	it('parses color maps and aggregates', () => {
		expect(parseColors('{"done":"success","blocked":"danger"}')).toEqual({
			done: 'success',
			blocked: 'danger'
		});
		expect(progressColorClass('danger')).toContain('destructive');
		const prog = columnProgress(
			[
				{ priority: 'high', amount: 10 },
				{ priority: 'high', amount: 5 },
				{ priority: 'normal', amount: 1 }
			],
			'priority',
			{ high: 'danger', normal: '200' },
			'amount'
		);
		expect(prog.total).toBe(3);
		expect(prog.sum).toBe(16);
		expect(prog.segments.find((s) => s.key === 'high')?.count).toBe(2);
	});
});

describe('board layout', () => {
	it('parses 2-1 style weights', () => {
		expect(parseBoardLayout('2-1')).toEqual([2, 1]);
		expect(boardGridTemplate([2, 1])).toBe('2fr 1fr');
		expect(parseBoardLayout('')).toEqual([]);
	});
});
