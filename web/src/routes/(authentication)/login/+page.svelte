<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { gotoPluginPage, landingPath, nextSlug } from '$lib/auth/landing';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import {
		fetchAuthMethods,
		fetchMe,
		loginLocal,
		oauthStartUrl,
		type AuthMethods
	} from '$lib/auth/api';

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
		await goto(resolve(landingPath(user)));
	}

	onMount(async () => {
		const me = await fetchMe();
		if (me) {
			await afterLogin(me.user);
			return;
		}
		methods = await fetchAuthMethods();
	});

	async function onSubmit(e: Event) {
		e.preventDefault();
		error = '';
		loading = true;
		try {
			const user = await loginLocal(username, password);
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
			<p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">Aether</p>
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
