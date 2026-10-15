<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { authSession } from '$lib/auth/session.svelte';

	let { children } = $props();

	const routeId = $derived(page.route.id ?? '');

	// `/apps` is the home launcher for every logged-in member.
	// `/profile` is self-service account settings for every signed-in user.
	// `/users` and `/roles` are for org owner / org_admin / user_manager (and developers, who are redirected to desk).
	// `/schedules` is for developers of the current org (unrestricted ones are redirected to desk).
	// `/desk/settings` is for org owner / org_admin (and developers).
	// Everything else under `/desk` is for developers only.
	// Legacy desk URLs (`/organizations`, …) stay reachable so they can 308 to `/desk/…`.
	const onApps = $derived(routeId.startsWith('/(app)/(system)/apps'));
	const onProfile = $derived(
		routeId === '/(app)/(system)/profile' || routeId.startsWith('/(app)/(system)/profile/')
	);
	const onUsers = $derived(
		routeId === '/(app)/(system)/users' || routeId.startsWith('/(app)/(system)/users/')
	);
	const onRoles = $derived(
		routeId === '/(app)/(system)/roles' || routeId.startsWith('/(app)/(system)/roles/')
	);
	const onSchedules = $derived(
		routeId === '/(app)/(system)/schedules' || routeId.startsWith('/(app)/(system)/schedules/')
	);
	const onDeskSettings = $derived(
		routeId === '/(app)/(system)/desk/settings' ||
			routeId.startsWith('/(app)/(system)/desk/settings/')
	);
	const onDesk = $derived(routeId.startsWith('/(app)/(system)/desk'));
	const onLegacyDesk = $derived(
		['/(app)/(system)/organizations', '/(app)/(system)/bridges', '/(app)/(system)/settings', '/(app)/(system)/studio'].some(
			(prefix) => routeId === prefix || routeId.startsWith(`${prefix}/`)
		)
	);
	const allowed = $derived(
		onApps ||
			onProfile ||
			(onUsers && (authSession.canManageUsers || authSession.isDeveloper)) ||
			(onRoles && (authSession.canManageUsers || authSession.isDeveloper)) ||
			(onSchedules && (authSession.isOrgDeveloper || authSession.isDeveloper)) ||
			(onDeskSettings && (authSession.canManageOrg || authSession.isDeveloper)) ||
			(authSession.isDeveloper && (onDesk || onLegacyDesk))
	);

	$effect(() => {
		if (!allowed) void goto(resolve('/apps'));
	});
</script>

{#if allowed}
	{@render children()}
{/if}
