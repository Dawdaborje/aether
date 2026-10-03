import { base } from '$app/paths';
import type { NavItem, ThemeNav } from '$lib/theme/types';

/**
 * Navigation used when the active theme does not provide any. Settings is an
 * item of its own; only developers are offered it.
 */
export function builtInNav(isDeveloper: boolean): ThemeNav {
	return {
		header: 'Aether',
		items: isDeveloper ? [{ label: 'Settings', href: '/settings', icon: 'settings' }] : []
	};
}

/**
 * The developer desk. Like Odoo's apps, each top-level item is an app of its own,
 * reached from the home menu; an app with children shows them as the top bar's menus.
 */
export const deskNav: ThemeNav = {
	header: 'Aether',
	items: [
		{ label: 'Apps', href: '/apps', icon: 'layout-dashboard' },
		{ label: 'Bridges', href: '/bridges', icon: 'plug' },
		{ label: 'Organizations', href: '/organizations', icon: 'building-2' },
		{ label: 'Settings', href: '/settings', icon: 'settings' },
		{ label: 'Developer tools', href: '/studio', icon: 'wrench' }
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
