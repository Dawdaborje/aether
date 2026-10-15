<script lang="ts">
	import { onMount } from 'svelte';
	import BuildingIcon from '@lucide/svelte/icons/building-2';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Empty } from '$lib/components/ui/empty';
	import { Input } from '$lib/components/ui/input';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import OrgLogo from '$lib/components/org/OrgLogo.svelte';
	import CreateOrganizationModal from '$lib/org/CreateOrganizationModal.svelte';
	import { fetchOrganizations, type OrganizationProfile } from '$lib/org/api';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let creating = $state(false);
	let organizations = $state<OrganizationProfile[] | null>(null);
	let failed = $state<string | null>(null);
	let query = $state('');

	const how = $derived(
		{
			address: 'The address (subdomain or path) decides which organization a request is for.',
			header: 'The browser names the organization on every request (X-Org-Slug).',
			session: 'The session remembers which organization you are working in.'
		}[orgStore.mode]
	);

	onMount(async () => {
		await reload();
	});

	async function reload() {
		failed = null;
		try {
			organizations = await fetchOrganizations();
			await orgStore.load();
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Organizations could not be loaded.';
			organizations = [];
		}
	}

	const shown = $derived(
		(organizations ?? []).filter((org) => {
			const hay = `${org.name} ${org.db_name} ${org.description ?? ''} ${org.parent_name ?? ''} ${org.parent_db_name ?? ''} ${org.owner_login ?? ''}`;
			return hay.toLowerCase().includes(query.trim().toLowerCase());
		})
	);

	function membership(org: OrganizationProfile) {
		return orgStore.organizations.find((row) => row.db_name === org.db_name);
	}
</script>

<ControlPanel crumbs={[{ label: 'Organizations' }]}>
	{#snippet actions()}
		<Button size="sm" onclick={() => (creating = true)}>Create organization</Button>
	{/snippet}
</ControlPanel>
<div class="w-full space-y-4 px-4 py-4 md:px-6">
	<div class="flex flex-wrap items-center gap-3">
		<p class="min-w-0 flex-1 text-sm text-muted-foreground">{how}</p>
		{#if organizations && organizations.length > 0}
			<div class="relative w-full max-w-xs">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 z-10 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					bind:value={query}
					type="search"
					placeholder="Search organizations…"
					aria-label="Search organizations"
					class="pl-9"
				/>
			</div>
		{/if}
	</div>

	{#if organizations === null}
		<div class="space-y-2">
			{#each Array(4) as _, i (i)}
				<Skeleton class="h-16 w-full rounded-lg" />
			{/each}
		</div>
	{:else if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{:else if organizations.length === 0}
		<Empty
			icon={BuildingIcon}
			title="No organizations yet"
			description="Create the first one to start installing apps."
		>
			<Button onclick={() => (creating = true)}>Create organization</Button>
		</Empty>
	{:else if shown.length === 0}
		<Empty
			icon={SearchIcon}
			title="No matching organizations"
			description="Try a different search."
		/>
	{:else}
		<Card.Root size="sm" class="py-0">
			<Card.Content class="p-0">
				<ul class="divide-y divide-border">
					{#each shown as org (org.db_name)}
						{@const member = membership(org)}
						<li class="flex flex-wrap items-center justify-between gap-4 px-(--card-spacing) py-4">
							<div class="flex min-w-0 flex-1 items-start gap-3">
								<OrgLogo
									logoUrl={org.logo_url}
									name={org.name}
									cacheKey={org.date_updated}
									class="size-11 rounded-lg"
								/>
								<div class="min-w-0 space-y-1">
									<div class="flex flex-wrap items-center gap-2">
										<p class="text-sm font-medium">{org.name}</p>
										{#if member}
											<Badge variant={member.member ? 'default' : 'secondary'}>
												{member.member ? 'Member' : 'Developer access'}
											</Badge>
										{/if}
										{#if org.db_name === orgStore.current}
											<Badge variant="outline">Current</Badge>
										{/if}
									</div>
									<p class="font-mono text-xs text-muted-foreground">{org.db_name}</p>
									{#if org.description}
										<p class="line-clamp-1 text-xs text-muted-foreground">{org.description}</p>
									{/if}
									{#if org.parent_name || org.parent_db_name}
										<p class="text-xs text-muted-foreground">
											Parent: {org.parent_name ?? org.parent_db_name}
										</p>
									{/if}
								</div>
							</div>
							<div class="flex shrink-0 flex-wrap items-center gap-2">
								<Button
									variant="outline"
									size="sm"
									href={resolve(`/desk/organizations/${org.db_name}/overview`)}
								>
									Open
								</Button>
								{#if org.db_name !== orgStore.current && orgStore.mode !== 'address'}
									<Button
										variant="secondary"
										size="sm"
										disabled={orgStore.switching}
										onclick={async () => {
											await orgStore.choose(org.db_name);
											if (!orgStore.error) await goto(resolve('/apps'));
										}}>Enter</Button
									>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			</Card.Content>
		</Card.Root>
	{/if}
</div>

<CreateOrganizationModal
	bind:open={creating}
	parents={organizations ?? []}
	onCreated={reload}
/>
