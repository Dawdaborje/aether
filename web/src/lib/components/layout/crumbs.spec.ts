import { describe, expect, it } from 'vitest';
import { orgCrumbs, settingsCrumbs } from './crumbs';

const org = (pathname: string, single = false) =>
	orgCrumbs({ single, name: 'Acme', db: 'acme', pathname });

describe('orgCrumbs', () => {
	it('stops at the organization on its overview', () => {
		expect(org('/desk/organizations/acme/overview')).toEqual([
			{ label: 'Organizations', href: '/desk/organizations' },
			{ label: 'Acme' }
		]);
	});
	it('adds the section', () => {
		expect(org('/desk/organizations/acme/members').map((c) => c.label)).toEqual([
			'Organizations',
			'Acme',
			'Members'
		]);
		expect(org('/desk/organizations/acme/members')[1].href).toBe(
			'/desk/organizations/acme/overview'
		);
	});
	it('adds the role and the children view', () => {
		const role = org('/desk/organizations/acme/roles/sales%20lead');
		expect(role.map((c) => c.label)).toEqual(['Organizations', 'Acme', 'Roles', 'sales lead']);
		expect(role[2].href).toBe('/desk/organizations/acme/roles');
		expect(org('/desk/organizations/acme/children/graph').at(-1)?.label).toBe('Graph');
	});
	it('has no organizations list on a single-organization install', () => {
		expect(org('/desk/organizations/acme/plugins', true).map((c) => c.label)).toEqual([
			'Acme',
			'Plugins'
		]);
	});
	it('names the audit log', () => {
		expect(org('/desk/organizations/acme/audit').at(-1)?.label).toBe('Audit log');
	});
	it('works under a base path', () => {
		expect(org('/web/desk/organizations/acme/schedules').at(-1)?.label).toBe('Schedules');
	});
});

describe('settingsCrumbs', () => {
	const groups = [{ slug: 'mail', label: 'Email' }];
	it('names the group', () => {
		expect(settingsCrumbs('/desk/settings/mail', groups).map((c) => c.label)).toEqual([
			'Settings',
			'Email'
		]);
		expect(settingsCrumbs('/desk/settings/appearance', groups).at(-1)?.label).toBe('Appearance');
	});
	it('is just Settings on the index', () => {
		expect(settingsCrumbs('/desk/settings', groups)).toEqual([{ label: 'Settings' }]);
	});
});
