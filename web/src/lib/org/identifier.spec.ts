import { describe, expect, it } from 'vitest';
import { identifierFor } from './createOrganization';

describe('identifierFor', () => {
	it('makes a database name from an organization name', () => {
		expect(identifierFor('Acme Corporation')).toBe('acme_corporation');
		expect(identifierFor('  Acme  Holdings, Ltd. ')).toBe('acme_holdings_ltd');
		expect(identifierFor('Zürich 2026')).toBe('z_rich_2026');
	});
	it('is empty when nothing usable is left', () => {
		expect(identifierFor('***')).toBe('');
	});
});
