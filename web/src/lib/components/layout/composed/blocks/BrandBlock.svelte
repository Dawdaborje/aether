<script lang="ts">
	import { homePath } from '$lib/auth/landing';
	import type { LayoutBlock } from '$lib/theme/types';
	import { navHref } from '../../nav';
	import { getShellState } from '../shellContext.svelte';
	import { flagProp, textProp } from '../visibility';

	let { block }: { block: LayoutBlock } = $props();
	const shell = getShellState();

	// `follow="app"` turns the brand into the open app's name (and logo) while one is open.
	const following = $derived(textProp(block, 'follow', 'none') === 'app' && shell.openApp !== null);
	const label = $derived(following ? shell.openApp : textProp(block, 'label', shell.productName));
	const href = $derived(textProp(block, 'href', homePath()));
	const logo = $derived(following && shell.openAppLogo ? shell.openAppLogo : shell.logo);
	const showLogo = $derived(flagProp(block, 'show_logo', true) && Boolean(logo));
</script>

<!-- The organization's product name and logo (theme `branding`). -->
<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
<a
	href={navHref(href)}
	class="flex min-w-0 items-center gap-2 rounded-md px-1.5 py-1 font-semibold tracking-tight transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring"
>
	{#if showLogo}
		<img src={logo} alt="" class="size-6 shrink-0 object-contain" />
	{/if}
	{#if label}<span class="truncate">{label}</span>{/if}
</a>
<!-- eslint-enable svelte/no-navigation-without-resolve -->
