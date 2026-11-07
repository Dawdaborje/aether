import { describe, expect, it } from 'vitest';
import type { NavItem } from '$lib/theme/types';
import { navHref } from '../nav';
import { menuPath, shellTrail } from './trail';

const menus: NavItem[] = [
	{ label: 'Overview', href: '/hr' },
	{
		label: 'People',
		children: [
			{ label: 'Employees', href: '/hr/employees' },
			{ label: 'Teams', href: '/hr/teams' }
		]
	}
];
const app = { label: 'HR', href: '/hr' };

describe('shell trail', () => {
	it('finds an item and its groups', () => {
		expect(menuPath(menus, menus[1]!.children![1]!).map((i) => i.label)).toEqual([
			'People',
			'Teams'
		]);
		expect(menuPath(menus, null)).toEqual([]);
	});

	it('uses the menu when the page publishes nothing', () => {
		const trail = shellTrail({ app, menus, pathname: navHref('/hr/teams'), page: null });
		expect(trail).toEqual([{ label: 'HR', href: '/hr' }, { label: 'People' }, { label: 'Teams' }]);
	});

	it('puts the menu groups above a page that publishes only its title', () => {
		const trail = shellTrail({
			app,
			menus,
			pathname: navHref('/hr/employees/42'),
			page: [{ label: 'Ada Lovelace' }]
		});
		expect(trail.map((c) => c.label)).toEqual(['HR', 'People', 'Employees', 'Ada Lovelace']);
		expect(trail[2]).toEqual({ label: 'Employees', href: '/hr/employees' });
		expect(trail[3]).toEqual({ label: 'Ada Lovelace' });
	});

	it("keeps a page's own trail, adds the scope and drops the app's home page", () => {
		const trail = shellTrail({
			org: 'Acme',
			app,
			menus,
			pathname: navHref('/hr'),
			page: [
				{ label: 'Overview', href: '/hr' },
				{ label: 'Roles', href: '/hr/roles' },
				{ label: 'editor' }
			]
		});
		expect(trail).toEqual([
			{ label: 'Acme' },
			{ label: 'HR', href: '/hr' },
			{ label: 'Roles', href: '/hr/roles' },
			{ label: 'editor' }
		]);
	});

	it('the home page is just the app', () => {
		expect(shellTrail({ app, menus, pathname: navHref('/hr'), page: null })).toEqual([
			{ label: 'HR' }
		]);
	});
});
