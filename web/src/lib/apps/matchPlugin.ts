import { categoryLabel } from './categories';
import type { CatalogPlugin } from './catalogApi';

/** Case-insensitive match of a catalog plugin against a search query. */
export function matchPlugin(plugin: CatalogPlugin, query: string): boolean {
	const q = query.trim().toLowerCase();
	if (!q) return true;
	const haystack = [
		plugin.name,
		plugin.label,
		plugin.description ?? '',
		plugin.kind ?? '',
		categoryLabel(plugin.category),
		plugin.app_route ?? '',
		...plugin.dependencies,
		...plugin.authors.map((author) => author.name)
	]
		.join(' ')
		.toLowerCase();
	return haystack.includes(q);
}
