import { describe, expect, it } from 'vitest';
import type { NavItem } from '$lib/theme/types';
import { navHref } from '../../nav';
import {
	allLinks,
	currentLink,
	fitCount,
	holds,
	megaColumns,
	pick,
	NAV_VARIANTS
} from './navModel';

const tree: NavItem[] = [
	{ label: 'Home', href: '/home' },
	{
		label: 'People',
		children: [
			{ label: 'Employees', href: '/hr/employees' },
			{ label: 'New employee', href: '/hr/employees/new' },
			{ label: 'Teams', children: [{ label: 'All teams', href: '/hr/teams' }] }
		]
	}
];

describe('nav model', () => {
	it('lists every link depth first', () => {
		expect(allLinks(tree).map((l) => l.label)).toEqual([
			'Home',
			'Employees',
			'New employee',
			'All teams'
		]);
	});

	it('marks only the longest matching link current', () => {
		expect(currentLink(tree, navHref('/hr/employees/new'))?.label).toBe('New employee');
		expect(currentLink(tree, navHref('/hr/employees/42'))?.label).toBe('Employees');
		expect(currentLink(tree, navHref('/elsewhere'))).toBeNull();
		const current = currentLink(tree, navHref('/hr/teams'));
		expect(holds(tree[1]!, current)).toBe(true);
		expect(holds(tree[0]!, current)).toBe(false);
	});

	it('fits tabs and leaves room for More', () => {
		expect(fitCount([50, 50, 50], 200, 40)).toBe(3);
		expect(fitCount([50, 50, 50], 140, 40)).toBe(2);
		expect(fitCount([50, 50, 50], 100, 40)).toBe(1);
		expect(fitCount([50, 50, 50], 30, 40)).toBe(0);
		expect(fitCount([50, 50], 104, 40, 4)).toBe(2);
	});

	it('builds mega columns from nested groups', () => {
		const flat = megaColumns({ label: 'G', children: [{ label: 'A', href: '/a' }] });
		expect(flat.columns).toEqual([]);
		expect(flat.cells.map((c) => c.label)).toEqual(['A']);
		const nested = megaColumns(tree[1]!);
		expect(nested.columns.map((c) => c.heading)).toEqual([null, 'Teams']);
		expect(nested.columns[0]!.links.map((l) => l.label)).toEqual(['Employees', 'New employee']);
		expect(nested.columns[1]!.links.map((l) => l.label)).toEqual(['All teams']);
	});

	it('falls back to the default for an unknown variant', () => {
		expect(pick(NAV_VARIANTS, 'tabs', 'links')).toBe('tabs');
		expect(pick(NAV_VARIANTS, 'wheel', 'links')).toBe('links');
	});
});
