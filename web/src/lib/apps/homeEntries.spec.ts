import { describe, expect, it } from 'vitest';
import { buildHomeEntries } from './homeEntries';
import type { AppCategory, AppTileData } from '$lib/components/apps/types';

function app(partial: Partial<AppTileData> & Pick<AppTileData, 'plugin' | 'label' | 'route'>): AppTileData {
	return {
		icon: null,
		description: null,
		...partial
	};
}

describe('buildHomeEntries', () => {
	const categories: AppCategory[] = [
		{ id: 'base', label: 'Base', launcher: 'flat' },
		{ id: 'hrms', label: 'HRMS', launcher: 'folder' }
	];

	it('lists flat workspace members as individual tiles', () => {
		const apps = [
			app({ plugin: 'country', label: 'Countries', route: '/base/countries', category: 'base' }),
			app({ plugin: 'company', label: 'Companies', route: '/base/companies', category: 'base' })
		];
		const entries = buildHomeEntries(apps, categories);
		expect(entries.map((e) => (e.kind === 'app' ? e.app.plugin : e.id))).toEqual([
			'company',
			'country'
		]);
		expect(entries.every((e) => e.kind === 'app')).toBe(true);
	});

	it('groups folder-mode workspace members into one folder', () => {
		const apps = [
			app({ plugin: 'hr', label: 'Human Resources', route: '/hr/employees', category: 'hrms' }),
			app({ plugin: 'hr_leave', label: 'Leave', route: '/hr/leave', category: 'hrms' })
		];
		const entries = buildHomeEntries(apps, categories);
		expect(entries).toHaveLength(1);
		expect(entries[0]?.kind).toBe('folder');
		if (entries[0]?.kind === 'folder') {
			expect(entries[0].label).toBe('HRMS');
			expect(entries[0].apps).toHaveLength(2);
		}
	});

	it('uses the workspace logo, else its own icon, never a member app logo', () => {
		const withLogo: AppCategory[] = [
			{
				id: 'hrms',
				label: 'HRMS',
				launcher: 'folder',
				logo_url: '/api/ui/workspaces/hrms/logo',
				icon: 'users'
			}
		];
		const apps = [
			app({
				plugin: 'hr',
				label: 'Human Resources',
				route: '/hr/employees',
				category: 'hrms',
				icon: 'users',
				logo_url: '/api/ui/plugins/hr/logo'
			}),
			app({
				plugin: 'hr_leave',
				label: 'Leave',
				route: '/hr/leave',
				category: 'hrms',
				icon: 'calendar-check'
			})
		];
		const entries = buildHomeEntries(apps, withLogo);
		expect(entries[0]?.kind).toBe('folder');
		if (entries[0]?.kind === 'folder') {
			expect(entries[0].logo_url).toBe('/api/ui/workspaces/hrms/logo');
			expect(entries[0].icon).toBe('users');
		}

		const noWorkspaceLogo = buildHomeEntries(apps, [
			{ id: 'hrms', label: 'HRMS', launcher: 'folder' }
		]);
		expect(noWorkspaceLogo[0]?.kind === 'folder' && noWorkspaceLogo[0].logo_url).toBeNull();
		expect(noWorkspaceLogo[0]?.kind === 'folder' && noWorkspaceLogo[0].icon).toBe('users');
	});

	it('lets plugins join a named folder inside the same category', () => {
		const apps = [
			app({
				plugin: 'hr_leave',
				label: 'Leave',
				route: '/hr/leave',
				category: 'hrms',
				folder: 'Time'
			}),
			app({
				plugin: 'hr_attendance',
				label: 'Attendance',
				route: '/hr/attendance',
				category: 'hrms',
				folder: 'Time'
			}),
			app({ plugin: 'hr', label: 'Human Resources', route: '/hr/employees', category: 'hrms' }),
			app({ plugin: 'hr_payroll', label: 'Payroll', route: '/hr/payroll', category: 'hrms' })
		];
		const entries = buildHomeEntries(apps, categories);
		expect(entries).toHaveLength(2);
		const time = entries.find((e) => e.kind === 'folder' && e.label === 'Time');
		const hrFolder = entries.find((e) => e.kind === 'folder' && e.label === 'HRMS');
		expect(time?.kind === 'folder' && time.apps.map((a) => a.plugin).sort()).toEqual([
			'hr_attendance',
			'hr_leave'
		]);
		expect(hrFolder?.kind === 'folder' && hrFolder.apps.map((a) => a.plugin).sort()).toEqual([
			'hr',
			'hr_payroll'
		]);
	});

	it('keeps a one-app folder as a folder', () => {
		const apps = [
			app({
				plugin: 'hr_leave',
				label: 'Leave',
				route: '/hr/leave',
				category: 'hrms',
				folder: 'Time'
			})
		];
		const entries = buildHomeEntries(apps, categories);
		expect(entries).toHaveLength(1);
		expect(entries[0]?.kind).toBe('folder');
		expect(entries[0]?.kind === 'folder' && entries[0].apps).toEqual(apps);
	});
});
