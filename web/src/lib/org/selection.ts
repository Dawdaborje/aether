import { browserStorage } from '$lib/browserStorage';

/**
 * The organization this browser chose, for deployments where the client sends
 * `X-Org-Slug` (header tenancy). Kept in localStorage so it survives reloads.
 * Plain module state (not reactive): the API client reads it on every request.
 */
const KEY = 'aether.org';

let current: string | null = null;
let restored = false;

function restore(): void {
	if (restored) return;
	restored = true;
	try {
		current = browserStorage()?.getItem(KEY) ?? null;
	} catch {
		// Storage can be blocked (private mode); the choice then lasts for this page only.
		current = null;
	}
}

export function getHeaderOrg(): string | null {
	restore();
	return current;
}

export function setHeaderOrg(org: string | null): void {
	restore();
	current = org;
	try {
		const storage = browserStorage();
		if (org) storage?.setItem(KEY, org);
		else storage?.removeItem(KEY);
	} catch {
		// See restore(): not being able to persist is not an error.
	}
}
