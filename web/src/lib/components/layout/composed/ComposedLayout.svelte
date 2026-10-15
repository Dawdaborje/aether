<script lang="ts">
	import type { Snippet } from 'svelte';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import type { LayoutDef } from '$lib/theme/types';
	import Region from './Region.svelte';
	import { ShellState, setShellState, type NavSource } from './shellContext.svelte';

	let { def, children }: { def: LayoutDef; children: Snippet } = $props();

	const shell = setShellState(new ShellState());

	/** The navigation the layout reads: the `source` of its first `app-menu` or `nav` block. */
	const navSource = $derived.by((): NavSource => {
		for (const region of Object.values(def.regions ?? {})) {
			for (const block of region?.blocks ?? []) {
				const source = block.props?.source;
				if ((block.type === 'app-menu' || block.type === 'nav') && typeof source === 'string') {
					if (source === 'desk' || source === 'manage' || source === 'theme') return source;
				}
			}
		}
		return 'theme';
	});
	$effect.pre(() => {
		shell.navSource = navSource;
	});

	// App menus and the home menu list the installed apps.
	$effect(() => {
		if (authSession.isUser && appsStore.apps === null) void appsStore.load();
	});

	const regions = $derived(def.regions ?? {});
	const sideLeft = $derived(
		regions.sidebar && regions.sidebar.position !== 'right' ? regions.sidebar : null
	);
	const sideRight = $derived(regions.sidebar?.position === 'right' ? regions.sidebar : null);
</script>

<!--
	A layout built from regions and blocks (`layouts/<name>.json`). The built-in default
	and desk layouts use this too; see `builtins.ts`.
-->
<div class="flex h-screen flex-col overflow-hidden bg-canvas" data-layout={def.name}>
	{#if regions.banner}<Region name="banner" region={regions.banner} />{/if}
	{#if regions.topbar}<Region name="topbar" region={regions.topbar} />{/if}
	<div class="flex min-h-0 flex-1">
		{#if sideLeft}<Region name="sidebar" region={sideLeft} />{/if}
		<!-- Scrolls for ordinary pages; full-height children (e.g. models studio) manage their own panes. -->
		<main class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto" data-part="shell.content">
			<div class="flex min-h-0 flex-1 flex-col">{@render children()}</div>
			{#if regions.footer}<Region name="footer" region={regions.footer} />{/if}
		</main>
		{#if regions.aside}<Region name="aside" region={regions.aside} />{/if}
		{#if sideRight}<Region name="sidebar" region={sideRight} />{/if}
	</div>
</div>
