<script lang="ts">
	import { page } from '$app/state';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { authSession } from '$lib/auth/session.svelte';
	import { companyStore } from '$lib/company/companyStore.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import type { ThemeNav } from '$lib/theme/types';
	import { activeApp, activeInstalledApp, navHref } from '../nav';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import HomeMenu from './HomeMenu.svelte';
	import NotificationBell from '$lib/notifications/NotificationBell.svelte';
	import UserMenu from './UserMenu.svelte';

	let { nav }: { nav: ThemeNav } = $props();

	// The member launcher at `/apps` — not `/desk/apps`.
	const onApps = $derived(page.route.id?.startsWith('/(app)/(system)/apps') ?? false);
	const pagePlugin = $derived(
		typeof page.data?.body?.plugin === 'string' ? (page.data.body.plugin as string) : null
	);
	// Theme nav wins when it matches; otherwise the installed `[app]` for this path.
	const themeApp = $derived(activeApp(nav, page.url.pathname));
	const installedApp = $derived(
		themeApp ? null : activeInstalledApp(appsStore.apps, page.url.pathname, pagePlugin)
	);
	const brand = $derived(
		onApps ? 'Apps' : (themeApp?.label ?? installedApp?.label ?? nav.header ?? 'Aether')
	);

	// The home menu lists the installed apps, so make sure they are loaded on every page.
	$effect(() => {
		if (authSession.isUser && appsStore.apps === null) void appsStore.load();
	});
	async function leaveOrganization() {
		await orgStore.leave();
		if (!orgStore.error) await goto(resolve('/desk/organizations'));
	}
</script>

<!--
	Slim top bar: home menu, current app name, systray. App page menus live in the
	left sidebar (see AppNavSidebar), not here.
-->
<header
	class="sticky top-0 z-30 flex h-12 shrink-0 items-center gap-1 border-b border-sidebar-border bg-sidebar px-2 text-sidebar-foreground"
>
	<HomeMenu />

	{#if onApps}
		<span class="mx-1.5 text-[15px] font-semibold tracking-tight text-sidebar-foreground"
			>{brand}</span
		>
	{:else}
		<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
		<a
			href={navHref('/apps')}
			title="Back to apps"
			class="mx-1.5 rounded-md px-1.5 py-0.5 text-[15px] font-semibold tracking-tight text-sidebar-foreground outline-none transition hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring"
		>
			{brand}
		</a>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	{/if}

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
		{#if companyStore.available && companyStore.currentName}
			<DropdownMenu.Root>
				<DropdownMenu.Trigger
					disabled={!companyStore.canSwitch || companyStore.switching}
					aria-label="Company"
					title="Working company"
					class="flex h-8 max-w-44 items-center gap-1.5 rounded-md px-2.5 text-[13px] text-sidebar-foreground/85 outline-none transition enabled:cursor-pointer enabled:hover:bg-sidebar-accent enabled:hover:text-sidebar-accent-foreground disabled:cursor-default data-[state=open]:bg-sidebar-accent"
				>
					<span class="truncate">{companyStore.currentName}</span>
					{#if companyStore.canSwitch}<ChevronDownIcon class="size-3 shrink-0 opacity-60" />{/if}
				</DropdownMenu.Trigger>
				{#if companyStore.canSwitch}
					<DropdownMenu.Content align="end" class="min-w-52">
						{#each companyStore.companies as company (company.id)}
							<DropdownMenu.Item
								disabled={companyStore.switching || company.id === companyStore.current?.id}
								onclick={() => void companyStore.choose(company.id)}
							>
								<span class="flex-1 truncate">{company.name}</span>
								{#if company.is_default}
									<span class="text-xs text-muted-foreground">Default</span>
								{/if}
							</DropdownMenu.Item>
						{/each}
					</DropdownMenu.Content>
				{/if}
			</DropdownMenu.Root>
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
						<DropdownMenu.Item onclick={() => orgStore.openSwitcher()}
							>Switch organization</DropdownMenu.Item
						>
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
