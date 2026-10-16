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
				icon: 'users',
				children: [
					{ label: 'Employees', href: '/hr/employees', icon: 'users' },
					{ label: 'Departments', href: '/hr/departments', icon: 'building' }
				]
			},
			{ label: 'Attendance', href: '/hr/attendance', icon: 'clock' }
		]);
	});

	it('puts ungrouped Home above groups even when it appears later in the list', () => {
		expect(
			groupAppMenus([
				{ label: 'Applicant sources', href: '/hr/config/recruitment/sources', group: 'Recruitment', icon: 'list' },
				{ label: 'Interviewer pools', href: '/hr/config/recruitment/pools', group: 'Recruitment', icon: 'users' },
				{ label: 'Home', href: '/hr/config', icon: 'layout-dashboard' }
			])
		).toEqual([
			{ label: 'Home', href: '/hr/config', icon: 'layout-dashboard' },
			{
				label: 'Recruitment',
				icon: 'list',
				children: [
					{ label: 'Applicant sources', href: '/hr/config/recruitment/sources', icon: 'list' },
					{ label: 'Interviewer pools', href: '/hr/config/recruitment/pools', icon: 'users' }
				]
			}
		]);
	});

	it('uses a folder icon when a group has no child icons', () => {
		expect(
			groupAppMenus([
				{ label: 'A', href: '/a', group: 'Section' },
				{ label: 'B', href: '/b', group: 'Section' }
			])
		).toEqual([
			{
				label: 'Section',
				icon: 'folder',
				children: [
					{ label: 'A', href: '/a', icon: undefined },
					{ label: 'B', href: '/b', icon: undefined }
				]
			}
		]);
	});

	it('prefers menu_group_icon over the first child icon', () => {
		expect(
			groupAppMenus([
				{
					label: 'Payroll settings',
					href: '/hr/config/payroll/settings',
					group: 'Payroll',
					icon: 'settings',
					group_icon: 'banknote'
				},
				{
					label: 'Salary components',
					href: '/hr/config/payroll/components',
					group: 'Payroll',
					icon: 'puzzle'
				}
			])
		).toEqual([
			{
				label: 'Payroll',
				icon: 'banknote',
				children: [
					{
						label: 'Payroll settings',
						href: '/hr/config/payroll/settings',
						icon: 'settings'
					},
					{
						label: 'Salary components',
						href: '/hr/config/payroll/components',
						icon: 'puzzle'
					}
				]
			}
		]);
	});
});
