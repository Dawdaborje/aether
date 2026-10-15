import { apiFetch } from '$lib/api/client';

/** Shown until the server answers, and when no name is set. */
export const DEFAULT_APP_NAME = 'Aether';

/**
 * The application's public name (General settings → Application Name): the current
 * organization's own, else the global one. Drives the browser tab and the shell.
 */
class BrandingStore {
	appName = $state(DEFAULT_APP_NAME);

	/** Ask the server for the name that applies to whoever is looking now. */
	async load(): Promise<void> {
		try {
			const res = await apiFetch('/api/ui/branding');
			if (!res.ok) return;
			const body = (await res.json()) as { app_name?: string };
			const name = body.app_name?.trim();
			this.appName = name || DEFAULT_APP_NAME;
		} catch {
			// Keep what is shown: a missing name must never break the page.
		}
	}
}

export const brandingStore = new BrandingStore();
