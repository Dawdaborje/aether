import { categoryLabel } from '$lib/apps/categories';
import type { AppCategory, AppTileData } from '$lib/components/apps/types';

/** One entry on the Apps home grid: a single app or a folder of apps. */
export type HomeEntry =
	| { kind: 'app'; app: AppTileData }
	| {
			kind: 'folder';
			id: string;
			label: string;
			icon?: string | null;
			logo_url?: string | null;
			apps: AppTileData[];
	  };

type FolderBucket = {
	label: string;
	icon?: string | null;
	logo_url?: string | null;
	apps: AppTileData[];
	sort: string;
};

function absorbVisuals(bucket: FolderBucket, app: AppTileData) {
	if (!bucket.logo_url && app.logo_url) bucket.logo_url = app.logo_url;
	if (!bucket.icon && app.icon) bucket.icon = app.icon;
}

/**
 * Build the Apps home grid.
 *
 * - `[app] folder="…"` → plugins that share a folder (same workspace) form a folder tile.
 * - Workspace `launcher = "flat"` → remaining members are individual tiles (Base).
 * - Workspace `launcher = "folder"` (default) → remaining members share the workspace folder.
 * - Folders with only one app collapse to that app's tile.
 */
export function buildHomeEntries(
	apps: AppTileData[],
	categories: AppCategory[] | null | undefined
): HomeEntry[] {
	const meta = new Map((categories ?? []).map((category) => [category.id, category]));
	const buckets = new Map<string, FolderBucket>();
	const singles: AppTileData[] = [];

	const launcherOf = (categoryId: string): 'flat' | 'folder' => {
		const raw = meta.get(categoryId)?.launcher?.trim().toLowerCase();
		return raw === 'flat' ? 'flat' : 'folder';
	};

	for (const app of apps) {
		const categoryId = app.category?.trim() || '';
		const pluginFolder = app.folder?.trim();

		if (pluginFolder) {
			const id = `${categoryId || 'other'}::folder::${pluginFolder}`;
			const existing = buckets.get(id);
			if (existing) {
				existing.apps.push(app);
				absorbVisuals(existing, app);
			} else {
				buckets.set(id, {
					label: pluginFolder,
					icon: app.icon,
					logo_url: app.logo_url,
					apps: [app],
					sort: pluginFolder.toLowerCase()
				});
			}
			continue;
		}

		if (categoryId && launcherOf(categoryId) === 'folder') {
			const metaRow = meta.get(categoryId);
			const id = categoryId;
			const existing = buckets.get(id);
			if (existing) {
				existing.apps.push(app);
				absorbVisuals(existing, app);
				// Workspace logo/icon always win over member fallbacks.
				if (metaRow?.logo_url) existing.logo_url = metaRow.logo_url;
				if (metaRow?.icon) existing.icon = metaRow.icon;
			} else {
				buckets.set(id, {
					label: metaRow?.label ?? categoryLabel(categoryId),
					icon: metaRow?.icon ?? app.icon,
					logo_url: metaRow?.logo_url ?? app.logo_url,
					apps: [app],
					sort: (metaRow?.label ?? categoryId).toLowerCase()
				});
			}
			continue;
		}

		singles.push(app);
	}

	const entries: HomeEntry[] = [];
	for (const [id, bucket] of buckets) {
		if (bucket.apps.length === 1 && bucket.apps[0]) {
			entries.push({ kind: 'app', app: bucket.apps[0] });
		} else {
			entries.push({
				kind: 'folder',
				id,
				label: bucket.label,
				icon: bucket.icon,
				logo_url: bucket.logo_url,
				apps: bucket.apps.sort((a, b) => a.label.localeCompare(b.label))
			});
		}
	}
	for (const app of singles) {
		entries.push({ kind: 'app', app });
	}

	entries.sort((a, b) => {
		const la = a.kind === 'app' ? a.app.label : a.label;
		const lb = b.kind === 'app' ? b.app.label : b.label;
		return la.localeCompare(lb);
	});
	return entries;
}
