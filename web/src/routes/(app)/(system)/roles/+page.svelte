<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import RolesManager from '$lib/components/org/RolesManager.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	$effect(() => {
		if (authSession.isLoading) return;
		if (authSession.isDeveloper) {
			const org = orgStore.current;
			if (org) {
				void goto(resolve(`/desk/organizations/${encodeURIComponent(org)}/roles`), {
					replaceState: true
				});
			} else {
				void goto(resolve('/desk/organizations'), { replaceState: true });
			}
			return;
		}
		if (!authSession.canManageUsers) {
			void goto(resolve('/apps'), { replaceState: true });
		}
	});
</script>

<ControlPanel crumbs={[{ label: 'Roles' }]} />

<div class="w-full space-y-4 px-4 py-4 md:px-6">
	{#if authSession.isLoading || authSession.isDeveloper}
		<p class="text-sm text-muted-foreground">Opening roles…</p>
	{:else if !authSession.canManageUsers}
		<p class="text-sm text-muted-foreground">You do not manage roles in this organization.</p>
	{:else if !orgStore.current}
		<p class="text-sm text-muted-foreground">
			Choose an organization first, then open Roles again.
		</p>
	{:else}
		<RolesManager db={orgStore.current} title="Roles" detailMode="org" />
	{/if}
</div>
