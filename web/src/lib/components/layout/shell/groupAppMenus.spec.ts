import { describe, expect, it } from 'vitest';
import { groupAppMenus } from './groupAppMenus';

describe('groupAppMenus', () => {
	it('keeps ungrouped items as top-level links', () => {
		expect(
			groupAppMenus([
				{ label: 'Employees', href: '/hr/employees', icon: 'users' },
				{ label: 'Departments', href: '/hr/departments', icon: 'building' }
			])
		).toEqual([
			{ label: 'Employees', href: '/hr/employees', icon: 'users' },
			{ label: 'Departments', href: '/hr/departments', icon: 'building' }
		]);
	});

	it('flattens a one-page group to a single link', () => {
		expect(
			groupAppMenus([
				{ label: 'Attendance', href: '/hr/attendance', group: 'Attendance', icon: 'clock' }
			])
		).toEqual([{ label: 'Attendance', href: '/hr/attendance', icon: 'clock' }]);
	});

	it('keeps multi-page groups as collapsible sections', () => {
		expect(
			groupAppMenus([
				{ label: 'Employees', href: '/hr/employees', group: 'Employee', icon: 'users' },
				{ label: 'Departments', href: '/hr/departments', group: 'Employee', icon: 'building' },
				{ label: 'Attendance', href: '/hr/attendance', group: 'Attendance', icon: 'clock' }
			])
		).toEqual([
			{
				label: 'Employee',
				children: [
					{ label: 'Employees', href: '/hr/employees', icon: 'users' },
					{ label: 'Departments', href: '/hr/departments', icon: 'building' }
				]
			},
			{ label: 'Attendance', href: '/hr/attendance', icon: 'clock' }
		]);
	});
});
