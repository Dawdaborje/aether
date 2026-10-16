import { apiFetch } from '$lib/api/client';
import type { AppCategory, AppTileData } from '$lib/components/apps/types';
import { orgStore } from '$lib/org/orgStore.svelte';
import { classifyStatus, pageError } from '$lib/pages/pageContext.svelte';

/** Why there is nothing to show, when the list could not be loaded. */
export type AppsProblem = 'organization' | 'none-exist' | 'failed';

/**
 * The apps installed in the current organization. The home menu lists them and the
 * Apps launcher groups them by workspace category.
 */
class AppsStore {
	apps = $state<AppTileData[] | null>(null);
	categories = $state<AppCategory[] | null>(null);
	problem = $state<AppsProblem | null>(null);

	async load(): Promise<void> {
		this.apps = null;
		this.categories = null;
		this.problem = null;
		const res = await apiFetch('/api/ui/apps');
		if (res.ok) {
			const body = (await res.json()) as {
				apps: AppTileData[];
				categories?: AppCategory[];
			};
			this.apps = body.apps;
			this.categories = body.categories ?? [];
			return;
		}
		if (res.status === 409) {
			// Several organizations and none chosen yet: ask.
			void orgStore.requireSelection();
			this.problem = 'organization';
		} else if (res.status === 404) {
			// A developer who belongs to no organization has not entered one yet,
			// or there is no organization at all.
			this.problem = orgStore.organizations.length === 0 ? 'none-exist' : 'organization';
		} else {
			this.problem = 'failed';
			pageError.current = {
				kind: classifyStatus(res.status),
				status: res.status,
				message: `The apps could not be loaded (${res.status}).`,
				plugin: 'aether',
				fn: ''
			};
		}
		this.apps = [];
		this.categories = [];
	}
}

export const appsStore = new AppsStore();
