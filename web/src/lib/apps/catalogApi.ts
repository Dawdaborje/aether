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
	/** Why this version could not be loaded, when it failed and has not loaded since. */
	load_failure?: PluginLoadFailure | null;
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
	// The status goes last, so a page can tell "server down" from "no access" from "not found".
	return new Error(
		typeof body.error === 'string' && body.error
			? `${body.error} (${res.status})`
			: `${fallback} (${res.status})`
	);
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

export interface UninstallOutcome {
	/** `name@version` of each plugin removed. */
	uninstalled: string[];
	/** Data the plugins left behind; installing again picks it up. */
	kept_data: { plugin: string; tables: number; records: number }[];
	/** Data dropped by an uninstall-and-purge (backed up first). */
	purged_data: { plugin: string; tables: number; records: number }[];
}

/** Uninstall plugins from one organization; `purge` also drops the tables they created. */
export async function uninstallApps(
	plugins: string[],
	organization: string,
	purge = false
): Promise<UninstallOutcome> {
	const res = await apiFetch('/api/ui/catalog/uninstall', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ plugins, organization, purge })
	});
	if (!res.ok) throw await failure(res, 'Could not uninstall');
	return (await res.json()) as UninstallOutcome;
}

/** The tables a plugin's models created in an organization, with record counts. */
export interface PluginData {
	plugin: string;
	records: number;
	tables: { table: string; model: string; records: number }[];
}

export interface UninstallPreview {
	/** The plugins in the order they would go (dependants first). */
	plan: { name: string; version: string }[];
	/** What each owns; a purge drops all of it. */
	data: PluginData[];
}

/** What uninstalling `plugins` would do; fails like the uninstall when a dependant stays. */
export async function previewUninstall(
	plugins: string[],
	organization: string
): Promise<UninstallPreview> {
	const res = await apiFetch('/api/ui/catalog/uninstall/preview', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ plugins, organization })
	});
	if (!res.ok) throw await failure(res, 'Could not check the uninstall');
	return (await res.json()) as UninstallPreview;
}

export interface InstalledPlugin {
	name: string;
	version: string;
	is_enabled: boolean;
	/** Installed plugins this one needs. */
	dependencies: string[];
	/** Installed plugins that need this one. */
	dependents: string[];
	/** Why this version could not be loaded, when it failed and has not loaded since. */
	load_failure?: PluginLoadFailure | null;
}

export interface OrganizationPlugins {
	installed: InstalledPlugin[];
	/** Data left by plugins uninstalled earlier. */
	orphans: PluginData[];
}

export async function fetchOrganizationPlugins(db: string): Promise<OrganizationPlugins> {
	const res = await apiFetch(`/api/ui/catalog/organizations/${encodeURIComponent(db)}/plugins`);
	if (!res.ok) throw await failure(res, 'Could not load the organization’s plugins');
	return (await res.json()) as OrganizationPlugins;
}

/** Switch an installed plugin on or off in one organization; nothing is removed. */
export async function setPluginEnabled(
	plugin: string,
	organization: string,
	enabled: boolean
): Promise<void> {
	const res = await apiFetch('/api/ui/catalog/enabled', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ plugin, organization, enabled })
	});
	if (!res.ok) throw await failure(res, enabled ? 'Could not enable' : 'Could not disable');
}

/** Drop the data an uninstalled plugin left behind (backed up first). Final. */
export async function purgeOrphans(plugin: string, organization: string): Promise<PluginData> {
	const res = await apiFetch('/api/ui/catalog/purge-orphans', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ plugin, organization })
	});
	if (!res.ok) throw await failure(res, 'Could not purge');
	return ((await res.json()) as { purged: PluginData }).purged;
}

export interface SeedOutcome {
	organization: string;
	seeded: string[];
	failed: string[];
	error: string | null;
}

/** Re-run master-data seed commands for installed plugins in each organization. */
export async function seedApps(plugins: string[], organizations: string[]): Promise<SeedOutcome[]> {
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
export async function fetchPluginFunctions(
	name: string,
	version: string
): Promise<PluginFunctions> {
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

/** Why a version could not be loaded the last time it was tried (since the server started). */
export interface PluginLoadFailure {
	message: string;
	at: string;
	/** How many times in a row it has failed. */
	count: number;
}

/** An install, upgrade or load of a plugin that did not work. */
export interface PluginFailureEvent {
	at: string | null;
	kind: string;
	to_version?: string | null;
	organization?: string | null;
	actor_name?: string | null;
	source?: string | null;
	error?: string | null;
}

const FAILURE_KINDS: Record<string, string> = {
	install: 'Install',
	upgrade: 'Upgrade',
	uninstall: 'Uninstall',
	purge: 'Purge',
	enable: 'Enable',
	disable: 'Disable',
	catalog_load: 'Load from disk'
};

/** "Upgrade to 1.2.0 in acme", for the problems list. */
export function describeFailureEvent(event: PluginFailureEvent): string {
	const what = FAILURE_KINDS[event.kind] ?? event.kind;
	const version = event.to_version ? ` to ${event.to_version}` : '';
	const where = event.organization ? ` in ${event.organization}` : '';
	return `${what}${version}${where}`;
}

/** Whether a version has anything to show in the problems section. */
export function hasProblems(
	details: Pick<PluginDetails, 'load_failure' | 'recent_failures'> | null
): boolean {
	return Boolean(details && (details.load_failure || details.recent_failures?.length));
}

/** What one catalogued version asks for and offers (developers only). */
export interface PluginDetails {
	plugin: string;
	version: string;
	revision: string | null;
	/** Why loading this version failed, when it has failed and not worked since. */
	load_failure?: PluginLoadFailure | null;
	/** The newest installs, upgrades and loads of this plugin that failed, in any organization. */
	recent_failures?: PluginFailureEvent[];
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

/**
 * Re-read `plugin_paths` from aether.toml (or load `paths`) into the catalog. With `apply`, every
 * organization that has a changed plugin installed is also moved to the new revision.
 */
export async function reloadCatalog(
	paths?: string[],
	options: { apply?: boolean } = {}
): Promise<CatalogLoadResult> {
	const res = await apiFetch('/api/ui/catalog/reload', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ ...(paths?.length ? { paths } : {}), apply: options.apply === true })
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
