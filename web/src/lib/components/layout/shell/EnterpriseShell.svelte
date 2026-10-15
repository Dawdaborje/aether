<script lang="ts">
	import { page } from '$app/state';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import type { NavItem, ThemeNav } from '$lib/theme/types';
	import { activeApp, activeInstalledApp } from '../nav';
	import AppNavSidebar from './AppNavSidebar.svelte';
	import { groupAppMenus } from './groupAppMenus';
	import Navbar from './Navbar.svelte';

	let { nav, children }: { nav: ThemeNav; children: import('svelte').Snippet } = $props();

	const onApps = $derived(page.route.id?.startsWith('/(app)/(system)/apps') ?? false);
	const pagePlugin = $derived(
		typeof page.data?.body?.plugin === 'string' ? (page.data.body.plugin as string) : null
	);
	const themeApp = $derived(activeApp(nav, page.url.pathname));
	const installedApp = $derived(
		themeApp ? null : activeInstalledApp(appsStore.apps, page.url.pathname, pagePlugin)
	);

	/**
	 * Desk sections are a flat list of top-level links (Apps, Organizations, …).
	 * Users / Roles / Settings stay in the home menu — never as a fake app sidebar.
	 */
	const flatDeskMenus = $derived(
		Boolean(themeApp && !themeApp.children?.length && nav.items.length > 1)
	);

	/** Page menus for the left sidebar (desk sections or an installed app's pages). */
	const menus: NavItem[] = $derived.by(() => {
		if (onApps) return [];
		if (themeApp?.children?.length) return themeApp.children;
		if (installedApp?.menus?.length) {
			return groupAppMenus(installedApp.menus);
		}
		if (flatDeskMenus) return nav.items;
		return [];
	});

	$effect(() => {
		if (authSession.isUser && appsStore.apps === null) void appsStore.load();
	});
</script>

<!--
	Shell: slim top bar + left sidebar for the current app's pages + main canvas.
	Org management links (Users / Roles / Settings) live in the home menu only.
-->
<div class="flex h-screen flex-col overflow-hidden bg-canvas">
	<Navbar {nav} />
	<div class="flex min-h-0 flex-1">
		<AppNavSidebar {menus} />
		<!-- Scrolls for ordinary pages; full-height children (e.g. models studio) fill and manage their own panes. -->
		<main class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
			{@render children()}
		</main>
	</div>
</div>
