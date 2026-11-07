<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import SchedulesPanel from '$lib/components/schedules/SchedulesPanel.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	// Unrestricted developers use the desk page for the organization; developers of this
	// organization only see its schedules here.
	$effect(() => {
		if (authSession.isLoading) return;
		if (authSession.isDeveloper) {
			const org = orgStore.current;
			void goto(
				resolve(
					org ? `/desk/organizations/${encodeURIComponent(org)}/schedules` : '/desk/schedules'
				),
				{ replaceState: true }
			);
			return;
		}
		if (!authSession.isOrgDeveloper) {
			void goto(resolve('/apps'), { replaceState: true });
		}
	});
</script>

<ControlPanel crumbs={[{ label: 'Schedules' }]} />

<div class="w-full space-y-3 px-4 py-4 md:px-6">
	{#if authSession.isLoading || authSession.isDeveloper}
		<p class="text-sm text-muted-foreground">Opening schedules…</p>
	{:else if !authSession.isOrgDeveloper}
		<p class="text-sm text-muted-foreground">
			Schedules are for developers of this organization.
		</p>
	{:else if !orgStore.current}
		<p class="text-sm text-muted-foreground">
			Choose an organization first, then open Schedules again.
		</p>
	{:else}
		<p class="text-sm text-muted-foreground">
			Recurring tasks from plugins installed on this organization, and their run history.
		</p>
		{#key orgStore.current}
			<SchedulesPanel db={orgStore.current} />
		{/key}
	{/if}
</div>
