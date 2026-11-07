import { describe, expect, it } from 'vitest';
import { formatUptime, hitRatio, statusSummary } from './api';

describe('formatUptime', () => {
	it('uses the two biggest units', () => {
		expect(formatUptime(45)).toBe('45s');
		expect(formatUptime(125)).toBe('2m');
		expect(formatUptime(2 * 3600 + 5 * 60)).toBe('2h 5m');
		expect(formatUptime(3 * 86400 + 4 * 3600)).toBe('3d 4h');
		expect(formatUptime(-1)).toBe('–');
	});
});

describe('hitRatio', () => {
	it('is a whole percent, or null with no lookups', () => {
		expect(hitRatio(3, 1)).toBe(75);
		expect(hitRatio(0, 0)).toBeNull();
	});
});

describe('statusSummary', () => {
	const check = (level: 'ok' | 'warn' | 'error') => ({ id: 'x', label: 'x', level, detail: '' });
	it('says all is well, or counts what is not', () => {
		expect(statusSummary({ status: 'ok', checks: [check('ok')] })).toBe(
			'Everything is running normally.'
		);
		expect(
			statusSummary({ status: 'error', checks: [check('error'), check('warn'), check('warn')] })
		).toBe('1 problem and 2 warnings');
	});
});
