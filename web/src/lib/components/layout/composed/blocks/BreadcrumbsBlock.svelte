<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import type { LayoutBlock } from '$lib/theme/types';
	import { navHref } from '../../nav';
	import { pageTrail } from '../../pageTrail.svelte';
	import { getShellState } from '../shellContext.svelte';
	import { shellTrail } from '../trail';
	import { flagProp, textProp } from '../visibility';

	let { block, vertical = false }: { block: LayoutBlock; vertical?: boolean } = $props();
	const shell = getShellState();

	const STYLES = ['chevron', 'slash', 'arrows'] as const;
	const look = $derived(STYLES.find((style) => style === textProp(block, 'style')) ?? STYLES[0]);
	const scope = $derived(flagProp(block, 'scope'));

	const appHome = $derived(shell.themeApp?.href ?? shell.installedApp?.route);
	const trail = $derived(
		shellTrail({
			org: scope ? orgStore.currentName : null,
			app: shell.onApps ? null : { label: shell.appTitle, href: appHome },
			menus: shell.appMenus,
			pathname: page.url.pathname,
			page: pageTrail.crumbs
		})
	);

	// While this block is on screen the page's control panel shows only the page title.
	onMount(() => {
		pageTrail.shellBlocks += 1;
		return () => {
			pageTrail.shellBlocks -= 1;
		};
	});

	const crumbClass =
		'truncate rounded-sm text-region-muted-foreground transition outline-none hover:text-region-hover-foreground hover:underline focus-visible:ring-2 focus-visible:ring-region-ring';
</script>

<!-- The page's place: organization and app (with `scope`), then the page's own trail. -->
<nav aria-label="Breadcrumb" data-part="breadcrumbs" data-style={look} class="min-w-0">
	<ol
		class="flex min-w-0 items-center text-sm {look === 'arrows' ? 'gap-0' : 'gap-1.5'} {vertical
			? 'flex-wrap gap-y-1'
			: ''}"
	>
		{#each trail as crumb, i (i + crumb.label)}
			{@const last = i === trail.length - 1}
			{#if i > 0 && look !== 'arrows'}
				<li role="presentation" aria-hidden="true" class="text-region-muted-foreground">
					{#if look === 'slash'}
						<span class="px-0.5 opacity-60">/</span>
					{:else}
						<AppIcon name="chevron-right" class="size-3.5" />
					{/if}
				</li>
			{/if}
			<li
				data-part="breadcrumbs.item"
				class="flex min-w-0 items-center {look === 'arrows'
					? 'crumb-arrow bg-region-hover px-3 py-1 first:pl-2.5 last:bg-region-active'
					: ''} {last ? 'shrink-0' : ''}"
			>
				{#if last}
					<span aria-current="page" class="truncate font-medium text-region-foreground"
						>{crumb.label}</span
					>
				{:else if scope && i === 0 && orgStore.canSwitch && crumb.label === orgStore.currentName}
					<!-- The organization crumb switches organization. -->
					<button
						type="button"
						title="Switch organization"
						onclick={() => orgStore.openSwitcher()}
						class="{crumbClass} cursor-pointer"
					>
						{crumb.label}
					</button>
				{:else if crumb.href}
					<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
					<a href={navHref(crumb.href)} class={crumbClass}>{crumb.label}</a>
					<!-- eslint-enable svelte/no-navigation-without-resolve -->
				{:else}
					<span class="truncate text-region-muted-foreground">{crumb.label}</span>
				{/if}
			</li>
		{/each}
	</ol>
</nav>

<style>
	/* Elastic-style arrows: each crumb is a segment pointing at the next. */
	.crumb-arrow {
		clip-path: polygon(
			0 0,
			calc(100% - 0.5rem) 0,
			100% 50%,
			calc(100% - 0.5rem) 100%,
			0 100%,
			0.5rem 50%
		);
		margin-inline-start: -0.25rem;
		padding-inline-end: 1rem;
	}
	.crumb-arrow:first-child {
		clip-path: polygon(0 0, calc(100% - 0.5rem) 0, 100% 50%, calc(100% - 0.5rem) 100%, 0 100%);
		margin-inline-start: 0;
	}
</style>
