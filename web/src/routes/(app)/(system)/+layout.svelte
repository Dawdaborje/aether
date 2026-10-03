<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { authSession } from '$lib/auth/session.svelte';

	let { children } = $props();

	// The desk (organizations, settings, developer tools) is for developers.
	// Everyone logged in may open the Apps launcher, which is their home.
	const allowed = $derived(
		authSession.isDeveloper || (page.route.id?.startsWith('/(app)/(system)/apps') ?? false)
	);

	$effect(() => {
		if (!allowed) void goto(resolve('/apps'));
	});
</script>

{#if allowed}
	{@render children()}
{/if}
