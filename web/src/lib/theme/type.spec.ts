import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { cn } from '$lib/utils';
import { typeRules } from './applyTheme';
import { TYPE_ROLES } from './types';

const PROPERTIES = ['size', 'leading', 'weight', 'tracking', 'family', 'case'];

describe('theme type', () => {
	it('writes nothing without a type', () => {
		expect(typeRules(undefined)).toBe('');
		expect(typeRules({})).toBe('');
	});

	it('writes scale steps and role properties', () => {
		const css = typeRules({
			scale: { sm: '15px', '4xl': '3rem' },
			roles: {
				display: { size: '4xl', line_height: 1.1, weight: 700, tracking: '-0.02em' },
				label: { size: '0.8rem', family: 'mono', case: 'upper' },
				title: { family: 'heading' }
			}
		});
		expect(css).toContain('--text-sm: 15px;');
		expect(css).toContain('--text-4xl: 3rem;');
		expect(css).toContain('--type-display-size: var(--text-4xl);');
		expect(css).toContain('--type-display-leading: 1.1;');
		expect(css).toContain('--type-display-weight: 700;');
		expect(css).toContain('--type-display-tracking: -0.02em;');
		expect(css).toContain('--type-label-size: 0.8rem;');
		expect(css).toContain('--type-label-family: var(--font-mono);');
		expect(css).toContain('--type-label-case: uppercase;');
		expect(css).toContain('--type-title-family: var(--font-heading);');
		expect(css).not.toContain('--type-title-size');
	});

	it('layout.css gives every role a default for every property and a utility', () => {
		const css = readFileSync(
			fileURLToPath(new URL('../../routes/layout.css', import.meta.url)),
			'utf8'
		);
		for (const role of TYPE_ROLES) {
			for (const property of PROPERTIES) {
				expect(css, `${role}-${property}`).toMatch(
					new RegExp(`--type-${role}-${property}: [^;]+;`)
				);
			}
			expect(css, role).toContain(`@utility type-${role} {`);
		}
	});

	it('a later role replaces an earlier one in cn()', () => {
		expect(cn('type-subheading', 'type-metric tabular-nums')).toBe('type-metric tabular-nums');
		expect(cn('rounded-card', 'rounded-none')).toBe('rounded-none');
		expect(cn('type-label', 'text-sm')).toBe('type-label text-sm');
	});
});
