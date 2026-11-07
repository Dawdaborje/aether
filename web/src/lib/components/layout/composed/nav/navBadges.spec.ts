import { describe, expect, it } from 'vitest';
import { badgeFunction, badgeText, numberOf } from './navBadges.svelte';

describe('nav badges', () => {
	it('turns answers into what the badge shows', () => {
		expect(badgeText(3)).toBe('3');
		expect(badgeText(250)).toBe('99+');
		expect(badgeText(0)).toBeNull();
		expect(badgeText(-2)).toBeNull();
		expect(badgeText([1, 2])).toBe('2');
		expect(badgeText([])).toBeNull();
		expect(badgeText({ count: 7 })).toBe('7');
		expect(badgeText('New')).toBe('New');
		expect(badgeText('a far too long label')).toBeNull();
		expect(badgeText(null)).toBeNull();
		expect(badgeText(true)).toBeNull();
	});

	it('names the function of a counted badge only', () => {
		expect(badgeFunction({ function: 'crm.open_leads' })).toBe('crm.open_leads');
		expect(badgeFunction('New')).toBeNull();
		expect(badgeFunction(undefined)).toBeNull();
	});

	it('reads the number of an answer in full', () => {
		expect(numberOf(1250)).toBe(1250);
		expect(numberOf({ count: 4 })).toBe(4);
		expect(numberOf([1, 2, 3])).toBe(3);
		expect(numberOf('7')).toBeNull();
		expect(numberOf(null)).toBeNull();
	});
});
