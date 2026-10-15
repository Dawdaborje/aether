import type { CatalogPlugin } from './catalogApi';

/** One plugin with every catalogued version (newest first). */
export interface CatalogPluginGroup {
	name: string;
	/** Newest revision first. */
	versions: CatalogPlugin[];
	/** Convenience: same as `versions[0]`. */
	latest: CatalogPlugin;
}

/** Sort catalog versions so stamped rebuilds (`0.2.1+20261009T…`) beat the base. */
export function compareCatalogVersions(a: string, b: string): number {
	return b.localeCompare(a, undefined, { numeric: true, sensitivity: 'base' });
}

/** Newest catalogued revision first (`date_created`, then version stamp). */
export function compareCatalogPlugins(a: CatalogPlugin, b: CatalogPlugin): number {
	const byDate = (b.date_created ?? '').localeCompare(a.date_created ?? '');
	if (byDate !== 0) return byDate;
	return compareCatalogVersions(a.version, b.version);
}

/** Collapse catalog rows into one group per plugin name. */
export function groupCatalogByName(plugins: CatalogPlugin[]): CatalogPluginGroup[] {
	const byName = new Map<string, CatalogPlugin[]>();
	for (const plugin of plugins) {
		const list = byName.get(plugin.name) ?? [];
		list.push(plugin);
		byName.set(plugin.name, list);
	}
	const groups: CatalogPluginGroup[] = [];
	for (const [name, versions] of byName) {
		versions.sort(compareCatalogPlugins);
		groups.push({ name, versions, latest: versions[0]! });
	}
	groups.sort((a, b) => {
		const label = a.latest.label.localeCompare(b.latest.label, undefined, {
			sensitivity: 'base'
		});
		return label !== 0 ? label : a.name.localeCompare(b.name);
	});
	return groups;
}
