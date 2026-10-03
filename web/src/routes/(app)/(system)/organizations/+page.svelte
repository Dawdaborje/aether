<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
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
		<div class="space-y-2 rounded-lg border border-dashed border-border p-8 text-center">
			<p class="font-medium">No organizations yet</p>
			<p class="text-sm text-muted-foreground">Create the first one to start installing apps.</p>
			<Button onclick={() => (creating = true)}>Create organization</Button>
		</div>
	{:else}
		<ul class="divide-y divide-border rounded-md border border-border bg-card shadow-xs">
			{#each orgStore.organizations as org (org.db_name)}
				<li class="flex items-center justify-between gap-4 p-4">
					<div>
						<div class="flex items-center gap-2">
							<p class="text-sm font-medium">{org.name}</p>
							<span
								class="rounded-sm px-1.5 py-0.5 text-[10px] font-semibold tracking-wider uppercase {org.member
									? 'bg-primary/10 text-primary'
									: 'bg-muted text-muted-foreground'}"
							>
								{org.member ? 'Member' : 'Developer access'}
							</span>
						</div>
						<p class="font-mono text-xs text-muted-foreground">{org.db_name}</p>
					</div>
					{#if org.db_name === orgStore.current}
						<span class="text-xs tracking-wide text-muted-foreground uppercase">Current</span>
					{:else if orgStore.mode !== 'address'}
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
	{/if}
</div>

<CreateOrganizationModal bind:open={creating} />
