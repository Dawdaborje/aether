import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { shapeRules } from './applyTheme';
import { SHAPE_ROLES, SHAPE_STEPS } from './types';

describe('theme shape', () => {
	it('writes nothing without a shape', () => {
		expect(shapeRules(undefined)).toBe('');
		expect(shapeRules({})).toBe('');
	});

	it('writes steps and resolves roles', () => {
		const css = shapeRules({
			scale: { sm: '8px', lg: '16px' },
			roles: {
				button: 'full',
				field: 'xs',
				card: 'lg',
				dialog: 'none',
				chip: '6px',
				menu: 'xs xs none none'
			}
		});
		expect(css).toContain('--shape-sm: 8px;');
		expect(css).toContain('--shape-lg: 16px;');
		expect(css).toContain('--shape-button: calc(infinity * 1px);');
		expect(css).toContain('--shape-field: var(--shape-xs);');
		expect(css).toContain('--shape-card: var(--shape-lg);');
		expect(css).toContain('--shape-dialog: 0;');
		expect(css).toContain('--shape-chip: 6px;');
		expect(css).toContain('--shape-menu: var(--shape-xs) var(--shape-xs) 0 0;');
	});

	it('layout.css gives every step and role a default and a utility', () => {
		const css = readFileSync(
			fileURLToPath(new URL('../../routes/layout.css', import.meta.url)),
			'utf8'
		);
		for (const name of [...SHAPE_STEPS, ...SHAPE_ROLES]) {
			expect(css, name).toMatch(new RegExp(`--shape-${name}: [^;]+;`));
			expect(css, name).toContain(`--radius-${name}: var(--shape-${name});`);
		}
	});
});
