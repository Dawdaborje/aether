<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import RoleDetail from '$lib/components/org/RoleDetail.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { params }: { params: { role: string } } = $props();

	const roleName = $derived(decodeURIComponent(params.role));
	const crumbLabel = $derived(roleName);

	$effect(() => {
		if (authSession.isLoading) return;
		if (authSession.isDeveloper) {
			const org = orgStore.current;
			if (org) {
				void goto(
					resolve(
						`/desk/organizations/${encodeURIComponent(org)}/roles/${encodeURIComponent(roleName)}`
					),
					{ replaceState: true }
				);
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

<ControlPanel crumbs={[{ label: 'Roles', href: '/roles' }, { label: crumbLabel }]} />

<div class="w-full space-y-4 px-4 py-4 md:px-6">
	{#if authSession.isLoading || authSession.isDeveloper}
		<p class="text-sm text-muted-foreground">Opening role…</p>
	{:else if !authSession.canManageUsers}
		<p class="text-sm text-muted-foreground">You do not manage roles in this organization.</p>
	{:else if !orgStore.current}
		<p class="text-sm text-muted-foreground">
			Choose an organization first, then open Roles again.
		</p>
	{:else}
		<RoleDetail db={orgStore.current} {roleName} listHref="/roles" />
	{/if}
</div>
