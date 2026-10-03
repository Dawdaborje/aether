import { apiFetch } from '$lib/api/client';

/** One plugin version in the catalog (`aether --load-plugin`). */
export interface CatalogPlugin {
	name: string;
	version: string;
	label: string;
	description: string | null;
	category: string | null;
	kind: string | null;
	is_app: boolean;
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
	return ((await res.json()) as { plugins: CatalogPlugin[] }).plugins;
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
