import { apiFetch } from '$lib/api/client';
import type { PageLoad } from './$types';

// Org slugs are dynamic and resolved at runtime, so this route cannot be
// prerendered. It is served via the SPA fallback (see svelte.config.js).
export const prerender = false;

/** What the page API returns for a page (see `GET /api/ui/pages/{route}`). */
export interface PluginPage {
	route: string;
	title: string;
	public: boolean;
	/** The layout the page asks for (`<page layout="bare">`), or null for the theme's. */
	layout: string | null;
	/** Values captured by `{param}` segments of the route. */
	params: Record<string, string>;
	page: unknown;
}

/**
 * Fetch the page before anything renders, so the application shell can pick
 * the page's layout from route data instead of flipping after mount.
 */
export const load: PageLoad = async ({ params, fetch }) => {
	const res = await apiFetch(`/api/ui/pages/${params.slug}`, {}, fetch);
	if (!res.ok) {
		return { status: res.status, body: null as PluginPage | null, layout: null as string | null };
	}
	const body = (await res.json()) as PluginPage;
	return {
		status: res.status,
		body,
		layout: typeof body.layout === 'string' ? body.layout : null
	};
};
