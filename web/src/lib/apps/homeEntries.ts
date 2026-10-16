import { categoryIcon, categoryLabel } from '$lib/apps/categories';
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


/**
 * Build the Apps home grid.
 *
 * - `[app] folder="…"` → plugins that share a folder (same workspace) form a folder tile.
 * - Workspace `launcher = "flat"` → remaining members are individual tiles (Base).
 * - Workspace `launcher = "folder"` (default) → remaining members share the workspace folder.
 * - A folder with one app stays a folder, so every role sees the workspace's icon.
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
			} else {
				buckets.set(id, {
					label: pluginFolder,
					icon: categoryIcon(categoryId),
					logo_url: null,
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
			} else {
				buckets.set(id, {
					label: metaRow?.label ?? categoryLabel(categoryId),
					icon: metaRow?.icon ?? categoryIcon(categoryId),
					logo_url: metaRow?.logo_url ?? null,
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
		// A workspace always shows as its folder, even when a role leaves one app in it, so
		// members see the same icons a developer does.
		entries.push({
			kind: 'folder',
			id,
			label: bucket.label,
			icon: bucket.icon,
			logo_url: bucket.logo_url,
			apps: bucket.apps.sort((a, b) => a.label.localeCompare(b.label))
		});
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
