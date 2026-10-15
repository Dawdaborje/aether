<script lang="ts">
	import { onMount } from 'svelte';
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { themeStore } from '$lib/theme';
	import { authSession } from '$lib/auth/session.svelte';
	import { brandingStore } from '$lib/branding/brandingStore.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { children } = $props();

	onMount(() => {
		void themeStore.loadFromApi('');
	});

	// The name belongs to the organization: ask again when someone signs in or out, or
	// switches organization.
	$effect(() => {
		void authSession.isUser;
		void orgStore.current;
		void brandingStore.load();
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>{brandingStore.appName}</title>
</svelte:head>

<div class="h-screen w-full bg-background text-foreground">
	{@render children()}
</div>
