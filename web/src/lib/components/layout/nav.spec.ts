import { describe, expect, it } from 'vitest';
import type { AppTileData } from '$lib/components/apps/types';
import { activeInstalledApp, pathUnderApp } from './nav';

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
});

describe('pathUnderApp', () => {
	it('requires an absolute app route', () => {
		expect(pathUnderApp('/web/base/countries', 'base/countries')).toBe(false);
	});
});
