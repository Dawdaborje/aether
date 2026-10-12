import { base } from '$app/paths';
import type { AppTileData } from '$lib/components/apps/types';
import type { NavItem, ThemeNav } from '$lib/theme/types';

/**
 * Navigation used when the active theme does not provide any. Settings is an
 * item of its own; only developers are offered it.
 */
export function builtInNav(isDeveloper: boolean): ThemeNav {
	return {
		header: 'Aether',
		items: isDeveloper ? [{ label: 'Settings', href: '/desk/settings', icon: 'settings' }] : []
	};
}

/**
 * The developer desk under `/desk`. Each top-level item is reached from the home
 * menu; an app with children shows them as the top bar's menus. The member-facing
 * app launcher stays at `/apps`.
 */
export const deskNav: ThemeNav = {
	header: 'Aether',
	items: [
		{ label: 'Apps', href: '/desk/apps', icon: 'layout-dashboard' },
		{ label: 'Bridges', href: '/desk/bridges', icon: 'plug' },
		{ label: 'Organizations', href: '/desk/organizations', icon: 'building-2' },
		{ label: 'Settings', href: '/desk/settings', icon: 'settings' },
		{ label: 'Developer tools', href: '/desk/studio', icon: 'wrench' }
	]
};

/**
 * The top-level item the current page belongs to (an "app" in Odoo's terms): the item
 * itself, or the one holding the page among its children.
 */
export function activeApp(nav: ThemeNav, pathname: string): NavItem | null {
	const inside = (item: NavItem): boolean =>
		isActive(item, pathname) || (item.children ?? []).some(inside);
	return nav.items.find(inside) ?? null;
}

/**
 * The installed `[app]` for the current path: longest matching `route` prefix.
 * When `plugin` is set (from the page API), prefer that plugin if its route also
 * matches, so two apps under a shared prefix stay unambiguous.
 */
export function activeInstalledApp(
	apps: AppTileData[] | null | undefined,
	pathname: string,
	plugin?: string | null
): AppTileData | null {
	if (!apps?.length) return null;
	const matching = apps.filter((app) => pathUnderApp(pathname, app.route));
	if (matching.length === 0) return null;
	if (plugin) {
		const byPlugin = matching.find((app) => app.plugin === plugin);
		if (byPlugin) return byPlugin;
	}
	matching.sort((a, b) => b.route.length - a.route.length);
	return matching[0] ?? null;
}

/** True when `pathname` is the app's entry route or a page under it. */
export function pathUnderApp(pathname: string, route: string): boolean {
	if (!route.startsWith('/')) return false;
	const target = navHref(route);
	return pathname === target || pathname.startsWith(`${target}/`);
}

/** App paths are served under the app's base path; absolute URLs are left alone. */
export function navHref(href: string): string {
	return href.startsWith('/') ? `${base}${href}` : href;
}

/** True when `pathname` is the item's page or inside it. */
export function isActive(item: NavItem, pathname: string): boolean {
	if (!item.href || !item.href.startsWith('/')) return false;
	const target = navHref(item.href);
	return pathname === target || pathname.startsWith(`${target}/`);
}
