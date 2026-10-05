<script lang="ts">
	import { page } from '$app/state';
	import AppShell from '$lib/components/layout/AppShell.svelte';
	import OrgSwitcherModal from '$lib/components/layout/OrgSwitcherModal.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { gotoLogin } from '$lib/auth/landing';
	import { ServerUnavailableError } from '$lib/auth/api';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { themeStore } from '$lib/theme';
	import { onDestroy, onMount } from 'svelte';
	import NotificationToasts from '$lib/notifications/NotificationToasts.svelte';
	import { notificationsStore } from '$lib/notifications/notificationsStore.svelte';

	let { children } = $props();

	// Plugin pages are served by the catch-all route. An anonymous browser may
	// open one; the page API answers 401 (log in), 403 or 404 itself, so only
	// the rest of the app needs a login up front.
	const isPluginPage = $derived(page.route.id === '/(app)/(org)/[...slug]');

	let ready = $state(false);
	/** The server is up but cannot answer (usually its database is down); we keep retrying. */
	let unavailable = $state(false);
	const RETRY_MS = 3000;

	onDestroy(() => notificationsStore.stop());

	// Notifications belong to an organization: switching starts a fresh stream.
	let streamOrg = orgStore.current;
	$effect(() => {
		const current = orgStore.current;
		if (ready && current !== streamOrg) {
			streamOrg = current;
			notificationsStore.restart();
		}
	});

	onMount(async () => {
		// Who the user is, which organizations they have and the organization's
		// theme do not depend on each other, so they are fetched together: the page
		// waits for the slowest of them, not for the sum.
		const hadOrganization = orgStore.current !== null;
		for (;;) {
			try {
				await Promise.all([
					authSession.load(),
					orgStore.load().catch(() => undefined),
					themeStore.loadFromApi('')
				]);
				unavailable = false;
				break;
			} catch (err) {
				// Only an outage is retried; anything else is a real bug.
				if (!(err instanceof ServerUnavailableError)) throw err;
				unavailable = true;
				await new Promise((resolve) => setTimeout(resolve, RETRY_MS));
			}
		}
		if (authSession.isUser || isPluginPage) {
			// The theme was requested before the organization was known (the first
			// visit of a header-tenancy deployment): ask again now that it is.
			if (authSession.isUser && !hadOrganization && orgStore.current) {
				await themeStore.loadFromApi('');
			}
			if (authSession.isUser) void appsStore.load();
			notificationsStore.start();
			ready = true;
			return;
		}
		void gotoLogin(page.url.pathname.replace(/^\/web/, '') || '/');
	});
</script>

<div class="flex w-full flex-col">
	<NotificationToasts />
	{#if !ready}
		<!-- Chrome first, content when it is known: never a blank screen. -->
		<div class="flex min-h-screen flex-col bg-canvas" aria-busy="true">
			<div class="h-12 border-b border-black/30 bg-sidebar"></div>
			<div class="h-1 w-full overflow-hidden bg-primary/10">
				<div class="h-full w-1/3 animate-[loading_1.1s_ease-in-out_infinite] rounded-full bg-primary/60"></div>
			</div>
			{#if unavailable}
				<p class="p-6 text-center text-sm text-muted-foreground" role="status">
					The server can't reach its database right now. Retrying…
				</p>
			{/if}
		</div>
	{:else}
		<AppShell>
			{@render children()}
		</AppShell>
		{#if authSession.isUser}
			<OrgSwitcherModal />
		{/if}
	{/if}
</div>
