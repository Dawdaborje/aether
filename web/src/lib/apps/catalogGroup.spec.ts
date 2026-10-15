import { describe, expect, it } from 'vitest';
import type { CatalogPlugin } from './catalogApi';
import { compareCatalogVersions, groupCatalogByName } from './catalogGroup';

function plugin(
	name: string,
	version: string,
	label = name,
	date_created: string | null = null
): CatalogPlugin {
	return {
		name,
		version,
		label,
		description: null,
		long_description: null,
		category: 'hrms',
		kind: 'addon',
		is_app: true,
		is_builtin: false,
		icon: 'clock',
		logo_url: null,
		date_created,
		files_added: [],
		files_changed: [],
		files_removed: [],
		website: null,
		authors: [],
		dependencies: [],
		app_route: null,
		app_description: null,
		installed_in: [],
		seed_commands: []
	};
}

describe('groupCatalogByName', () => {
	it('collapses revisions into one card and keeps newest first', () => {
		const groups = groupCatalogByName([
			plugin('hr_attendance', '0.2.1', 'Attendance', '2026-10-09T19:44:40Z'),
			plugin(
				'hr_attendance',
				'0.2.1+20261009T213000Z',
				'Attendance',
				'2026-10-09T21:30:00Z'
			),
			plugin(
				'hr_attendance',
				'0.2.1+20261009T201312Z',
				'Attendance',
				'2026-10-09T20:13:12Z'
			),
			plugin('hr', '0.1.0', 'Human Resources', '2026-10-09T19:00:00Z')
		]);

		expect(groups.map((g) => g.name)).toEqual(['hr_attendance', 'hr']);
		expect(groups[0]!.versions.map((v) => v.version)).toEqual([
			'0.2.1+20261009T213000Z',
			'0.2.1+20261009T201312Z',
			'0.2.1'
		]);
		expect(groups[0]!.latest.version).toBe('0.2.1+20261009T213000Z');
	});
});

describe('compareCatalogVersions', () => {
	it('orders stamped rebuilds ahead of the base version', () => {
		expect(compareCatalogVersions('0.2.1', '0.2.1+20261009T213000Z')).toBeGreaterThan(0);
		expect(compareCatalogVersions('0.2.1+20261009T213000Z', '0.2.1+20261009T201312Z')).toBeLessThan(
			0
		);
	});
});
