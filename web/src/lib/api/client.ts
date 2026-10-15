import { getHeaderOrg } from '$lib/org/selection';

const CSRF_COOKIE = 'aether_csrf';
const CSRF_HEADER = 'X-CSRF-Token';
const UNSAFE = new Set(['POST', 'PUT', 'PATCH', 'DELETE']);

function cookieValue(name: string): string | undefined {
	if (typeof document === 'undefined') return undefined;
	const prefix = `${name}=`;
	for (const part of document.cookie.split(';')) {
		const trimmed = part.trim();
		if (trimmed.startsWith(prefix)) {
			return decodeURIComponent(trimmed.slice(prefix.length));
		}
	}
	return undefined;
}

function isUnsafe(method: string | undefined): boolean {
	return UNSAFE.has((method ?? 'GET').toUpperCase());
}

async function ensureCsrfCookie(fetchFn: typeof fetch): Promise<string | undefined> {
	const existing = cookieValue(CSRF_COOKIE);
	if (existing) return existing;
	await fetchFn('/api/auth/methods', { credentials: 'include' });
	return cookieValue(CSRF_COOKIE);
}

/**
 * `fetch` for Aether's API: always sends the session cookie, and names the
 * organization this browser chose in `X-Org-Slug`. The server only reads that
 * header when it is configured for header tenancy, so sending it is harmless
 * elsewhere.
 *
 * Unsafe methods also send `X-CSRF-Token` matching the readable `aether_csrf`
 * cookie (double-submit). A missing cookie is bootstrapped once via
 * `GET /api/auth/methods`.
 *
 * Pass SvelteKit's own `fetch` from a `load` function as `fetchFn`.
 */
export async function apiFetch(
	input: string,
	init: RequestInit = {},
	fetchFn: typeof fetch = fetch
): Promise<Response> {
	const headers = new Headers(init.headers);
	const org = getHeaderOrg();
	if (org && !headers.has('x-org-slug')) headers.set('x-org-slug', org);

	if (isUnsafe(init.method) && !headers.has(CSRF_HEADER)) {
		const token = await ensureCsrfCookie(fetchFn);
		if (token) headers.set(CSRF_HEADER, token);
	}

	// Auth and org answers must never be served from the HTTP cache: a stale
	// `/api/auth/me` would show the wrong user after another tab signed in.
	return fetchFn(input, { ...init, credentials: 'include', cache: 'no-store', headers });
}

/** Exposed for unit tests. */
export const __csrfTest = { cookieValue, isUnsafe, CSRF_COOKIE, CSRF_HEADER };
