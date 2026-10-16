import { base } from '$app/paths';
import type { AppTileData } from '$lib/components/apps/types';
import type { NavItem, ThemeNav } from '$lib/theme/types';

/**
 * Role-gated management links for the home menu's Navigate section (and the
 * fallback theme nav when the organization theme does not supply one).
 *
 * - Developers: desk tools (organizations, users, apps, settings, …).
 * - Org admins (owner / org_admin): Users + Roles + Settings for the current org.
 * - User managers: Users + Roles for the current org.
 * - Org-scoped developers: Schedules for the current org (with whatever else they may manage).
 * - Plain members: nothing — Navigate stays hidden.
 */
export function builtInNav(
	isDeveloper: boolean,
	canManageUsers = false,
	canManageOrg = false,
	isOrgDeveloper = false
): ThemeNav {
	return {
		header: 'Aether',
		items: manageNavItems(isDeveloper, canManageUsers, canManageOrg, isOrgDeveloper)
	};
}

/** Links shown under Home → Navigate, filtered by what the signed-in user may do. */
export function manageNavItems(
	isDeveloper: boolean,
	canManageUsers = false,
	canManageOrg = false,
	isOrgDeveloper = false
): NavItem[] {
	if (isDeveloper) {
		return [
			{ label: 'Organizations', href: '/desk/organizations', icon: 'building-2' },
			{ label: 'Users', href: '/desk/users', icon: 'users' },
			{ label: 'Apps', href: '/desk/apps', icon: 'layout-dashboard' },
			{ label: 'Settings', href: '/desk/settings', icon: 'settings' },
			{ label: 'Schedules', href: '/desk/schedules', icon: 'clock' },
			{ label: 'Mantle', href: '/desk/mantle', icon: 'flame' },
			{ label: 'Bridges', href: '/desk/bridges', icon: 'plug' }
		];
	}
	const items: NavItem[] = [];
	if (canManageUsers) {
		items.push({ label: 'Users', href: '/users', icon: 'users' });
		items.push({ label: 'Roles', href: '/roles', icon: 'shield' });
	}
	if (canManageOrg) {
		items.push({ label: 'Settings', href: '/desk/settings', icon: 'settings' });
	}
	if (isOrgDeveloper) {
		items.push({ label: 'Schedules', href: '/schedules', icon: 'clock' });
	}
	return items;
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
		{ label: 'Schedules', href: '/desk/schedules', icon: 'clock' },
		{ label: 'Mantle', href: '/desk/mantle', icon: 'flame' },
		{ label: 'Organizations', href: '/desk/organizations', icon: 'building-2' },
		{ label: 'Users', href: '/desk/users', icon: 'users' },
		{ label: 'Settings', href: '/desk/settings', icon: 'settings' },
		{ label: 'Developer tools', href: '/desk/studio', icon: 'wrench' }
	]
};

/**
 * The top-level item the current page belongs to (an "app"): the item
 * itself, or the one holding the page among its children.
 */
export function activeApp(nav: ThemeNav, pathname: string): NavItem | null {
	const inside = (item: NavItem): boolean =>
		isActive(item, pathname) || (item.children ?? []).some(inside);
	return nav.items.find(inside) ?? null;
}

/**
 * The installed `[app]` for the current path: longest matching `route` / menu.
 * When `plugin` is set (from the page API), prefer that plugin's tile when it
 * still exists. Thin workspace apps are often folded into a host tile — then we
 * resolve by path so the host's merged menus still drive the sidebar.
 */
export function activeInstalledApp(
	apps: AppTileData[] | null | undefined,
	pathname: string,
	plugin?: string | null
): AppTileData | null {
	if (!apps?.length) return null;
	if (plugin) {
		const byPlugin = apps.find((app) => app.plugin === plugin);
		// Prefer the page's own tile when it still exposes menus (or owns the path).
		if (byPlugin && ((byPlugin.menus?.length ?? 0) > 0 || appCoversPath(byPlugin, pathname))) {
			return byPlugin;
		}
	}
	const matching = apps.filter((app) => appCoversPath(app, pathname));
	if (matching.length === 0) return null;
	matching.sort((a, b) => {
		const aScore = Math.max(a.route.length, ...(a.menus ?? []).map((m) => m.href.length));
		const bScore = Math.max(b.route.length, ...(b.menus ?? []).map((m) => m.href.length));
		return bScore - aScore;
	});
	return matching[0] ?? null;
}

/** True when the installed app owns this path (its entry route or one of its menus). */
export function appCoversPath(app: AppTileData, pathname: string): boolean {
	if (pathUnderApp(pathname, app.route)) return true;
	return (app.menus ?? []).some((item) => pathUnderApp(pathname, item.href));
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
	if (pathname === target || pathname.startsWith(`${target}/`)) {
		// Prefer the longest matching sibling so a short prefix (Employees) does not stay
		// current when a longer menu path is the real page (only when both are in `children`).
		return true;
	}
	return false;
}

/**
 * Among sibling menu links, mark current only the longest href that covers `pathname`
 * (avoids Employees + New employee both showing as current).
 */
export function isMenuCurrent(item: NavItem, pathname: string, siblings: NavItem[] = []): boolean {
	if (!isActive(item, pathname)) return false;
	const longer = siblings.some(
		(other) =>
			other !== item &&
			other.href &&
			other.href.length > (item.href?.length ?? 0) &&
			isActive(other, pathname)
	);
	return !longer;
}
