<script lang="ts">
	import { homePath } from '$lib/auth/landing';
	import { navHref } from '../../nav';
	import { getShellState } from '../shellContext.svelte';

	const shell = getShellState();
</script>

<!-- The current app's name; a link back to the home page everywhere but the launcher. -->
{#if shell.onApps}
	<span class="mx-1.5 truncate text-base font-semibold tracking-tight">
		{shell.appTitle}
	</span>
{:else}
	<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
	<a
		href={navHref(homePath())}
		title="Home"
		class="mx-1 truncate rounded-md px-1.5 py-0.5 text-base font-semibold tracking-tight transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring"
	>
		{shell.appTitle}
	</a>
	<!-- eslint-enable svelte/no-navigation-without-resolve -->
{/if}
