import { describe, expect, it } from 'vitest';
import { isValidName, modelListLabel } from './api';

describe('isValidName', () => {
	it('accepts what the kernel accepts', () => {
		for (const good of ['title', 'created_at', 'a1', 'x'.repeat(48)]) expect(isValidName(good)).toBe(true);
	});
	it('refuses everything else', () => {
		for (const bad of ['', 'Title', '1abc', 'a b', 'a-b', 'a.b', 'x'.repeat(49), '_x']) {
			expect(isValidName(bad)).toBe(false);
		}
	});
});

describe('modelListLabel', () => {
	it('prefers display_label', () => {
		expect(
			modelListLabel({ name: 'expense_report', label: '%expense_report%', display_label: 'Expense report' })
		).toBe('Expense report');
	});
	it('humanizes unresolved %keys%', () => {
		expect(modelListLabel({ name: 'expense_report', label: '%expense_report%' })).toBe('Expense report');
	});
	it('falls back to the technical name', () => {
		expect(modelListLabel({ name: 'counter' })).toBe('counter');
	});
});
