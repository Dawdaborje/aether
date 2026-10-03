import { getHeaderOrg } from '$lib/org/selection';

/**
 * `fetch` for Aether's API: always sends the session cookie, and names the
 * organization this browser chose in `X-Org-Slug`. The server only reads that
 * header when it is configured for header tenancy, so sending it is harmless
 * elsewhere.
 *
 * Pass SvelteKit's own `fetch` from a `load` function as `fetchFn`.
 */
export function apiFetch(
	input: string,
	init: RequestInit = {},
	fetchFn: typeof fetch = fetch
): Promise<Response> {
	const headers = new Headers(init.headers);
	const org = getHeaderOrg();
	if (org && !headers.has('x-org-slug')) headers.set('x-org-slug', org);
	return fetchFn(input, { credentials: 'include', ...init, headers });
}
