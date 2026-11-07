<script lang="ts">
	import { themeStore } from '$lib/theme/themeStore.svelte';
	import { navHref } from '$lib/components/layout/nav';
	import { iconRef } from '$lib/icons';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { navBadges } from '$lib/components/layout/composed/nav/navBadges.svelte';
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

	const Icon = $derived(iconRef(app.icon));
	const link = $derived(href === undefined ? navHref(app.route) : href);
	let logoFailed = $state(false);
	const showLogo = $derived(Boolean(app.logo_url) && !logoFailed);
	const compact = $derived(size === 'compact');

	// A live number on the full-size tile (`[app.tile]`), asked while the tile is on screen.
	const tileFunction = $derived(compact ? null : (app.tile?.function ?? null));
	const tileNumber = $derived(tileFunction ? (navBadges.numbers[tileFunction] ?? null) : null);
	$effect(() => {
		if (tileFunction) return navBadges.use(tileFunction);
	});

	$effect(() => {
		void app.logo_url;
		logoFailed = false;
	});
</script>

{#snippet body()}
	{@const variant = compact ? 'grid' : (themeStore.theme.tokens.launcher?.variant ?? 'grid')}
	<!-- A launcher tile: icon and name, no frame until hovered. -->
	<div
		data-part="apps.tile"
		data-variant={variant}
		class="flex h-full rounded-tile transition-colors {variant === 'list'
			? 'flex-row items-center gap-3 p-2 text-left'
			: variant === 'cards'
				? 'flex-col items-start border border-border bg-card p-4 text-left'
				: 'flex-col items-center text-center'} {compact
			? 'gap-2 p-3'
			: 'gap-3 p-4'} {interactive
			? 'group-hover:bg-accent group-focus-visible:ring-2 group-focus-visible:ring-ring'
			: ''}"
	>
		<span
			class="flex shrink-0 items-center justify-center overflow-hidden bg-primary/10 text-primary {compact
				? 'size-11 rounded-tile'
				: 'size-14 rounded-tile sm:size-16'}"
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
				<AppIcon name={Icon} class={compact ? 'size-5' : 'size-7'} />
			{:else}
				<span class="font-semibold {compact ? 'text-base' : 'text-xl'}"
					>{app.label.charAt(0).toUpperCase()}</span
				>
			{/if}
		</span>
		<div class="max-w-full min-w-0 space-y-0.5">
			<p class="truncate text-sm font-medium text-foreground">{app.label}</p>
			{#if app.problem}
				<span
					class="inline-block rounded-full bg-destructive/10 px-2 py-0.5 text-xs font-medium text-destructive"
					title={app.problem}
					data-testid="app-problem"
				>
					Load failed
				</span>
			{/if}
			{#if tileNumber !== null && app.tile}
				<p data-part="apps.tile-number" class="text-lg font-semibold text-foreground tabular-nums">
					{tileNumber.toLocaleString()}{app.tile.unit ?? ''}
					{#if app.tile.label}
						<span class="block text-xs font-normal text-muted-foreground">{app.tile.label}</span>
					{/if}
				</p>
			{/if}
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
