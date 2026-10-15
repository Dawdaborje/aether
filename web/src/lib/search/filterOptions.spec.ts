import { describe, expect, it } from 'vitest';
import { parseFilterOptions } from './filterOptions';

describe('parseFilterOptions', () => {
	it('keeps plain tokens as value and label', () => {
		expect(parseFilterOptions('person,organization')).toEqual([
			{ value: 'person', label: 'person' },
			{ value: 'organization', label: 'organization' }
		]);
	});

	it('parses Label=value and coerces booleans', () => {
		expect(parseFilterOptions('Active=true,Inactive=false')).toEqual([
			{ value: true, label: 'Active' },
			{ value: false, label: 'Inactive' }
		]);
	});
});
