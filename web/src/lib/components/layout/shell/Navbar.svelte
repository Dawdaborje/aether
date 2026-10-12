<script lang="ts">
	import { page } from '$app/state';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { appIcon } from '$lib/components/apps/icons';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import type { ThemeNav } from '$lib/theme/types';
	import { activeApp, activeInstalledApp, isActive, navHref } from '../nav';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import type { NavItem } from '$lib/theme/types';
	import HomeMenu from './HomeMenu.svelte';
	import NotificationBell from '$lib/notifications/NotificationBell.svelte';
	import UserMenu from './UserMenu.svelte';

	let { nav }: { nav: ThemeNav } = $props();

	// The member launcher at `/apps` — not `/desk/apps`.
	const onApps = $derived(page.route.id?.startsWith('/(app)/(system)/apps') ?? false);
	const pagePlugin = $derived(
		typeof page.data?.body?.plugin === 'string' ? (page.data.body.plugin as string) : null
	);
	// Theme nav wins when it matches; otherwise the installed `[app]` for this path
	// (Countries, etc.) names the bar and supplies menus from its pages.
	const themeApp = $derived(activeApp(nav, page.url.pathname));
	const installedApp = $derived(
		themeApp ? null : activeInstalledApp(appsStore.apps, page.url.pathname, pagePlugin)
	);
	const brand = $derived(
		onApps ? 'Apps' : (themeApp?.label ?? installedApp?.label ?? nav.header ?? 'Aether')
	);
	const menus: NavItem[] = $derived.by(() => {
		if (onApps) {
			return (appsStore.apps ?? []).map((a) => ({
				label: a.label,
				href: a.route,
				icon: a.icon ?? undefined
			}));
		}
		if (themeApp?.children?.length) return themeApp.children;
		if (installedApp?.menus?.length) {
			return installedApp.menus.map((item) => ({
				label: item.label,
				href: item.href,
				icon: item.icon ?? undefined
			}));
		}
		return themeApp && nav.items.length > 1 ? nav.items : [];
	});
	// The home menu lists the installed apps, so make sure they are loaded on every page.
	$effect(() => {
		if (authSession.isUser && appsStore.apps === null) void appsStore.load();
	});
	async function leaveOrganization() {
		await orgStore.leave();
		if (!orgStore.error) await goto(resolve('/desk/organizations'));
	}

	const trigger =
		'flex h-8 items-center gap-1 rounded-md px-2.5 text-[13px] font-medium text-sidebar-foreground/80 outline-none transition hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground';
</script>

{#snippet link(entry: NavItem)}
	{@const Icon = appIcon(entry.icon)}
	<DropdownMenu.Item class="gap-2.5 py-2">
		{#snippet child({ props })}
			<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
			<a
				href={entry.href ? navHref(entry.href) : '#'}
				{...props}
				aria-current={isActive(entry, page.url.pathname) ? 'page' : undefined}
				class="{props.class} aria-[current=page]:font-semibold aria-[current=page]:text-primary"
			>
				{#if Icon}<Icon class="size-4 text-muted-foreground" />{/if}
				{entry.label}
			</a>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
		{/snippet}
	</DropdownMenu.Item>
{/snippet}

<!--
	The top bar, laid out the way Odoo's is: the apps button at the far left (it returns to
	the home menu), the current app's name, that app's menus, and the systray on the right.
-->
<header
	class="sticky top-0 z-30 flex h-12 shrink-0 items-center gap-1 border-b border-sidebar-border bg-sidebar px-2 text-sidebar-foreground"
>
	<HomeMenu {nav} />

		<span class="mx-1.5 text-[15px] font-semibold tracking-tight text-sidebar-foreground">{brand}</span>
		<nav class="flex items-center gap-0.5" aria-label="Menu">
			{#each menus as item (item.label)}
				{#if item.children?.length}
					<DropdownMenu.Root>
						<DropdownMenu.Trigger class={trigger}>
							{item.label}
							<ChevronDownIcon class="size-3 opacity-60" />
						</DropdownMenu.Trigger>
						<DropdownMenu.Content align="start" class="min-w-52">
							{#each item.children as entry (entry.label)}
								{@const Icon = appIcon(entry.icon)}
								<DropdownMenu.Item class="gap-2.5 py-2">
									{#snippet child({ props })}
										<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
										<a
											href={entry.href ? navHref(entry.href) : "#"}
											{...props}
											aria-current={isActive(entry, page.url.pathname) ? 'page' : undefined}
											class="{props.class} aria-[current=page]:font-semibold aria-[current=page]:text-primary"
										>
											{#if Icon}<Icon class="size-4 text-muted-foreground" />{/if}
											{entry.label}
										</a>
										<!-- eslint-enable svelte/no-navigation-without-resolve -->
									{/snippet}
								</DropdownMenu.Item>
							{/each}
						</DropdownMenu.Content>
					</DropdownMenu.Root>
				{:else if item.href}
					<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
					<a
						href={navHref(item.href)}
						aria-current={isActive(item, page.url.pathname) ? 'page' : undefined}
						class="{trigger} aria-[current=page]:bg-sidebar-accent aria-[current=page]:text-sidebar-accent-foreground aria-[current=page]:shadow-[inset_0_-2px_0_0_var(--sidebar-primary)]"
					>
						{item.label}
					</a>
					<!-- eslint-enable svelte/no-navigation-without-resolve -->
				{/if}
			{/each}
		</nav>

	<div class="flex-1"></div>

	<div class="flex items-center gap-2 pr-1">
		{#if authSession.isDeveloper}
			<span
				class="hidden rounded-full bg-sidebar-primary/15 px-2.5 py-0.5 text-[10px] font-semibold tracking-[0.14em] text-sidebar-primary uppercase ring-1 ring-sidebar-primary/30 ring-inset sm:inline"
				>Developer</span
			>
		{/if}
		{#if authSession.isUser}
			<NotificationBell />
		{/if}
		{#if orgStore.currentName}
			<DropdownMenu.Root>
				<DropdownMenu.Trigger
					disabled={!orgStore.canSwitch && !orgStore.canLeave}
					aria-label="Organization"
					class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-[13px] text-sidebar-foreground/85 outline-none transition enabled:cursor-pointer enabled:hover:bg-sidebar-accent enabled:hover:text-sidebar-accent-foreground disabled:cursor-default data-[state=open]:bg-sidebar-accent"
				>
					<span class="size-1.5 rounded-full bg-sidebar-primary"></span>
					{orgStore.currentName}
					{#if orgStore.canSwitch || orgStore.canLeave}<ChevronDownIcon class="size-3 opacity-60" />{/if}
				</DropdownMenu.Trigger>
				<DropdownMenu.Content align="end" class="min-w-52">
					{#if orgStore.canSwitch}
						<DropdownMenu.Item onclick={() => orgStore.openSwitcher()}>Switch organization</DropdownMenu.Item>
					{/if}
					{#if orgStore.canLeave}
						<DropdownMenu.Item onclick={leaveOrganization}>Leave organization</DropdownMenu.Item>
					{/if}
				</DropdownMenu.Content>
			</DropdownMenu.Root>
		{/if}
		<UserMenu tone="on-primary" />
	</div>
</header>
