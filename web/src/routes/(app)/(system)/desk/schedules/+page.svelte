<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import OrgSearchPicker from '$lib/components/org/OrgSearchPicker.svelte';
	import SchedulesPanel from '$lib/components/schedules/SchedulesPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let selectedDb = $state<string | null>(null);

	$effect(() => {
		const current = orgStore.current;
		const orgs = orgStore.organizations;
		if (selectedDb && orgs.some((org) => org.db_name === selectedDb)) return;
		selectedDb =
			current && orgs.some((org) => org.db_name === current)
				? current
				: (orgs[0]?.db_name ?? null);
	});
</script>

<ControlPanel crumbs={[{ label: 'Schedules' }]} />

<div class="flex w-full flex-1 flex-col gap-4 px-4 py-4 md:px-6">
	<div class="flex flex-wrap items-end justify-between gap-3">
		<div class="min-w-0 space-y-1">
			<p class="text-sm text-muted-foreground">
				Recurring tasks and their runs for one organization. Schedules come from plugins installed
				on that org.
			</p>
		</div>
		{#if orgStore.organizations.length > 0}
			<OrgSearchPicker
				organizations={orgStore.organizations}
				bind:value={selectedDb}
				currentDb={orgStore.current}
			/>
		{/if}
	</div>

	{#if !orgStore.loaded}
		<p class="text-sm text-muted-foreground">Loading organizations…</p>
	{:else if !selectedDb}
		<div class="space-y-3 rounded-md border border-dashed border-border px-4 py-8 text-center">
			<p class="text-sm text-muted-foreground">
				Choose an organization to view its schedules. Nothing is shown across orgs mixed together.
			</p>
			{#if orgStore.canSwitch}
				<Button size="sm" variant="outline" onclick={() => orgStore.openSwitcher()}>
					Switch organization
				</Button>
			{/if}
		</div>
	{:else}
		<p class="font-mono text-xs text-muted-foreground">{selectedDb}</p>
		{#key selectedDb}
			<SchedulesPanel db={selectedDb} />
		{/key}
	{/if}
</div>
