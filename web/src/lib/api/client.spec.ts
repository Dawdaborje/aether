import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { __csrfTest, apiFetch } from './client';

function stubDocumentCookie() {
	const jar = new Map<string, string>();
	const documentStub = {
		get cookie() {
			return [...jar.entries()].map(([k, v]) => `${k}=${v}`).join('; ');
		},
		set cookie(value: string) {
			const [pair] = value.split(';');
			const eq = pair.indexOf('=');
			if (eq < 0) return;
			const name = pair.slice(0, eq).trim();
			const raw = pair.slice(eq + 1).trim();
			if (value.toLowerCase().includes('max-age=0')) {
				jar.delete(name);
				return;
			}
			jar.set(name, raw);
		}
	};
	vi.stubGlobal('document', documentStub);
	return jar;
}

describe('apiFetch CSRF', () => {
	beforeEach(() => {
		stubDocumentCookie();
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('attaches X-CSRF-Token on POST when the cookie is present', async () => {
		document.cookie = `${__csrfTest.CSRF_COOKIE}=tok123; Path=/`;
		const fetchFn = vi.fn(async () => new Response('ok'));
		await apiFetch('/api/auth/logout', { method: 'POST' }, fetchFn);
		expect(fetchFn).toHaveBeenCalledOnce();
		const init = fetchFn.mock.calls[0][1] as RequestInit;
		const headers = new Headers(init.headers);
		expect(headers.get(__csrfTest.CSRF_HEADER)).toBe('tok123');
		expect(init.credentials).toBe('include');
	});

	it('bootstraps the cookie via GET /api/auth/methods when missing', async () => {
		const fetchFn = vi.fn(async (input: RequestInfo | URL) => {
			const url = String(input);
			if (url.includes('/api/auth/methods')) {
				document.cookie = `${__csrfTest.CSRF_COOKIE}=bootstrapped; Path=/`;
				return new Response('{}');
			}
			return new Response('ok');
		});
		await apiFetch('/api/auth/login', { method: 'POST', body: '{}' }, fetchFn);
		expect(fetchFn.mock.calls[0][0]).toBe('/api/auth/methods');
		const init = fetchFn.mock.calls[1][1] as RequestInit;
		expect(new Headers(init.headers).get(__csrfTest.CSRF_HEADER)).toBe('bootstrapped');
	});

	it('does not send a CSRF header on GET', async () => {
		document.cookie = `${__csrfTest.CSRF_COOKIE}=tok123; Path=/`;
		const fetchFn = vi.fn(async () => new Response('ok'));
		await apiFetch('/api/auth/me', {}, fetchFn);
		const headers = new Headers((fetchFn.mock.calls[0][1] as RequestInit).headers);
		expect(headers.get(__csrfTest.CSRF_HEADER)).toBeNull();
	});
});
