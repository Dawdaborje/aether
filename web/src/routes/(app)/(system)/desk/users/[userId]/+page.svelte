<script lang="ts">
	import { reportError } from '$lib/errors/report';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import MemberDetail from '$lib/components/org/MemberDetail.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { fetchPlatformUsers } from '$lib/org/api';

	let { params }: { params: { userId: string } } = $props();

	const userId = $derived(decodeURIComponent(params.userId));
	const orgFromQuery = $derived(page.url.searchParams.get('org')?.trim() || '');

	let orgDb = $state<string | null>(null);
	let resolveFailed = $state<string | null>(null);

	$effect(() => {
		if (authSession.isLoading) return;
		if (!authSession.isDeveloper) {
			void goto(resolve('/apps'), { replaceState: true });
		}
	});

	$effect(() => {
		const id = userId;
		const preferred = orgFromQuery;
		let cancelled = false;
		resolveFailed = null;
		if (preferred) {
			orgDb = preferred;
			return;
		}
		orgDb = null;
		void fetchPlatformUsers()
			.then((users) => {
				if (cancelled) return;
				const user = users.find((row) => row.user_id === id);
				const first = user?.organizations[0];
				if (!first) {
					resolveFailed = 'This account is not in any organization.';
					return;
				}
				orgDb = first;
			})
			.catch((error) => {
				if (cancelled) return;
				resolveFailed = reportError(error, 'Could not resolve the user’s organization.');
			});
		return () => {
			cancelled = true;
		};
	});

	const crumbLabel = $derived(userId.replace(/^users:/, '') || 'User');
</script>

{#if authSession.isDeveloper}
	<div class="flex h-full min-h-0 w-full flex-col">
		<ControlPanel
			crumbs={[
				{ label: 'Users', href: '/desk/users' },
				{ label: crumbLabel }
			]}
		/>

		{#if resolveFailed}
			<p class="px-4 py-4 text-sm text-destructive md:px-6" role="alert">{resolveFailed}</p>
		{:else if !orgDb}
			<p class="px-4 py-4 text-sm text-muted-foreground md:px-6">Opening user…</p>
		{:else}
			<MemberDetail db={orgDb} {userId} listHref="/desk/users" />
		{/if}
	</div>
{:else}
	<p class="px-4 py-4 text-sm text-muted-foreground md:px-6">Developers only.</p>
{/if}
