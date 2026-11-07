import { apiFetch } from '$lib/api/client';

/** Shown until the server answers, and when no name is set. */
export const DEFAULT_APP_NAME = 'Aether';
export const DEFAULT_LOGIN_HEADLINE = 'Welcome back';
export const DEFAULT_LOGIN_SUBTITLE = 'Sign in to pick up where you left off.';

/**
 * The application's public name (General settings → Application Name): the current
 * organization's own, else the global one. Drives the browser tab and the shell.
 */
class BrandingStore {
	appName = $state(DEFAULT_APP_NAME);
	/** Login page text (General settings → Login headline / subtitle). */
	loginHeadline = $state(DEFAULT_LOGIN_HEADLINE);
	loginSubtitle = $state(DEFAULT_LOGIN_SUBTITLE);
	/** The server runs with `is_development_mode = true`: not a production setup. */
	developmentMode = $state(false);

	/** Ask the server for the name that applies to whoever is looking now. */
	async load(): Promise<void> {
		try {
			const res = await apiFetch('/api/ui/branding');
			if (!res.ok) return;
			const body = (await res.json()) as {
				app_name?: string;
				login_headline?: string;
				login_subtitle?: string;
				development_mode?: boolean;
			};
			const name = body.app_name?.trim();
			this.appName = name || DEFAULT_APP_NAME;
			this.loginHeadline = body.login_headline?.trim() || DEFAULT_LOGIN_HEADLINE;
			this.loginSubtitle = body.login_subtitle?.trim() || DEFAULT_LOGIN_SUBTITLE;
			this.developmentMode = body.development_mode === true;
		} catch {
			// Keep what is shown: a missing name must never break the page.
		}
	}
}

export const brandingStore = new BrandingStore();
