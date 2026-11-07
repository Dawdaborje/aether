import { describe, expect, it } from 'vitest';
import { choiceParam, numberParam, textParam, withParams } from './filterParams';

const q = (s: string) => new URLSearchParams(s);

describe('reading', () => {
	it('reads text and falls back when missing', () => {
		expect(textParam(q('plugin=crm'), 'plugin')).toBe('crm');
		expect(textParam(q(''), 'plugin')).toBe('');
	});
	it('ignores values that are not allowed', () => {
		expect(choiceParam(q('source=runtime'), 'source', ['all', 'manifest', 'runtime'], 'all')).toBe(
			'runtime'
		);
		expect(choiceParam(q('source=bogus'), 'source', ['all', 'manifest', 'runtime'], 'all')).toBe(
			'all'
		);
		expect(numberParam(q('within=24'), 'within', [0, 1, 24], 0)).toBe(24);
		expect(numberParam(q('within=5'), 'within', [0, 1, 24], 0)).toBe(0);
		expect(numberParam(q('within=abc'), 'within', [0, 1, 24], 0)).toBe(0);
	});
});

describe('withParams', () => {
	const url = new URL('https://x.test/schedules?org=main#top');
	it('writes set filters and keeps other parameters and the hash', () => {
		expect(withParams(url, { plugin: 'crm', within: 24 })).toBe(
			'/schedules?org=main&plugin=crm&within=24#top'
		);
	});
	it('drops defaults and empty values', () => {
		const set = new URL('https://x.test/schedules?plugin=crm&source=runtime');
		expect(withParams(set, { plugin: '', source: 'all' }, { source: 'all' })).toBe('/schedules');
	});
});
