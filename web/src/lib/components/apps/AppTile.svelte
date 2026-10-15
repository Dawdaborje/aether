<script lang="ts">
	import { navHref } from '$lib/components/layout/nav';
	import { appIcon } from './icons';
	import type { AppTileData } from './types';

	let {
		app,
		href,
		interactive = true,
		/** Compact for the home-menu Apps grid; default matches the full Apps launcher. */
		size = 'default',
		onclick
	}: {
		app: AppTileData;
		/** Override the launcher route; pass `null` for a non-linking tile. */
		href?: string | null;
		interactive?: boolean;
		size?: 'default' | 'compact';
		onclick?: () => void;
	} = $props();

	const Icon = $derived(appIcon(app.icon));
	const link = $derived(href === undefined ? navHref(app.route) : href);
	let logoFailed = $state(false);
	const showLogo = $derived(Boolean(app.logo_url) && !logoFailed);
	const compact = $derived(size === 'compact');

	$effect(() => {
		void app.logo_url;
		logoFailed = false;
	});
</script>

{#snippet body()}
	<!-- A launcher tile: icon and name, no frame until hovered. -->
	<div
		data-part="apps.tile"
		class="flex h-full flex-col items-center rounded-xl text-center transition-colors {compact
			? 'gap-2 p-3'
			: 'gap-3 p-4'} {interactive
			? 'group-hover:bg-accent group-focus-visible:ring-2 group-focus-visible:ring-ring'
			: ''}"
	>
		<span
			class="flex items-center justify-center overflow-hidden bg-primary/10 text-primary {compact
				? 'size-11 rounded-xl'
				: 'size-14 rounded-2xl sm:size-16'}"
			aria-hidden="true"
		>
			{#if showLogo && app.logo_url}
				<img
					src={app.logo_url}
					alt=""
					class="object-contain {compact ? 'size-8' : 'size-10 sm:size-12'}"
					onerror={() => (logoFailed = true)}
				/>
			{:else if Icon}
				<Icon class={compact ? 'size-5' : 'size-7'} />
			{:else}
				<span class="font-semibold {compact ? 'text-base' : 'text-xl'}"
					>{app.label.charAt(0).toUpperCase()}</span
				>
			{/if}
		</span>
		<div class="max-w-full space-y-0.5">
			<p class="truncate text-sm font-medium text-foreground">{app.label}</p>
			{#if app.description && !compact}
				<p class="line-clamp-2 text-xs leading-snug text-muted-foreground">
					{app.description}
				</p>
			{/if}
		</div>
	</div>
{/snippet}

{#if onclick}
	<button type="button" class="group block w-full outline-none" {onclick}>
		{@render body()}
	</button>
{:else if link}
	<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the app's base path -->
	<a href={link} class="group block outline-none">
		{@render body()}
	</a>
	<!-- eslint-enable svelte/no-navigation-without-resolve -->
{:else}
	<div class="block">
		{@render body()}
	</div>
{/if}
