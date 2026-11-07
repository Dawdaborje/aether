<script lang="ts">
	import { page } from '$app/state';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { uiPrefs } from '$lib/theme/uiPrefs.svelte';
	import { navHref } from '../../nav';
	import { getShellState } from '../shellContext.svelte';

	type Pin = { label: string; href: string };
	const shell = getShellState();

	const pins = $derived(uiPrefs.get<Pin[]>('pinned', []));
	/** The open page as a pin: its app-relative path and the title the shell shows. */
	const here = $derived.by((): Pin | null => {
		const base = navHref('/');
		const path = page.url.pathname;
		const rel = path.startsWith(base) ? `/${path.slice(base.length)}`.replace('//', '/') : path;
		if (shell.onApps || rel === '/') return null;
		return {
			label: shell.openApp ? `${shell.openApp}: ${document.title}` : document.title,
			href: rel
		};
	});
	const pinned = $derived(here !== null && pins.some((pin) => pin.href === here.href));

	function toggle() {
		if (!here) return;
		uiPrefs.set(
			'pinned',
			pinned ? pins.filter((pin) => pin.href !== here!.href) : [...pins, here].slice(-12)
		);
	}
</script>

<!-- The person's pinned pages, and a star to pin or unpin the open one. -->
<div data-part="shell.pinned" class="flex min-w-0 flex-col gap-0.5 text-sm">
	{#each pins as pin (pin.href)}
		<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
		<a
			href={navHref(pin.href)}
			class="truncate rounded-md px-2.5 py-1.5 text-region-muted-foreground transition hover:bg-region-hover hover:text-region-hover-foreground"
			>{pin.label}</a
		>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	{/each}
	{#if here}
		<button
			type="button"
			aria-pressed={pinned}
			onclick={toggle}
			class="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-region-muted-foreground hover:bg-region-hover"
		>
			<AppIcon name="star" class="size-3.5 {pinned ? 'text-region-primary' : ''}" />
			{pinned ? 'Unpin this page' : 'Pin this page'}
		</button>
	{/if}
</div>
