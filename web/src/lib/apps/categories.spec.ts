import { describe, expect, it } from 'vitest';
import { OTHER_CATEGORY, categoryLabel } from './categories';

describe('categoryLabel', () => {
	it('returns Other for empty workspace', () => {
		expect(categoryLabel(null)).toBe(OTHER_CATEGORY);
		expect(categoryLabel('')).toBe(OTHER_CATEGORY);
	});

	it('title-cases ordinary words', () => {
		expect(categoryLabel('base')).toBe('Base');
		expect(categoryLabel('number_sequence')).toBe('Number Sequence');
	});

	it('uppercases known acronyms', () => {
		expect(categoryLabel('crm')).toBe('CRM');
		expect(categoryLabel('erp')).toBe('ERP');
		expect(categoryLabel('erp_sales')).toBe('ERP Sales');
		expect(categoryLabel('hr')).toBe('HR');
	});
});
