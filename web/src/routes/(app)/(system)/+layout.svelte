<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { authSession } from '$lib/auth/session.svelte';

	let { children } = $props();

	const routeId = $derived(page.route.id ?? '');

	// `/apps` is the home launcher for every logged-in member.
	// Everything under `/desk` is for developers only.
	// Legacy desk URLs (`/organizations`, …) stay reachable so they can 308 to `/desk/…`.
	const onApps = $derived(routeId.startsWith('/(app)/(system)/apps'));
	const onDesk = $derived(routeId.startsWith('/(app)/(system)/desk'));
	const onLegacyDesk = $derived(
		['/(app)/(system)/organizations', '/(app)/(system)/bridges', '/(app)/(system)/settings', '/(app)/(system)/studio'].some(
			(prefix) => routeId === prefix || routeId.startsWith(`${prefix}/`)
		)
	);
	const allowed = $derived(onApps || (authSession.isDeveloper && (onDesk || onLegacyDesk)));

	$effect(() => {
		if (!allowed) void goto(resolve('/apps'));
	});
</script>

{#if allowed}
	{@render children()}
{/if}
