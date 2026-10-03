<script lang="ts">
	import { themeStore } from '$lib/theme';
	import NavMenu from '../NavMenu.svelte';
	import OrgSwitchButton from '../OrgSwitchButton.svelte';
	import UserMenu from '../shell/UserMenu.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { builtInNav } from '../nav';

	const nav = $derived(themeStore.theme.nav ?? builtInNav(authSession.isDeveloper));
</script>

<nav
	class="flex items-center justify-between border-b border-border bg-primary px-6 py-4 text-primary-foreground"
>
	<h1 class="text-lg font-semibold tracking-tight">{nav.header ?? themeStore.theme.label}</h1>
	<div class="flex items-center gap-4">
		<NavMenu items={nav.items} />
		<OrgSwitchButton tone="on-primary" />
		<UserMenu tone="on-primary" />
	</div>
</nav>
