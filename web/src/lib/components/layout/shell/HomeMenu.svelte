<script lang="ts">
	import type { LayoutBlock } from '$lib/theme/types';
	import { textProp } from '../composed/visibility';
	import { page } from '$app/state';
	import { iconRef } from '$lib/icons';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import AppTile from '$lib/components/apps/AppTile.svelte';
	import type { AppTileData } from '$lib/components/apps/types';
	import type { NavItem } from '$lib/theme/types';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { collapseDeskLinks, isActive, manageNavItems, navHref, pluginNavItems } from '../nav';

	let { block }: { block?: LayoutBlock; vertical?: boolean } = $props();

	/** `panel` (side panel), `dropdown` (under the button), `launcher` (full width) or `drawer` (narrow list). */
	const variant = $derived(
		textProp(block ?? ({ type: 'home-menu' } as LayoutBlock), 'variant', 'panel')
	);
	const trigger = $derived(
		textProp(block ?? ({ type: 'home-menu' } as LayoutBlock), 'trigger', 'grid')
	);
	const triggerLabel = $derived(
		textProp(block ?? ({ type: 'home-menu' } as LayoutBlock), 'label', 'Apps')
	);

	let open = $state(false);
	let filter = $state('');
	const allApps = $derived(
		(appsStore.apps ?? []).filter((app) =>
			`${app.label} ${app.description ?? ''}`.toLowerCase().includes(filter.trim().toLowerCase())
		)
	);
	/** The category the person clicked; null follows the app they are in. */
	let picked = $state<string | null>(null);

	const OTHER = '_other';

	/** The categories that have apps, in the order the server lists them, with their apps. */
	const groups = $derived.by(() => {
		const apps = appsStore.apps ?? [];
		const known = appsStore.categories ?? [];
		const by = new Map<string, AppTileData[]>();
		for (const app of apps) {
			const id = app.category && known.some((c) => c.id === app.category) ? app.category : OTHER;
			by.set(id, [...(by.get(id) ?? []), app]);
		}
		const out = known
			.filter((c) => by.has(c.id))
			.map((c) => ({ id: c.id, label: c.label, apps: by.get(c.id)! }));
		if (by.has(OTHER)) out.push({ id: OTHER, label: 'Other', apps: by.get(OTHER)! });
		return out;
	});

	/** The category of the app whose route the page is under (the longest match wins). */
	const currentCategory = $derived.by(() => {
		const path = page.url.pathname;
		let best: AppTileData | null = null;
		for (const app of appsStore.apps ?? []) {
			const base = navHref(app.route);
			const hit = path === base || path.startsWith(base.endsWith('/') ? base : `${base}/`);
			if (hit && (!best || app.route.length > best.route.length)) best = app;
		}
		return best?.category ?? null;
	});

	const shownId = $derived(
		[picked, currentCategory].find((id) => id && groups.some((g) => g.id === id)) ?? groups[0]?.id
	);
	const shown = $derived(groups.find((g) => g.id === shownId));

	/** Settings, Users, … — only what this account is allowed to open. */
	const manageItems = $derived(
		(orgStore.isSingle && authSession.isDeveloper
			? collapseDeskLinks
			: (items: NavItem[]) => items)(
			manageNavItems(
				authSession.isDeveloper,
				authSession.canManageUsers,
				authSession.canManageOrg,
				authSession.isOrgDeveloper,
				authSession.canManageRoles,
				pluginNavItems(appsStore.navLinks)
			)
		)
	);

	// Navigating anywhere closes the menu.
	$effect(() => {
		void page.url.pathname;
		open = false;
		picked = null;
	});
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<button
	type="button"
	onclick={() => (open = !open)}
	aria-label="Home menu"
	aria-expanded={open}
	title="Home menu"
	data-part="shell.home-menu"
	data-variant={variant}
	class="group flex h-9 min-w-9 items-center justify-center rounded-md transition outline-none hover:bg-region-hover focus-visible:ring-2 focus-visible:ring-region-ring aria-expanded:bg-region-hover"
>
	{#if trigger === 'hamburger'}
		<AppIcon name="list" class="size-5 text-region-muted-foreground" />
	{:else if trigger === 'label'}
		<span class="px-2 text-sm font-medium text-region-muted-foreground">{triggerLabel}</span>
	{:else}
		<span class="grid grid-cols-3 gap-[3px]" aria-hidden="true">
			{#each Array(9) as _, i (i)}
				<span
					class="size-[5px] rounded-[1.5px] bg-region-muted-foreground transition group-hover:bg-region-hover-foreground"
				></span>
			{/each}
		</span>
	{/if}
</button>

{#if open}
	<!-- Click-away backdrop below the bar. -->
	<button
		type="button"
		tabindex="-1"
		aria-label="Close home menu"
		onclick={() => (open = false)}
		class="fixed inset-x-0 top-12 bottom-0 z-40 cursor-default bg-foreground/10"
	></button>

	{#if variant !== 'panel'}
		<div
			role="presentation"
			data-part="shell.home-menu-body"
			onclick={(e) => (e.target as HTMLElement).closest('a') && (open = false)}
			class="fixed z-50 overflow-y-auto border-border bg-popover text-popover-foreground shadow-lg {variant ===
			'dropdown'
				? 'top-12 left-2 max-h-[70vh] w-[min(24rem,calc(100vw-1rem))] rounded-lg border'
				: variant === 'drawer'
					? 'top-12 bottom-0 left-0 w-72 border-r'
					: 'inset-x-0 top-12 bottom-0 border-t'}"
		>
			{#if variant === 'launcher'}
				<div class="mx-auto flex max-w-5xl flex-col gap-4 px-6 py-8">
					<input
						type="search"
						bind:value={filter}
						placeholder="Search apps…"
						aria-label="Search apps"
						class="h-11 w-full max-w-md border-b border-input bg-field px-3 text-sm outline-none focus-visible:border-ring"
					/>
					{#each groups as group (group.id)}
						{@const hits = group.apps.filter((a) => allApps.includes(a))}
						{#if hits.length > 0}
							<section>
								<h2 class="pb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
									{group.label}
								</h2>
								<ul class="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-3">
									{#each hits as app (app.plugin)}<li><AppTile {app} size="compact" /></li>{/each}
								</ul>
							</section>
						{/if}
					{/each}
				</div>
			{:else}
				<div class="flex flex-col gap-1 p-2">
					{#if variant === 'drawer' && orgStore.currentName}
						<p class="px-3 py-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
							{orgStore.currentName}
						</p>
					{/if}
					<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
					{#each appsStore.apps ?? [] as app (app.plugin)}
						{@const AppGlyph = iconRef(app.icon)}
						<a
							href={navHref(app.route)}
							class="flex items-center gap-3 rounded-md px-3 py-2 text-sm hover:bg-accent"
						>
							<span
								class="flex size-7 items-center justify-center rounded-md bg-primary/10 text-primary"
							>
								{#if AppGlyph}<AppIcon name={AppGlyph} class="size-4" />{:else}{app.label.charAt(
										0
									)}{/if}
							</span>
							<span class="truncate">{app.label}</span>
						</a>
					{/each}
					<a
						href={navHref('/apps')}
						class="mt-1 border-t border-border px-3 py-2 text-sm font-medium text-primary hover:bg-accent"
						>All apps</a
					>
					{#if variant === 'drawer'}
						{#each manageItems as item (item.href ?? item.label)}
							<a
								href={item.href ? navHref(item.href) : '#'}
								class="rounded-md px-3 py-2 text-sm text-muted-foreground hover:bg-accent"
								>{item.label}</a
							>
						{/each}
					{/if}
					<!-- eslint-enable svelte/no-navigation-without-resolve -->
				</div>
			{/if}
		</div>
	{:else}
		<div
			role="presentation"
			onclick={(e) => (e.target as HTMLElement).closest('a') && (open = false)}
			class="fixed top-12 bottom-0 left-0 z-50 w-full max-w-4xl overflow-y-auto border-r border-border bg-popover text-popover-foreground shadow-md"
		>
			<div class="flex w-full flex-col gap-6 px-6 py-6 md:flex-row md:gap-8">
				<div class="w-full shrink-0 space-y-5 md:w-64">
					{#if orgStore.currentName}
						<section>
							<h2
								class="px-3 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase"
							>
								Organization
							</h2>
							<button
								type="button"
								disabled={!orgStore.canSwitch}
								onclick={() => {
									open = false;
									orgStore.openSwitcher();
								}}
								class="flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm transition enabled:hover:bg-accent disabled:cursor-default"
							>
								<AppIcon name="building-2" class="size-5 text-muted-foreground" />
								<span class="flex-1 truncate font-medium">{orgStore.currentName}</span>
								{#if orgStore.canSwitch}<span class="text-xs text-muted-foreground">Switch</span
									>{/if}
							</button>
						</section>
					{/if}

					{#if manageItems.length > 0}
						<section>
							<h2
								class="px-3 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase"
							>
								Navigate
							</h2>
							<ul class="space-y-0.5">
								{#each manageItems as item (item.href ?? item.label)}
									{@const Icon = iconRef(item.icon)}
									<li>
										<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
										<a
											href={item.href ? navHref(item.href) : '#'}
											aria-current={isActive(item, page.url.pathname) ? 'page' : undefined}
											class="flex items-center gap-3 rounded-md px-3 py-2.5 text-sm transition hover:bg-accent aria-[current=page]:bg-accent aria-[current=page]:font-semibold"
										>
											{#if Icon}<AppIcon name={Icon} class="size-5 text-muted-foreground" />{/if}
											{item.label}
										</a>
										<!-- eslint-enable svelte/no-navigation-without-resolve -->
									</li>
								{/each}
							</ul>
						</section>
					{/if}
				</div>

				<section class="min-w-0 flex-1">
					<div class="flex items-center justify-between gap-2 px-3 pb-2">
						<h2 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">Apps</h2>
						<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
						<a
							href={navHref('/apps')}
							class="rounded-md bg-primary/10 px-2 py-1 text-xs font-medium text-primary hover:bg-primary/15"
						>
							All apps
						</a>
						<!-- eslint-enable svelte/no-navigation-without-resolve -->
					</div>
					{#if appsStore.apps === null}
						<p class="px-3 text-sm text-muted-foreground">Loading apps…</p>
					{:else if appsStore.apps.length === 0}
						<p class="px-3 text-sm text-muted-foreground">No apps installed yet.</p>
					{:else}
						<div class="flex flex-col gap-4 sm:flex-row">
							{#if groups.length > 1}
								<ul
									class="flex shrink-0 gap-1 overflow-x-auto sm:w-44 sm:flex-col"
									role="tablist"
									aria-label="App categories"
								>
									{#each groups as group (group.id)}
										{@const GroupIcon = iconRef(
											appsStore.categories?.find((c) => c.id === group.id)?.icon
										)}
										<li>
											<button
												type="button"
												role="tab"
												aria-selected={group.id === shownId}
												onclick={() => (picked = group.id)}
												class="flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm whitespace-nowrap transition hover:bg-accent aria-selected:bg-accent aria-selected:font-semibold"
											>
												{#if GroupIcon}<AppIcon
														name={GroupIcon}
														class="size-5 text-muted-foreground"
													/>{/if}
												<span class="flex-1 truncate">{group.label}</span>
												<span class="text-xs text-muted-foreground">{group.apps.length}</span>
											</button>
										</li>
									{/each}
								</ul>
							{/if}
							<ul
								class="grid min-w-0 flex-1 grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] content-start gap-3"
							>
								{#each shown?.apps ?? [] as app (app.plugin)}
									<li><AppTile {app} size="compact" /></li>
								{/each}
							</ul>
						</div>
					{/if}
				</section>
			</div>
		</div>
	{/if}
{/if}
