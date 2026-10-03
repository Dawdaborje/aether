<script lang="ts">
	import { authSession } from '$lib/auth/session.svelte';
	import { themeStore } from '$lib/theme';
	import { builtInNav } from '../nav';
	import EnterpriseShell from '../shell/EnterpriseShell.svelte';

	let { children } = $props();

	// The organization's theme may bring its own navigation; otherwise the
	// built-in one (Apps, plus Settings for developers).
	const nav = $derived(themeStore.theme.nav ?? builtInNav(authSession.isDeveloper));
</script>

<EnterpriseShell {nav}>
	{@render children()}
</EnterpriseShell>
