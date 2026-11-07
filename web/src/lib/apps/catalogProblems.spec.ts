import { describe, expect, it } from 'vitest';
import { describeFailureEvent, hasProblems } from './catalogApi';

describe('describeFailureEvent', () => {
	it('names what was tried, the version and the organization', () => {
		expect(
			describeFailureEvent({ at: null, kind: 'upgrade', to_version: '1.2.0', organization: 'acme' })
		).toBe('Upgrade to 1.2.0 in acme');
		expect(describeFailureEvent({ at: null, kind: 'catalog_load' })).toBe('Load from disk');
		expect(describeFailureEvent({ at: null, kind: 'something_new' })).toBe('something_new');
	});
});

describe('hasProblems', () => {
	it('is true for a load failure or any failed attempt', () => {
		expect(hasProblems(null)).toBe(false);
		expect(hasProblems({ load_failure: null, recent_failures: [] })).toBe(false);
		expect(
			hasProblems({ load_failure: { message: 'x', at: '', count: 1 }, recent_failures: [] })
		).toBe(true);
		expect(
			hasProblems({ load_failure: null, recent_failures: [{ at: null, kind: 'install' }] })
		).toBe(true);
	});
});

import { groupLoadProblem } from './catalogGroup';
import type { CatalogPlugin } from './catalogApi';

describe('groupLoadProblem', () => {
	const version = (v: string, message?: string) =>
		({
			name: 'crm',
			version: v,
			load_failure: message ? { message, at: '', count: 1 } : null
		}) as CatalogPlugin;
	const group = (versions: CatalogPlugin[]) => ({ name: 'crm', versions, latest: versions[0]! });

	it('is null when no version failed', () => {
		expect(groupLoadProblem(group([version('2'), version('1')]))).toBeNull();
	});
	it('prefers the newest failing version', () => {
		expect(groupLoadProblem(group([version('2', 'new'), version('1', 'old')]))).toBe('new');
		expect(groupLoadProblem(group([version('2'), version('1', 'old')]))).toBe('old');
	});
});
