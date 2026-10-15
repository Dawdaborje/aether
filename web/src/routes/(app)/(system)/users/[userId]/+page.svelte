<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import MemberDetail from '$lib/components/org/MemberDetail.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { params }: { params: { userId: string } } = $props();

	const userId = $derived(decodeURIComponent(params.userId));
	const crumbLabel = $derived(userId.replace(/^users:/, '') || 'User');

	$effect(() => {
		if (authSession.isLoading) return;
		if (authSession.isDeveloper) {
			const org = orgStore.current;
			const path = resolve(`/desk/users/${encodeURIComponent(userId)}`);
			if (org) {
				void goto(`${path}?org=${encodeURIComponent(org)}`, { replaceState: true });
			} else {
				void goto(path, { replaceState: true });
			}
			return;
		}
		if (!authSession.canManageUsers) {
			void goto(resolve('/apps'), { replaceState: true });
		}
	});
</script>

{#if authSession.isLoading || authSession.isDeveloper}
	<p class="px-4 py-4 text-sm text-muted-foreground md:px-6">Opening user…</p>
{:else if !authSession.canManageUsers}
	<p class="px-4 py-4 text-sm text-muted-foreground md:px-6">
		You do not manage users in this organization.
	</p>
{:else if !orgStore.current}
	<p class="px-4 py-4 text-sm text-muted-foreground md:px-6">
		Choose an organization first, then open Users again.
	</p>
{:else}
	<div class="flex h-full min-h-0 w-full flex-col">
		<ControlPanel crumbs={[{ label: 'Users', href: '/users' }, { label: crumbLabel }]} />
		<MemberDetail db={orgStore.current} {userId} listHref="/users" />
	</div>
{/if}
