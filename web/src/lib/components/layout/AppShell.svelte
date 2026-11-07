<script lang="ts">
	import { page } from '$app/state';
	import { authSession } from '$lib/auth/session.svelte';
	import { themeStore } from '$lib/theme';
	import CustomLayout from './theme/CustomLayout.svelte';
	import { layoutFor } from './registry';
	import PluginErrorModal from '$lib/components/errors/PluginErrorModal.svelte';
	import { watchPluginChanges } from '$lib/apps/pluginsChanged';

	let { children } = $props();

	// A plugin (a theme among them) was installed or upgraded: reload the theme and apps live.
	$effect(() => watchPluginChanges());

	// Developer tools live under `/desk`. The `/apps` launcher is the home page for
	// everyone and uses the organization's theme layout, not the desk chrome.
	const inDesk = $derived(page.route.id?.startsWith('/(app)/(system)/desk') ?? false);

	// A page can ask for its own layout (`<page layout="bare">`); the page's
	// `load` function puts it in route data before anything renders.
	const requested = $derived(
		// `?embed` is a page shown inside another (the list's detail pane): no shell around it.
		page.url.searchParams.has('embed')
			? 'bare'
			: typeof page.data?.layout === 'string'
				? page.data.layout
				: null
	);

	// Otherwise:
	// - visitors get none: public pages carry no application chrome;
	// - developers on `/desk/*` get the theme's layout too, reading the desk's navigation;
	// - everyone else (including `/apps`) gets the organization's active theme.
	//   The desk is for developers only, so a theme that names it falls back to
	//   the default layout for other users.
	const name = $derived.by(() => {
		if (requested) return requested;
		if (!authSession.isUser) return 'bare';
		const themed = themeStore.theme.layout;
		return themed === 'desk' && !authSession.isDeveloper ? 'default' : themed;
	});

	// When the backend's theme reply carries a layout, that layout draws the app, on every page
	// (the desk included). Only a bare page (a visitor, `?embed`, `<page layout="bare">`) skips it.
	// The built-in layouts below are for a reply with no layout of its own.
	const layoutDef = $derived(name === 'bare' ? null : themeStore.theme.layoutDef);

	const Layout = $derived(layoutFor(name));
</script>

{#if layoutDef}
	<CustomLayout def={layoutDef}>
		{@render children()}
	</CustomLayout>
{:else}
	<Layout>
		{@render children()}
	</Layout>
{/if}

<!-- Failed plugin calls and plugins that did not load pop up here, on every page. -->
<PluginErrorModal />
