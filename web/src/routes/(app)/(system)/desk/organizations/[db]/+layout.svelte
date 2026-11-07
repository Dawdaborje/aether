<script lang="ts">
	import { DetailSkeleton } from '$lib/components/ui/skeleton';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { errorMessage, loadFailure } from '$lib/errors/report';
	import { page } from '$app/stores';
	import { resolve } from '$app/paths';
	import { orgCrumbs } from '$lib/components/layout/crumbs';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import OrgLogo from '$lib/components/org/OrgLogo.svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { fetchOrganization, type OrganizationProfile } from '$lib/org/api';
	import { setOrgDetailContext } from '$lib/org/orgDetailContext';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { onMount } from 'svelte';

	let { children, params }: { children: import('svelte').Snippet; params: { db: string } } =
		$props();

	let organization = $state<OrganizationProfile | null>(null);
	let failed = $state<string | null>(null);
	let loading = $state(true);

	const tabs = $derived(
		[
			{ id: 'overview', label: 'Overview', href: `/desk/organizations/${params.db}/overview` },
			{ id: 'members', label: 'Members', href: `/desk/organizations/${params.db}/members` },
			{ id: 'roles', label: 'Roles', href: `/desk/organizations/${params.db}/roles` },
			{
				id: 'schedules',
				label: 'Schedules',
				href: `/desk/organizations/${params.db}/schedules`
			},
			{ id: 'plugins', label: 'Plugins', href: `/desk/organizations/${params.db}/plugins` },
			{ id: 'audit', label: 'Audit log', href: `/desk/organizations/${params.db}/audit` },
			{
				id: 'plugin-activity',
				label: 'Plugin activity',
				href: `/desk/organizations/${params.db}/plugin-activity`
			}
		]
	);

	const activeTab = $derived.by(() => {
		const path = $page.url.pathname;
		if (path.includes('/members')) return 'members';
		if (path.includes('/roles')) return 'roles';
		if (path.includes('/schedules')) return 'schedules';
		if (path.includes('/plugin-activity')) return 'plugin-activity';
		if (path.includes('/audit')) return 'audit';
		if (path.includes('/plugins')) return 'plugins';
		return 'overview';
	});

	async function reload() {
		loading = true;
		failed = null;
		try {
			organization = await fetchOrganization(params.db);
		} catch (error) {
			failed = loadFailure(error, 'Organization could not be loaded.');
			organization = null;
		} finally {
			loading = false;
		}
	}

	setOrgDetailContext({
		get organization() {
			return organization;
		},
		reload,
		setOrganization(next) {
			organization = next;
		}
	});

	onMount(reload);

	let lastDb = params.db;
	$effect(() => {
		if (params.db === lastDb) return;
		lastDb = params.db;
		void reload();
	});
</script>

<ControlPanel
	crumbs={orgCrumbs({
		single: orgStore.isSingle,
		name: organization?.name ?? params.db,
		db: params.db,
		pathname: $page.url.pathname
	})}
/>

<div class="flex min-h-0 w-full flex-1 flex-col gap-4 px-4 py-4 md:px-6">
	{#if loading && !organization}
		<DetailSkeleton fields={2} label="Loading organization…" />
	{:else if failed && !organization}
		<LoadFailed message={failed} onRetry={reload} retrying={loading} />
	{:else if organization}
		<header class="flex shrink-0 flex-wrap items-start gap-4">
			<OrgLogo
				logoUrl={organization.logo_url}
				name={organization.name}
				cacheKey={organization.date_updated}
				class="size-16 rounded-xl"
			/>
			<div class="min-w-0 flex-1 space-y-1">
				<div class="flex flex-wrap items-center gap-2">
					<h1 class="text-xl font-semibold tracking-tight">{organization.name}</h1>
					{#if organization.db_name === orgStore.current}
						<Badge variant="outline">Current</Badge>
					{/if}
				</div>
				<p class="font-mono text-xs text-muted-foreground">{organization.db_name}</p>
				{#if organization.description}
					<p class="max-w-2xl text-sm text-muted-foreground">{organization.description}</p>
				{/if}
			</div>
		</header>

		<nav
			class="flex shrink-0 overflow-x-auto border-b border-border"
			aria-label="Organization sections"
		>
			<!-- Tabs share the bar's width evenly; a narrow screen scrolls the bar instead of wrapping it. -->
			{#each tabs as tab (tab.id)}
				<a
					href={resolve(tab.href)}
					class="flex-1 border-b-2 px-3 py-2 text-center text-sm whitespace-nowrap transition-colors {activeTab ===
					tab.id
						? 'border-primary font-medium text-foreground'
						: 'border-transparent text-muted-foreground hover:text-foreground'}"
					aria-current={activeTab === tab.id ? 'page' : undefined}
				>
					{tab.label}
				</a>
			{/each}
		</nav>

		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			{@render children()}
		</div>
	{/if}
</div>
