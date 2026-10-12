import { apiFetch } from '$lib/api/client';

export interface CatalogAuthor {
	name: string;
	email?: string | null;
	github?: string | null;
	website?: string | null;
}

/** One plugin version in the catalog (`aether --load-plugin`). */
export interface CatalogPlugin {
	name: string;
	version: string;
	label: string;
	description: string | null;
	long_description: string | null;
	category: string | null;
	kind: string | null;
	is_app: boolean;
	is_builtin: boolean;
	/** Lucide icon name from `[app].icon`. */
	icon: string | null;
	/** `/api/ui/plugins/{name}/logo` when the revision ships a logo file. */
	logo_url: string | null;
	website: string | null;
	authors: CatalogAuthor[];
	dependencies: string[];
	app_route: string | null;
	app_description: string | null;
	/** Database names of the organizations that have this version. */
	installed_in: string[];
}

export interface InstallOutcome {
	organization: string;
	installed: string[];
	already_installed: string[];
	error: string | null;
}

async function failure(res: Response, fallback: string): Promise<Error> {
	const body = await res.json().catch(() => ({}));
	return new Error(typeof body.error === 'string' ? body.error : `${fallback} (${res.status})`);
}

export async function fetchCatalog(): Promise<CatalogPlugin[]> {
	const res = await apiFetch('/api/ui/catalog');
	if (!res.ok) throw await failure(res, 'Could not load the catalog');
	const plugins = ((await res.json()) as { plugins: CatalogPlugin[] }).plugins;
	return plugins.map((plugin) => ({
		...plugin,
		long_description: plugin.long_description ?? null,
		is_builtin: plugin.is_builtin ?? false,
		website: plugin.website ?? null,
		authors: plugin.authors ?? [],
		dependencies: plugin.dependencies ?? [],
		app_route: plugin.app_route ?? null,
		app_description: plugin.app_description ?? null,
		installed_in: plugin.installed_in ?? []
	}));
}

/** Install `plugins` (`name@version`) into each of `organizations` (database names). */
export async function installApps(
	plugins: string[],
	organizations: string[]
): Promise<InstallOutcome[]> {
	const res = await apiFetch('/api/ui/catalog/install', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ plugins, organizations })
	});
	if (!res.ok) throw await failure(res, 'Could not install');
	return ((await res.json()) as { results: InstallOutcome[] }).results;
}
