import { notificationsStore } from '$lib/notifications/notificationsStore.svelte';
import { themeStore } from '$lib/theme';
import { appsStore } from './appsStore.svelte';

/**
 * The server announces `kernel` / `plugins.changed` when a plugin is installed, upgraded,
 * removed or switched in this organization (`plugins_changed` in `facets/core_api`). The theme
 * and the apps list are read again, so a new theme revision or a new app shows at once,
 * without a page refresh. Returns the function that stops listening.
 */
export function watchPluginChanges(): () => void {
	return notificationsStore.onEvent((event) => {
		if (event.plugin !== 'kernel' || event.event !== 'plugins.changed') return;
		void themeStore.loadFromApi();
		// Only when the launcher has been loaded; otherwise its first load reads the new state.
		if (appsStore.apps !== null) void appsStore.load();
	});
}
