<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Empty } from '$lib/components/ui/empty';
	import BuildingIcon from '@lucide/svelte/icons/building-2';
	import CreateOrganizationModal from '$lib/org/CreateOrganizationModal.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let creating = $state(false);

	const how = $derived(
		{
			address: 'The address (subdomain or path) decides which organization a request is for.',
			header: 'The browser names the organization on every request (X-Org-Slug).',
			session: 'The session remembers which organization you are working in.'
		}[orgStore.mode]
	);
</script>

<ControlPanel crumbs={[{ label: 'Organizations' }]}>
	{#snippet actions()}
		<Button size="sm" onclick={() => (creating = true)}>Create organization</Button>
	{/snippet}
</ControlPanel>
<div class="w-full space-y-4 px-4 py-4 md:px-6">
	<p class="text-sm text-muted-foreground">{how}</p>

	{#if orgStore.organizations.length === 0}
		<Empty
			icon={BuildingIcon}
			title="No organizations yet"
			description="Create the first one to start installing apps."
		>
			<Button onclick={() => (creating = true)}>Create organization</Button>
		</Empty>
	{:else}
		<Card.Root size="sm" class="py-0">
			<Card.Content class="p-0">
				<ul class="divide-y divide-border">
					{#each orgStore.organizations as org (org.db_name)}
						<li class="flex items-center justify-between gap-4 px-(--card-spacing) py-4">
							<div class="min-w-0 space-y-1">
								<div class="flex flex-wrap items-center gap-2">
									<p class="text-sm font-medium">{org.name}</p>
									<Badge variant={org.member ? 'default' : 'secondary'}>
										{org.member ? 'Member' : 'Developer access'}
									</Badge>
									{#if org.db_name === orgStore.current}
										<Badge variant="outline">Current</Badge>
									{/if}
								</div>
								<p class="font-mono text-xs text-muted-foreground">{org.db_name}</p>
							</div>
							{#if org.db_name !== orgStore.current && orgStore.mode !== 'address'}
								<Button
									variant="outline"
									size="sm"
									disabled={orgStore.switching}
									onclick={async () => {
										await orgStore.choose(org.db_name);
										if (!orgStore.error) await goto(resolve('/apps'));
									}}>Enter</Button
								>
							{/if}
						</li>
					{/each}
				</ul>
			</Card.Content>
		</Card.Root>
	{/if}
</div>

<CreateOrganizationModal bind:open={creating} />
