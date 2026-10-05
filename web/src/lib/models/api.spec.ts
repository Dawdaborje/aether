import { describe, expect, it } from 'vitest';
import { isValidName } from './api';

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
