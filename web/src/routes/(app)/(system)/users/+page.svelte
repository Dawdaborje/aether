<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import MembersManager from '$lib/components/org/MembersManager.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	$effect(() => {
		if (authSession.isLoading) return;
		if (authSession.isDeveloper) {
			void goto(resolve('/desk/users'), { replaceState: true });
			return;
		}
		if (!authSession.canManageUsers) {
			void goto(resolve('/apps'), { replaceState: true });
		}
	});
</script>

<ControlPanel crumbs={[{ label: 'Users' }]} />

<div class="w-full space-y-4 px-4 py-4 md:px-6">
	{#if authSession.isLoading || authSession.isDeveloper}
		<p class="text-sm text-muted-foreground">Opening users…</p>
	{:else if !authSession.canManageUsers}
		<p class="text-sm text-muted-foreground">You do not manage users in this organization.</p>
	{:else if !orgStore.current}
		<p class="text-sm text-muted-foreground">
			Choose an organization first, then open Users again.
		</p>
	{:else}
		<MembersManager db={orgStore.current} title="Users" detailMode="org" />
	{/if}
</div>
