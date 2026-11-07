import { describe, expect, it } from 'vitest';
import { actorLabel, auditParams, chainSummary, isRefused, summarize, type AuditRow } from './api';

const row = (over: Partial<AuditRow>): AuditRow => ({
	id: '1',
	date_created: '2026-11-02T00:00:00Z',
	request_id: 'r',
	actor_type: 'user',
	...over
});

describe('auditParams', () => {
	it('leaves out empty filters', () => {
		const p = auditParams({ table: 'data_access', actor: ' ', plugin: 'crm', failed: false });
		expect(p.get('table')).toBe('data_access');
		expect(p.get('plugin')).toBe('crm');
		expect(p.has('actor')).toBe(false);
		expect(p.has('failed')).toBe(false);
		expect(p.has('from')).toBe(false);
	});
	it('turns "within hours" into a start time', () => {
		const now = new Date('2026-11-02T12:00:00Z');
		const p = auditParams({ table: 'page_visits', withinHours: 24, failed: true }, now);
		expect(p.get('from')).toBe('2026-11-01T12:00:00.000Z');
		expect(p.get('failed')).toBe('true');
	});
});

describe('describing rows', () => {
	it('names who', () => {
		expect(actorLabel({ actor_type: 'anonymous' })).toBe('Anonymous');
		expect(actorLabel({ actor_type: 'user', actor_id: 'users:a' })).toBe('user users:a');
	});
	it('summarizes by table', () => {
		expect(
			summarize('data_access', row({ operation: 'update', model: 'contact', record_count: 3 }))
		).toBe('update contact (3)');
		expect(summarize('data_access', row({ operation: 'raw' }))).toBe('raw raw query');
		expect(summarize('plugin_calls', row({ plugin: 'crm', function_name: 'find' }))).toBe(
			'crm.find'
		);
		expect(summarize('page_visits', row({ method: 'GET', path: '/x/1' }))).toBe('GET /x/1');
	});
	it('spots refusals', () => {
		expect(isRefused({ status: 403 })).toBe(true);
		expect(isRefused({ status: 200 })).toBe(false);
		expect(isRefused({})).toBe(false);
	});
});

describe('chainSummary', () => {
	it('says when it was never checked', () => {
		expect(chainSummary(null).ok).toBeNull();
	});
	it('names the broken chains', () => {
		const s = chainSummary({
			heads: [],
			last_verification: {
				ok: false,
				at: 'not a date',
				report: {
					chains: [
						{ chain: 'data_access', ok: false } as never,
						{ chain: 'plugin_calls', ok: true } as never
					]
				}
			}
		});
		expect(s.ok).toBe(false);
		expect(s.text).toContain('data_access');
		expect(s.text).not.toContain('plugin_calls');
	});
});
