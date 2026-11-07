import { describe, expect, it } from 'vitest';
import { initials, orgTree } from './orgchart';

describe('orgTree', () => {
	it('puts people under their manager and starts a tree where the manager is missing', () => {
		const tree = orgTree([
			{ id: 'users:a', name: 'Ann', manager: '' },
			{ id: 'b', name: 'Bob', manager: 'users:a' },
			{ id: 'c', name: 'Cy', manager: 'b' },
			{ id: 'd', name: 'Di', manager: 'gone' }
		]);
		expect(tree.map((n) => n.name)).toEqual(['Ann', 'Di']);
		expect(tree[0].children[0].children[0].name).toBe('Cy');
	});

	it('never loops: a cycle is broken into separate trees', () => {
		const tree = orgTree([
			{ id: 'a', name: 'A', manager: 'b' },
			{ id: 'b', name: 'B', manager: 'a' }
		]);
		expect(tree.length).toBeGreaterThan(0);
	});

	it('makes initials', () => {
		expect(initials('ann lee smith')).toBe('AL');
		expect(initials('')).toBe('');
	});
});
