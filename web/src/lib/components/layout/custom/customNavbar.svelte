<script lang="ts">
	import { themeStore } from '$lib/theme';
	import NavMenu from '../NavMenu.svelte';
	import OrgSwitchButton from '../OrgSwitchButton.svelte';
	import UserMenu from '../shell/UserMenu.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { builtInNav, pluginNavItems } from '../nav';
	import { appsStore } from '$lib/apps/appsStore.svelte';

	const nav = $derived(
		themeStore.theme.nav ??
			builtInNav(
				authSession.isDeveloper,
				authSession.canManageUsers,
				authSession.canManageOrg,
				authSession.isOrgDeveloper,
				authSession.canManageRoles,
				pluginNavItems(appsStore.navLinks)
			)
	);
</script>

<nav
	class="flex items-center justify-between border-b border-border bg-primary px-6 py-4 text-primary-foreground"
>
	<h1 class="type-title">{nav.header ?? themeStore.theme.label}</h1>
	<div class="flex items-center gap-4">
		<NavMenu items={nav.items} />
		<OrgSwitchButton tone="on-primary" />
		<UserMenu tone="on-primary" />
	</div>
</nav>
