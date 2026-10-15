<script lang="ts">
	import { page } from '$app/state';
	import { authSession } from '$lib/auth/session.svelte';
	import { themeStore } from '$lib/theme';
	import ConfiguredLayout from './configured/ConfiguredLayout.svelte';
	import { layoutFor } from './registry';
	import PluginErrorModal from '$lib/components/errors/PluginErrorModal.svelte';

	let { children } = $props();

	// Developer tools live under `/desk`. The `/apps` launcher is the home page for
	// everyone and uses the organization's theme layout, not the desk chrome.
	const inDesk = $derived(page.route.id?.startsWith('/(app)/(system)/desk') ?? false);

	// A page can ask for its own layout (`<page layout="bare">`); the page's
	// `load` function puts it in route data before anything renders.
	const requested = $derived(typeof page.data?.layout === 'string' ? page.data.layout : null);

	// Otherwise:
	// - visitors get none: public pages carry no application chrome;
	// - developers get the desk layout on `/desk/*`;
	// - everyone else (including `/apps`) gets the organization's active theme.
	//   The desk is for developers only, so a theme that names it falls back to
	//   the default layout for other users.
	const name = $derived.by(() => {
		if (requested) return requested;
		if (!authSession.isUser) return 'bare';
		if (authSession.isDeveloper && inDesk) return 'desk';
		const themed = themeStore.theme.layout;
		return themed === 'desk' && !authSession.isDeveloper ? 'default' : themed;
	});

	const layoutDef = $derived.by(() => {
		const def = themeStore.theme.layoutDef;
		if (!def) return null;
		// Plugin layout defs apply when the active name matches the theme's layout
		// (or a page asks for that same name). Built-ins still use the Svelte registry.
		if (name === themeStore.theme.layout || (requested && requested === def.name)) {
			return def;
		}
		return null;
	});

	const Layout = $derived(layoutFor(name));
</script>

{#if layoutDef}
	<ConfiguredLayout def={layoutDef}>
		{@render children()}
	</ConfiguredLayout>
{:else}
	<Layout>
		{@render children()}
	</Layout>
{/if}

<!-- Failed plugin calls and plugins that did not load pop up here, on every page. -->
<PluginErrorModal />
