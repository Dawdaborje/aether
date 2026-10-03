import { describe, expect, it } from 'vitest';
import { timeAgo } from './time';

const now = Date.parse('2026-10-03T12:00:00Z');

describe('timeAgo', () => {
	it('says "just now" under a minute', () => {
		expect(timeAgo('2026-10-03T11:59:40Z', now)).toBe('just now');
	});
	it('counts minutes, hours and days', () => {
		expect(timeAgo('2026-10-03T11:55:00Z', now)).toMatch(/5 minutes ago/);
		expect(timeAgo('2026-10-03T09:00:00Z', now)).toMatch(/3 hours ago/);
		expect(timeAgo('2026-10-01T12:00:00Z', now)).toMatch(/2 days ago/);
	});
	it('returns nothing for a date it cannot read', () => {
		expect(timeAgo('not a date', now)).toBe('');
	});
});
