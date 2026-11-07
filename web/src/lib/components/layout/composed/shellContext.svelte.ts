import { getContext, setContext } from 'svelte';
import { page } from '$app/state';
import { appsStore } from '$lib/apps/appsStore.svelte';
import { authSession } from '$lib/auth/session.svelte';
import { brandingStore } from '$lib/branding/brandingStore.svelte';
import { uiPrefs } from '$lib/theme/uiPrefs.svelte';
import { orgStore } from '$lib/org/orgStore.svelte';
import { themeStore } from '$lib/theme';
import type { NavItem, ThemeNav } from '$lib/theme/types';
import {
	activeApp,
	activeInstalledApp,
	builtInNav,
	deskNav,
	pluginNavItems,
	singleOrgNav
} from '../nav';
import { groupAppMenus } from '../shell/groupAppMenus';

/** Which navigation a layout's blocks read: the theme's, the developer desk's, or the management links. */
export type NavSource = 'theme' | 'desk' | 'manage';

/**
 * What the blocks of a composed layout share: the navigation, the app the current page
 * belongs to, and that app's page menus. Computed once per layout, read by every block.
 */
export class ShellState {
	navSource = $state<NavSource>('theme');

	/** The sidebar is folded (`mode="collapsible"`), a choice kept for this person. */
	sidebarCollapsed = $derived(uiPrefs.get<boolean>('sidebar.collapsed', false));
	/** Below `md` an `overlay` sidebar is open over the page. */
	sidebarOpen = $state(false);

	toggleSidebar() {
		this.sidebarOpen = !this.sidebarOpen;
		uiPrefs.set('sidebar.collapsed', !this.sidebarCollapsed);
	}

	/** A developer on `/desk/*`: the theme's layout is kept, but it shows the desk's navigation. */
	inDesk = $derived(
		authSession.isDeveloper && (page.route.id?.startsWith('/(app)/(system)/desk') ?? false)
	);

	nav: ThemeNav = $derived.by(() => navFor(this.inDesk ? 'desk' : this.navSource));

	/** The member launcher at `/apps`, which has no app of its own. */
	onApps = $derived(page.route.id?.startsWith('/(app)/(system)/apps') ?? false);

	private pagePlugin = $derived(
		typeof page.data?.body?.plugin === 'string' ? (page.data.body.plugin as string) : null
	);

	/** The theme nav entry holding this page (it wins), else the installed `[app]` for the path. */
	themeApp = $derived(activeApp(this.nav, page.url.pathname));
	installedApp = $derived(
		this.themeApp ? null : activeInstalledApp(appsStore.apps, page.url.pathname, this.pagePlugin)
	);

	productName = $derived(themeStore.theme.shell?.branding?.product_name ?? brandingStore.appName);
	logo = $derived(themeStore.theme.shell?.branding?.logo ?? null);

	/** The app the person is in, when there is one (not on the launcher or the home page). */
	openApp = $derived(
		this.onApps ? null : (this.themeApp?.label ?? this.installedApp?.label ?? null)
	);
	/** That app's own logo, when it has one. */
	openAppLogo = $derived(this.onApps ? null : (this.installedApp?.logo_url ?? null));

	/** Name of the current app, for the top bar. */
	appTitle = $derived(
		this.onApps
			? 'Apps'
			: (this.themeApp?.label ?? this.installedApp?.label ?? this.nav.header ?? this.productName)
	);

	/** The current app's page menus (desk sections or an installed app's pages). */
	appMenus: NavItem[] = $derived.by(() => {
		if (this.onApps) return [];
		if (this.themeApp?.children?.length) return this.themeApp.children;
		if (this.installedApp?.menus?.length) return groupAppMenus(this.installedApp.menus);
		// Desk sections are a flat list of top-level links (Apps, Organizations, …).
		const flat = this.themeApp && !this.themeApp.children?.length && this.nav.items.length > 1;
		return flat ? this.nav.items : [];
	});
}

function navFor(source: NavSource): ThemeNav {
	const nav = rawNavFor(source);
	if (!orgStore.isSingle) return nav;
	return { ...nav, items: singleOrgNav(nav.items, orgStore.current) };
}

function rawNavFor(source: NavSource): ThemeNav {
	if (source === 'desk') return deskNav;
	const manage = builtInNav(
		authSession.isDeveloper,
		authSession.canManageUsers,
		authSession.canManageOrg,
		authSession.isOrgDeveloper,
		authSession.canManageRoles,
		pluginNavItems(appsStore.navLinks)
	);
	if (source === 'manage') return manage;
	// A theme nav with only `footer` links leaves the app's own navigation alone.
	const themed = themeStore.theme.nav;
	return themed && themed.items.length > 0 ? themed : manage;
}

const KEY = Symbol('aether-shell');

export function setShellState(state: ShellState): ShellState {
	return setContext(KEY, state);
}

export function getShellState(): ShellState {
	const state = getContext<ShellState | undefined>(KEY);
	if (!state) throw new Error('layout blocks must be rendered inside ComposedLayout');
	return state;
}
