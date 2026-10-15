<script lang="ts">
	import { navigating, page } from '$app/state';
	import AppShell from '$lib/components/layout/AppShell.svelte';
	import OrgSwitcherModal from '$lib/components/layout/OrgSwitcherModal.svelte';
	import BootstrapLoading from '$lib/components/errors/BootstrapLoading.svelte';
	import ServerUnavailable from '$lib/components/errors/ServerUnavailable.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { gotoLogin } from '$lib/auth/landing';
	import { ServerUnavailableError } from '$lib/auth/api';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { companyStore } from '$lib/company/companyStore.svelte';
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
	/** Bootstrap: fetch to the API failed (process down, proxy refused, DB outage). */
	let unavailable = $state(false);
	let attempt = $state(0);
	const RETRY_MS = 3000;
	/** Avoid a flash on instant navigations; page loads often take seconds. */
	const NAV_BANNER_DELAY_MS = 120;
	/** Brief stream blips should not flash a banner; sustained outage should. */
	const OUTAGE_BANNER_DELAY_MS = 1500;
	// `$app/state`'s `navigating` is always an object; idle is `to === null`.
	const isNavigating = $derived(navigating.to !== null);
	let showNavBanner = $state(false);
	let showOutageBanner = $state(false);

	$effect(() => {
		if (!isNavigating) {
			showNavBanner = false;
			return;
		}
		const timer = setTimeout(() => {
			showNavBanner = true;
		}, NAV_BANNER_DELAY_MS);
		return () => clearTimeout(timer);
	});

	$effect(() => {
		if (!ready || !notificationsStore.unreachable) {
			showOutageBanner = false;
			return;
		}
		const timer = setTimeout(() => {
			showOutageBanner = true;
		}, OUTAGE_BANNER_DELAY_MS);
		return () => clearTimeout(timer);
	});

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
				attempt = 0;
				break;
			} catch (err) {
				// Only an outage is retried; anything else is a real bug.
				if (!(err instanceof ServerUnavailableError)) throw err;
				unavailable = true;
				attempt += 1;
				await new Promise((resolve) => setTimeout(resolve, RETRY_MS));
			}
		}
		if (authSession.isUser || isPluginPage) {
			// The theme was requested before the organization was known (the first
			// visit of a header-tenancy deployment): ask again now that it is.
			if (authSession.isUser && !hadOrganization && orgStore.current) {
				await themeStore.loadFromApi('');
			}
			if (authSession.isUser) {
				void appsStore.load();
				if (orgStore.current) void companyStore.load();
			}
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
		{#if unavailable}
			<ServerUnavailable retrying {attempt} />
		{:else}
			<BootstrapLoading />
		{/if}
	{:else}
		{#if showOutageBanner}
			<ServerUnavailable
				variant="banner"
				retrying
				onRetry={() => notificationsStore.restart()}
			/>
		{/if}
		{#if showNavBanner}
			<div
				class="pointer-events-none fixed inset-x-0 top-0 z-50 h-1 w-full overflow-hidden bg-primary/10"
				role="progressbar"
				aria-busy="true"
				aria-label="Loading page"
			>
				<div
					class="h-full w-1/3 animate-[loading_1.1s_ease-in-out_infinite] rounded-full bg-primary/60"
				></div>
			</div>
		{/if}
		<AppShell>
			{@render children()}
		</AppShell>
		{#if authSession.isUser}
			<OrgSwitcherModal />
		{/if}
	{/if}
</div>
