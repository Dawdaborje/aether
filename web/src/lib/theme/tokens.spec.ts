import { describe, expect, it } from 'vitest';
import { appRules, elevationRules, focusRules, motionReduced, motionRules } from './applyTheme';

describe('elevation, motion, focus and app rules', () => {
	it('writes only what the theme sets', () => {
		expect(elevationRules(undefined)).toBe('');
		expect(elevationRules({ sm: '0 1px 2px #0003', xl: 'none' })).toBe(
			'--elevation-sm: 0 1px 2px #0003; --elevation-xl: none;'
		);
		expect(motionRules({ 'duration-short': '150ms' })).toBe('--motion-duration-short: 150ms;');
		expect(motionReduced()).toContain('prefers-reduced-motion');
	});
	it('draws a ring or a fill for focus', () => {
		expect(focusRules(undefined)).toBe('');
		expect(focusRules({ width: '3px' })).toContain('outline: 3px solid var(--ring)');
		expect(focusRules({ style: 'fill' })).toContain('--focus-fill');
	});
	it('scopes app colours to data-app', () => {
		expect(appRules({ crm: { primary: '#0176d3' } })).toBe(
			':root[data-app="crm"] { --primary: #0176d3; --ring: #0176d3; }'
		);
	});
});
