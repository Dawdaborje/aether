<script lang="ts">
	import { page } from '$app/stores';
	import { resolve } from '$app/paths';
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

	const tabs = $derived([
		{ id: 'overview', label: 'Overview', href: `/desk/organizations/${params.db}/overview` },
		{ id: 'members', label: 'Members', href: `/desk/organizations/${params.db}/members` },
		{ id: 'roles', label: 'Roles', href: `/desk/organizations/${params.db}/roles` },
		{
			id: 'ownership',
			label: 'Ownership',
			href: `/desk/organizations/${params.db}/ownership`
		},
		{
			id: 'schedules',
			label: 'Schedules',
			href: `/desk/organizations/${params.db}/schedules`
		}
	]);

	const activeTab = $derived.by(() => {
		const path = $page.url.pathname;
		if (path.includes('/members')) return 'members';
		if (path.includes('/roles')) return 'roles';
		if (path.includes('/ownership')) return 'ownership';
		if (path.includes('/schedules')) return 'schedules';
		return 'overview';
	});

	async function reload() {
		loading = true;
		failed = null;
		try {
			organization = await fetchOrganization(params.db);
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Organization could not be loaded.';
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
	crumbs={[
		{ label: 'Organizations', href: '/desk/organizations' },
		{ label: organization?.name ?? params.db }
	]}
/>

<div class="w-full space-y-4 px-4 py-4 md:px-6">
	{#if loading && !organization}
		<p class="text-sm text-muted-foreground">Loading organization…</p>
	{:else if failed && !organization}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{:else if organization}
		<header class="flex flex-wrap items-start gap-4">
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

		<nav class="flex flex-wrap gap-1 border-b border-border" aria-label="Organization sections">
			{#each tabs as tab (tab.id)}
				<a
					href={resolve(tab.href)}
					class="border-b-2 px-3 py-2 text-sm transition-colors {activeTab === tab.id
						? 'border-primary font-medium text-foreground'
						: 'border-transparent text-muted-foreground hover:text-foreground'}"
					aria-current={activeTab === tab.id ? 'page' : undefined}
				>
					{tab.label}
				</a>
			{/each}
		</nav>

		<div class="min-w-0">
			{@render children()}
		</div>
	{/if}
</div>
