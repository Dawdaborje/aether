import { describe, expect, it } from 'vitest';
import { blockVisible, flagProp, hideBelowClass, textProp, type Viewer } from './visibility';

const member: Viewer = {
	isUser: true,
	isDeveloper: false,
	canManageUsers: false,
	canManageOrg: false,
	roles: ['sales']
};
const visitor: Viewer = { ...member, isUser: false, roles: [] };
const developer: Viewer = { ...member, isDeveloper: true, roles: [] };

describe('blockVisible', () => {
	it('shows blocks without conditions to everyone', () => {
		expect(blockVisible({ type: 'spacer' }, visitor)).toBe(true);
	});

	it('applies `when`', () => {
		expect(blockVisible({ type: 'notifications', when: 'user' }, visitor)).toBe(false);
		expect(blockVisible({ type: 'notifications', when: 'user' }, member)).toBe(true);
		expect(blockVisible({ type: 'developer-badge', when: 'developer' }, member)).toBe(false);
		expect(blockVisible({ type: 'developer-badge', when: 'developer' }, developer)).toBe(true);
		expect(blockVisible({ type: 'link', when: 'admin' }, member)).toBe(false);
		expect(blockVisible({ type: 'link', when: 'admin' }, { ...member, canManageOrg: true })).toBe(
			true
		);
		expect(blockVisible({ type: 'link', when: 'manager' }, developer)).toBe(true);
	});

	it('needs one of `roles`, except for developers', () => {
		expect(blockVisible({ type: 'link', roles: ['sales', 'hr'] }, member)).toBe(true);
		expect(blockVisible({ type: 'link', roles: ['hr'] }, member)).toBe(false);
		expect(blockVisible({ type: 'link', roles: ['hr'] }, developer)).toBe(true);
	});
});

describe('props', () => {
	it('reads typed props with fallbacks', () => {
		const block = { type: 'brand', props: { label: 'Sea', show_logo: false } };
		expect(textProp(block, 'label')).toBe('Sea');
		expect(textProp(block, 'href', '/apps')).toBe('/apps');
		expect(textProp(block, 'show_logo', 'x')).toBe('x');
		expect(flagProp(block, 'show_logo', true)).toBe(false);
		expect(flagProp(block, 'label', true)).toBe(true);
	});

	it('maps breakpoints to Tailwind classes', () => {
		expect(hideBelowClass({ type: 'spacer', hide_below: 'md' })).toBe('max-md:hidden');
		expect(hideBelowClass({ type: 'spacer' })).toBe('');
	});
});
