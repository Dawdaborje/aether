import { describe, expect, it } from 'vitest';
import { numberStep, resolveField, resolveOptions } from './resolveFieldType';

describe('resolveField', () => {
	it('uses the model type when the page names none', () => {
		const cases: Array<[string, string]> = [
			['string', 'char'],
			['text', 'text'],
			['int', 'integer'],
			['float', 'float'],
			['decimal', 'decimal'],
			['bool', 'boolean'],
			['date', 'date'],
			['datetime', 'datetime'],
			['select', 'selection'],
			['link', 'link'],
			['many2many', 'many2many'],
			['json', 'json'],
			['image', 'image'],
			['file', 'file']
		];
		for (const [modelType, widget] of cases) {
			const field = resolveField({ name: 'x', modelType });
			expect(field.fieldType, modelType).toBe(widget);
			expect(field.inferred, modelType).toBe(true);
		}
	});

	it('lets the page override the model', () => {
		const field = resolveField({ name: 'x', modelType: 'decimal', fieldType: 'currency' });
		expect(field.fieldType).toBe('currency');
		expect(field.inferred).toBe(false);
	});

	it('understands old spellings of a widget name', () => {
		expect(resolveField({ name: 'factor', fieldType: 'number' }).fieldType).toBe('float');
		expect(resolveField({ name: 'on', fieldType: 'bool' }).fieldType).toBe('boolean');
		expect(resolveField({ name: 'kind', fieldType: 'select' }).fieldType).toBe('selection');
	});

	it('treats a `source` on an untyped or text field as a picker', () => {
		expect(resolveField({ name: 'dept', source: 'list_departments' }).fieldType).toBe('link');
		expect(
			resolveField({ name: 'dept', modelType: 'string', source: 'list_departments' }).fieldType
		).toBe('link');
		// A source on a date is not a picker.
		expect(resolveField({ name: 'd', modelType: 'date', source: 'x' }).fieldType).toBe('date');
	});

	it('falls back to text for unknown or missing types', () => {
		expect(resolveField({ name: 'x' }).fieldType).toBe('char');
		expect(resolveField({ name: 'x', modelType: 'from_the_future' }).fieldType).toBe('char');
	});

	it('takes label, help and required from the page first, then the model', () => {
		const modeled = resolveField({
			name: 'total',
			modelLabel: 'Total due',
			modelHelp: 'Incl. tax',
			modelRequired: true
		});
		expect(modeled).toMatchObject({ label: 'Total due', help: 'Incl. tax', required: true });

		const written = resolveField({ name: 'total', label: 'Amount', modelLabel: 'Total due' });
		expect(written.label).toBe('Amount');
		expect(resolveField({ field: 'status' }).label).toBe('status');
	});

	it('gives a decimal its scale, two by default', () => {
		expect(resolveField({ name: 'x', modelType: 'decimal', modelScale: 4 }).scale).toBe(4);
		expect(resolveField({ name: 'x', modelType: 'decimal' }).scale).toBe(2);
		expect(resolveField({ name: 'x', modelType: 'int' }).scale).toBeNull();
	});
});

describe('resolveOptions', () => {
	it('prefers the options the page lists', () => {
		const options = resolveOptions({
			options: 'male, female',
			modelOptions: [{ value: 'other' }]
		});
		expect(options.map((o) => o.value)).toEqual(['male', 'female']);
	});

	it('uses the model options with their labels and colours', () => {
		const options = resolveOptions({
			modelOptions: [
				{ value: 'on_leave', color: 'amber' },
				{ value: 'active', label: 'Working' },
				{ nonsense: true }
			]
		});
		expect(options).toEqual([
			{ value: 'on_leave', label: 'On leave', color: 'amber' },
			{ value: 'active', label: 'Working', color: undefined }
		]);
	});

	it('is empty when nothing lists options', () => {
		expect(resolveOptions({})).toEqual([]);
	});
});

describe('numberStep', () => {
	it('steps by the last digit of a decimal', () => {
		expect(numberStep({ fieldType: 'decimal', scale: 2 })).toBe('0.01');
		expect(numberStep({ fieldType: 'decimal', scale: 4 })).toBe('0.0001');
		expect(numberStep({ fieldType: 'decimal', scale: 0 })).toBe('1');
		expect(numberStep({ fieldType: 'integer', scale: null })).toBe('1');
		expect(numberStep({ fieldType: 'float', scale: null })).toBe('any');
		expect(numberStep({ fieldType: 'char', scale: null })).toBeUndefined();
	});
});
