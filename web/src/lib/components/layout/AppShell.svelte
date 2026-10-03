<script lang="ts">
	import { page } from '$app/state';
	import { authSession } from '$lib/auth/session.svelte';
	import { themeStore } from '$lib/theme';
	import { layoutFor } from './registry';

	let { children } = $props();

	const inDesk = $derived(page.route.id?.startsWith('/(app)/(system)') ?? false);

	// A page can ask for its own layout (`<page layout="bare">`); the page's
	// `load` function puts it in route data before anything renders.
	const requested = $derived(typeof page.data?.layout === 'string' ? page.data.layout : null);

	// Otherwise:
	// - visitors get none: public pages carry no application chrome;
	// - developers get the desk on the system pages (organizations, settings,
	//   developer tools, and the Apps launcher);
	// - everyone else gets the layout of the organization's active theme. The
	//   desk is for developers only, so a theme that names it falls back to the
	//   default layout for other users.
	const name = $derived.by(() => {
		if (requested) return requested;
		if (!authSession.isUser) return 'bare';
		if (authSession.isDeveloper && inDesk) return 'desk';
		const themed = themeStore.theme.layout;
		return themed === 'desk' && !authSession.isDeveloper ? 'default' : themed;
	});
	const Layout = $derived(layoutFor(name));
</script>

<Layout>
	{@render children()}
</Layout>
