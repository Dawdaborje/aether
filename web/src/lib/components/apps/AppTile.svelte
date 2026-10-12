<script lang="ts">
	import { navHref } from '$lib/components/layout/nav';
	import * as Card from '$lib/components/ui/card/index.js';
	import { appIcon } from './icons';
	import type { AppTileData } from './types';

	let {
		app,
		href,
		interactive = true,
		onclick
	}: {
		app: AppTileData;
		/** Override the launcher route; pass `null` for a non-linking tile. */
		href?: string | null;
		interactive?: boolean;
		onclick?: () => void;
	} = $props();

	const Icon = $derived(appIcon(app.icon));
	const link = $derived(href === undefined ? navHref(app.route) : href);
	let logoFailed = $state(false);
	const showLogo = $derived(Boolean(app.logo_url) && !logoFailed);

	$effect(() => {
		void app.logo_url;
		logoFailed = false;
	});
</script>

{#snippet body()}
	<Card.Root
		size="sm"
		class="h-full transition-colors {interactive
			? 'group-hover:bg-accent/40 group-focus-visible:ring-2 group-focus-visible:ring-ring'
			: ''}"
	>
		<Card.Content
			class="flex flex-col items-center gap-2.5 px-(--card-spacing) pt-(--card-spacing) text-center"
		>
			<span
				class="flex size-12 items-center justify-center overflow-hidden bg-primary/10 text-primary ring-1 ring-primary/15 transition {interactive
					? 'group-hover:bg-primary/15 group-hover:ring-primary/30'
					: ''}"
				aria-hidden="true"
			>
				{#if showLogo && app.logo_url}
					<img
						src={app.logo_url}
						alt=""
						class="size-10 object-contain"
						onerror={() => (logoFailed = true)}
					/>
				{:else if Icon}
					<Icon class="size-6" />
				{:else}
					<span class="text-xl font-semibold">{app.label.charAt(0).toUpperCase()}</span>
				{/if}
			</span>
			<div class="max-w-full space-y-0.5">
				<Card.Title class="truncate text-[13px] font-medium">{app.label}</Card.Title>
				{#if app.description}
					<Card.Description class="line-clamp-2 text-[11px] leading-snug">
						{app.description}
					</Card.Description>
				{/if}
			</div>
		</Card.Content>
	</Card.Root>
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
