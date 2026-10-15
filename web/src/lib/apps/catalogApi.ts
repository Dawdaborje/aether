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
	/** When this revision was catalogued (ISO-8601). */
	date_created: string | null;
	/** Files new vs the previous revision (every file on the first load). */
	files_added: string[];
	/** Files whose content changed vs the previous revision. */
	files_changed: string[];
	/** Files removed vs the previous revision. */
	files_removed: string[];
	website: string | null;
	authors: CatalogAuthor[];
	dependencies: string[];
	app_route: string | null;
	app_description: string | null;
	/** Database names of the organizations that have this version. */
	installed_in: string[];
	/** Commands marked `on_install` / `on_upgrade` (master data). */
	seed_commands: string[];
}

export interface InstallOutcome {
	organization: string;
	installed: string[];
	/** Already there at another version, moved to the chosen one (`name@version`). */
	upgraded: string[];
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
		date_created: plugin.date_created ?? null,
		files_added: plugin.files_added ?? [],
		files_changed: plugin.files_changed ?? [],
		files_removed: plugin.files_removed ?? [],
		website: plugin.website ?? null,
		authors: plugin.authors ?? [],
		dependencies: plugin.dependencies ?? [],
		app_route: plugin.app_route ?? null,
		app_description: plugin.app_description ?? null,
		installed_in: plugin.installed_in ?? [],
		seed_commands: plugin.seed_commands ?? []
	}));
}

export interface SeedOutcome {
	organization: string;
	seeded: string[];
	failed: string[];
	error: string | null;
}

/** Re-run master-data seed commands for installed plugins in each organization. */
export async function seedApps(
	plugins: string[],
	organizations: string[]
): Promise<SeedOutcome[]> {
	const res = await apiFetch('/api/ui/catalog/seed', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ plugins, organizations })
	});
	if (!res.ok) throw await failure(res, 'Could not seed data');
	return ((await res.json()) as { results: SeedOutcome[] }).results;
}

export interface PluginFunctionCommand {
	name: string | null;
	help: string | null;
	args: { name: string; type?: string | null; required?: boolean | null }[] | null;
	on_install: boolean | null;
	on_upgrade: boolean | null;
}

export interface PluginFunctionSchedule {
	name: string | null;
	every: string | null;
	cron: string | null;
}

export interface PluginFunction {
	name: string;
	/** The code defines it; false when the manifest names a function the code lacks. */
	exported: boolean;
	/** Anonymous visitors may call it. */
	public: boolean;
	/** Answers a variable in an access rule (`rule_var_*`). */
	rule_variable: boolean;
	commands: PluginFunctionCommand[];
	schedules: PluginFunctionSchedule[];
	/** Events whose delivery runs it. */
	listens_to: string[];
}

export interface PluginFunctions {
	plugin: string;
	version: string;
	kind: 'wasm' | 'rhai' | 'lua' | 'unknown';
	/** False when the plugin's code could not be read (only manifest-declared functions show). */
	inspected: boolean;
	functions: PluginFunction[];
}

/** What one catalogued version of a plugin can be called with (developers only). */
export async function fetchPluginFunctions(name: string, version: string): Promise<PluginFunctions> {
	const res = await apiFetch(
		`/api/ui/catalog/${encodeURIComponent(name)}/${encodeURIComponent(version)}/functions`
	);
	if (!res.ok) throw await failure(res, 'Could not read the plugin functions');
	return (await res.json()) as PluginFunctions;
}

export interface AccessGrant {
	name: string;
	permissions: string[];
}

/** What one catalogued version asks for and offers (developers only). */
export interface PluginDetails {
	plugin: string;
	version: string;
	revision: string | null;
	/** False when the stored `plugin.toml` could not be read. */
	manifest_read: boolean;
	definition: {
		api_version: string;
		capabilities: string[];
		access_models: AccessGrant[];
		public_capabilities: string[];
		public_access_models: AccessGrant[];
		public_functions: string[];
		http_hosts: string[];
		bridges: string[];
		i18n_default: string | null;
		code: string | null;
	} | null;
	roles: { name: string; label: string | null; description: string | null }[];
	events: { name: string; direction: string | null }[];
	watches: { name: string; path: string; function: string }[];
	pages: {
		route: string;
		title: string;
		public: boolean;
		roles: string[] | null;
		extends: string | null;
	}[];
	models: { name: string; label: string | null; fields: number | null }[];
	components: string[];
}

export async function fetchPluginDetails(name: string, version: string): Promise<PluginDetails> {
	const res = await apiFetch(
		`/api/ui/catalog/${encodeURIComponent(name)}/${encodeURIComponent(version)}/details`
	);
	if (!res.ok) throw await failure(res, 'Could not read the plugin details');
	return (await res.json()) as PluginDetails;
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
	return ((await res.json()) as { results: InstallOutcome[] }).results.map((row) => ({
		...row,
		upgraded: row.upgraded ?? []
	}));
}

export interface CatalogLoadEntry {
	name: string;
	version: string;
	created: boolean;
	revision: string | null;
}

/** An organization moved to a revision that was just loaded. */
export interface AppliedUpgrade {
	organization: string;
	plugin: string;
	version: string;
	ok: boolean;
	error: string | null;
}

export interface CatalogLoadResult {
	loaded: CatalogLoadEntry[];
	/** Organizations that had a reloaded plugin installed and were moved to its new revision. */
	applied?: AppliedUpgrade[];
	staged?: string;
}

/** Re-read `plugin_paths` from aether.toml (or load `paths`) into the catalog. */
export async function reloadCatalog(paths?: string[]): Promise<CatalogLoadResult> {
	const res = await apiFetch('/api/ui/catalog/reload', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(paths?.length ? { paths } : {})
	});
	if (!res.ok) throw await failure(res, 'Could not reload the catalog');
	return (await res.json()) as CatalogLoadResult;
}

function fileToBase64(file: File): Promise<string> {
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () => {
			const result = reader.result;
			if (typeof result !== 'string') {
				reject(new Error('Could not read the file'));
				return;
			}
			const comma = result.indexOf(',');
			resolve(comma >= 0 ? result.slice(comma + 1) : result);
		};
		reader.onerror = () => reject(new Error('Could not read the file'));
		reader.readAsDataURL(file);
	});
}

/** Upload a plugin or workspace zip and load it into the catalog. */
export async function uploadCatalogZip(file: File): Promise<CatalogLoadResult> {
	const content_base64 = await fileToBase64(file);
	const res = await apiFetch('/api/ui/catalog/upload', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ filename: file.name, content_base64 })
	});
	if (!res.ok) throw await failure(res, 'Could not upload the plugin');
	return (await res.json()) as CatalogLoadResult;
}
