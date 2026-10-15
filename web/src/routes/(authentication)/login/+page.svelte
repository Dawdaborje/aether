<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { gotoLanding, gotoPluginPage, nextSlug } from '$lib/auth/landing';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import {
		fetchAuthMethods,
		fetchMe,
		loginLocal,
		oauthStartUrl,
		ServerUnavailableError,
		type AuthMethods
	} from '$lib/auth/api';
	import { announceSessionChanged } from '$lib/auth/session.svelte';
	import { brandingStore } from '$lib/branding/brandingStore.svelte';

	let methods = $state<AuthMethods | null>(null);
	let username = $state('');
	let password = $state('');
	let error = $state('');
	let loading = $state(false);

	/** After login: the page they were trying to open, else the landing area for their role. */
	async function afterLogin(user: { is_developer: boolean }) {
		const slug = nextSlug(page.url.searchParams.get('next'));
		if (slug !== null) {
			await gotoPluginPage(slug);
			return;
		}
		await gotoLanding(user);
	}

	onMount(async () => {
		// An outage is not "logged out": keep the form so sign-in can report its own error.
		try {
			const me = await fetchMe();
			if (me) {
				await afterLogin(me.user);
				return;
			}
		} catch (err) {
			if (!(err instanceof ServerUnavailableError)) throw err;
		}
		methods = await fetchAuthMethods();
	});

	async function onSubmit(e: Event) {
		e.preventDefault();
		error = '';
		loading = true;
		try {
			const user = await loginLocal(username, password);
			// Other tabs share this cookie; make them drop stale in-memory identity.
			announceSessionChanged();
			await afterLogin(user);
		} catch (err) {
			error = err instanceof Error ? err.message : 'Login failed';
		} finally {
			loading = false;
		}
	}

	const localEnabled = $derived(methods?.enabled_methods?.includes('local') ?? true);
	const googleEnabled = $derived(methods?.enabled_methods?.includes('google') ?? false);
</script>

<div class="flex min-h-screen w-full items-center justify-center bg-background px-6">
	<div class="w-full max-w-md space-y-8">
		<div class="space-y-2">
			<p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">
				{brandingStore.appName}
			</p>
			<h1 class="text-3xl font-semibold tracking-tight">Sign in</h1>
			<p class="text-sm text-muted-foreground">
				Use your account credentials to access the desk.
			</p>
		</div>

		{#if localEnabled}
			<form class="space-y-4" onsubmit={onSubmit}>
				<div class="space-y-2">
					<label class="text-xs font-semibold tracking-widest uppercase" for="username"
						>Username or email</label
					>
					<Input id="username" autocomplete="username" bind:value={username} required />
				</div>
				<div class="space-y-2">
					<label class="text-xs font-semibold tracking-widest uppercase" for="password"
						>Password</label
					>
					<Input
						id="password"
						type="password"
						autocomplete="current-password"
						bind:value={password}
						required
					/>
				</div>
				{#if error}
					<p class="text-sm text-destructive">{error}</p>
				{/if}
				<Button type="submit" class="w-full" disabled={loading}>
					{loading ? 'Signing in…' : 'Sign in'}
				</Button>
			</form>
		{/if}

		{#if googleEnabled}
			<div class="space-y-3">
				{#if localEnabled}
					<p class="text-center text-xs tracking-widest text-muted-foreground uppercase">or</p>
				{/if}
				<Button
					variant="outline"
					class="w-full"
					href={oauthStartUrl('google')}
					type="button"
				>
					Continue with Google
				</Button>
			</div>
		{/if}

		{#if methods && !localEnabled && !googleEnabled}
			<p class="text-sm text-muted-foreground">No authentication methods are enabled.</p>
		{/if}
	</div>
</div>
