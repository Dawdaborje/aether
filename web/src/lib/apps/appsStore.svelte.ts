import { apiFetch } from '$lib/api/client';
import type { AppTileData } from '$lib/components/apps/types';
import { orgStore } from '$lib/org/orgStore.svelte';

/** Why there is nothing to show, when the list could not be loaded. */
export type AppsProblem = 'organization' | 'none-exist' | 'failed';

/**
 * The apps installed in the current organization. The home menu lists them and the top
 * bar offers them as links while you are on it, so both read this one list.
 */
class AppsStore {
	apps = $state<AppTileData[] | null>(null);
	problem = $state<AppsProblem | null>(null);

	async load(): Promise<void> {
		this.apps = null;
		this.problem = null;
		const res = await apiFetch('/api/ui/apps');
		if (res.ok) {
			this.apps = ((await res.json()) as { apps: AppTileData[] }).apps;
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
		}
		this.apps = [];
	}
}

export const appsStore = new AppsStore();
