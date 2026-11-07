import { describe, expect, it } from 'vitest';
import { fileTag } from './fileTag';

describe('fileTag', () => {
	it('names the kind of file', () => {
		expect(fileTag({ name: 'notes.md' }).label).toBe('MD');
		expect(fileTag({ name: 'a.pdf', mime: 'application/pdf' }).label).toBe('PDF');
	});

	it('falls back to the extension, then to FILE', () => {
		expect(fileTag({ name: 'model.step' }).label).toBe('STEP');
		expect(fileTag({ name: 'README' }).label).toBe('FILE');
	});
});
