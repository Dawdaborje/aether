import { describe, expect, it } from 'vitest';
import type { AppTileData } from '$lib/components/apps/types';
import {
	activeInstalledApp,
	builtInNav,
	deskNav,
	isMenuCurrent,
	manageNavItems,
	pathUnderApp
} from './nav';

describe('builtInNav', () => {
	it('offers desk Users to developers', () => {
		expect(builtInNav(true).items.map((item) => item.href)).toContain('/desk/users');
	});

	it('offers org Users and Roles when canManageUsers', () => {
		expect(builtInNav(false, true).items.map((item) => item.href)).toEqual(['/users', '/roles']);
	});

	it('offers Settings when canManageOrg', () => {
		expect(builtInNav(false, true, true).items.map((item) => item.href)).toEqual([
			'/users',
			'/roles',
			'/desk/settings'
		]);
	});

	it('offers Schedules to developers of the current org only', () => {
		expect(builtInNav(false, false, false, true).items.map((item) => item.href)).toEqual([
			'/schedules'
		]);
		expect(builtInNav(false, true, true, false).items.some((item) => item.href === '/schedules')).toBe(
			false
		);
	});

	it('hides Users for plain members', () => {
		expect(builtInNav(false, false).items.some((item) => item.label === 'Users')).toBe(false);
	});
});

describe('manageNavItems', () => {
	it('lists desk tools for developers', () => {
		expect(manageNavItems(true).map((item) => item.href)).toEqual([
			'/desk/organizations',
			'/desk/users',
			'/desk/apps',
			'/desk/settings',
			'/desk/schedules',
			'/desk/mantle',
			'/desk/bridges'
		]);
	});

	it('lists org Users and Roles for managers only', () => {
		expect(manageNavItems(false, true).map((item) => item.href)).toEqual(['/users', '/roles']);
		expect(manageNavItems(false, false)).toEqual([]);
	});

	it('lists Users, Roles and Settings for org admins', () => {
		expect(manageNavItems(false, true, true).map((item) => item.href)).toEqual([
			'/users',
			'/roles',
			'/desk/settings'
		]);
		expect(manageNavItems(false, false, true).map((item) => item.href)).toEqual([
			'/desk/settings'
		]);
	});

	it('adds Schedules after the rest for an org developer', () => {
		expect(manageNavItems(false, true, true, true).map((item) => item.href)).toEqual([
			'/users',
			'/roles',
			'/desk/settings',
			'/schedules'
		]);
	});

	it('keeps desk Schedules (not the org page) for unrestricted developers', () => {
		const hrefs = manageNavItems(true, true, true, true).map((item) => item.href);
		expect(hrefs).toContain('/desk/schedules');
		expect(hrefs).not.toContain('/schedules');
	});
});

describe('deskNav', () => {
	it('includes Users', () => {
		expect(deskNav.items.some((item) => item.href === '/desk/users')).toBe(true);
	});

	it('includes Schedules', () => {
		expect(deskNav.items.some((item) => item.href === '/desk/schedules')).toBe(true);
	});

	it('includes Mantle', () => {
		expect(deskNav.items.some((item) => item.href === '/desk/mantle')).toBe(true);
	});
});

const apps: AppTileData[] = [
	{
		plugin: 'country',
		label: 'Countries',
		icon: 'globe',
		route: '/base/countries',
		description: null,
		menus: [{ label: 'Countries', href: '/base/countries' }]
	},
	{
		plugin: 'company',
		label: 'Companies',
		icon: 'building',
		route: '/base/companies',
		description: null
	},
	{
		plugin: 'party',
		label: 'Parties',
		icon: 'users',
		route: '/base/parties',
		description: null
	}
];

describe('activeInstalledApp', () => {
	it('matches the app entry route', () => {
		expect(activeInstalledApp(apps, '/web/base/countries')?.plugin).toBe('country');
	});

	it('matches pages under the app route including /new', () => {
		expect(activeInstalledApp(apps, '/web/base/countries/new')?.label).toBe('Countries');
		expect(activeInstalledApp(apps, '/web/base/countries/abc')?.plugin).toBe('country');
	});

	it('prefers the page plugin when several routes could match', () => {
		const nested: AppTileData[] = [
			...apps,
			{
				plugin: 'country_setup',
				label: 'Country setup',
				icon: null,
				route: '/base/countries/setup',
				description: null
			}
		];
		expect(activeInstalledApp(nested, '/web/base/countries/setup', 'country_setup')?.plugin).toBe(
			'country_setup'
		);
		expect(activeInstalledApp(nested, '/web/base/countries/setup')?.plugin).toBe('country_setup');
	});

	it('returns null when nothing matches', () => {
		expect(activeInstalledApp(apps, '/web/apps')).toBeNull();
		expect(activeInstalledApp([], '/web/base/countries')).toBeNull();
		expect(activeInstalledApp(null, '/web/base/countries')).toBeNull();
	});

	it('picks the page plugin over a sibling in the same workspace folder', () => {
		const hrms: AppTileData[] = [
			{
				plugin: 'hr',
				category: 'hrms',
				label: 'Employees',
				icon: 'users',
				route: '/hr/employees',
				description: null,
				menus: [{ label: 'Employees', href: '/hr/employees' }]
			},
			{
				plugin: 'hr_attendance',
				category: 'hrms',
				label: 'Attendance',
				icon: 'clock',
				route: '/hr/attendance',
				description: null,
				menus: [{ label: 'Attendance', href: '/hr/attendance' }]
			}
		];
		expect(activeInstalledApp(hrms, '/web/hr/attendance', 'hr_attendance')?.plugin).toBe(
			'hr_attendance'
		);
		expect(activeInstalledApp(hrms, '/web/hr/attendance')?.plugin).toBe('hr_attendance');
		expect(activeInstalledApp(hrms, '/web/hr/employees')?.plugin).toBe('hr');
	});

	it('resolves a folded thin app to the workspace host that owns the menu', () => {
		// API folds single-menu apps into the host; hr_attendance is no longer a tile.
		const folded: AppTileData[] = [
			{
				plugin: 'hr',
				category: 'hrms',
				label: 'Human Resources',
				icon: 'users',
				route: '/hr/employees',
				description: null,
				menus: [
					{ label: 'Employees', href: '/hr/employees', group: null },
					{ label: 'Attendance', href: '/hr/attendance', group: 'Attendance' }
				]
			}
		];
		expect(activeInstalledApp(folded, '/web/hr/attendance', 'hr_attendance')?.plugin).toBe('hr');
		expect(activeInstalledApp(folded, '/web/hr/attendance')?.menus?.map((m) => m.href)).toContain(
			'/hr/attendance'
		);
	});
});

describe('pathUnderApp', () => {
	it('requires an absolute app route', () => {
		expect(pathUnderApp('/web/base/countries', 'base/countries')).toBe(false);
	});
});

describe('isMenuCurrent', () => {
	it('prefers the longer matching sibling', () => {
		const employees = { label: 'Employees', href: '/hr/employees' };
		const hire = { label: 'New employee', href: '/hr/employees/hire' };
		const siblings = [employees, hire];
		expect(isMenuCurrent(hire, '/web/hr/employees/hire', siblings)).toBe(true);
		expect(isMenuCurrent(employees, '/web/hr/employees/hire', siblings)).toBe(false);
		expect(isMenuCurrent(employees, '/web/hr/employees', siblings)).toBe(true);
	});
});
