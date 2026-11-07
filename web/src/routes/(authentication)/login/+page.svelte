<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import { gotoLanding, gotoPluginPage, nextSlug } from '$lib/auth/landing';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import CsrfErrorModal from '$lib/components/errors/CsrfErrorModal.svelte';
	import {
		fetchAuthMethods,
		fetchMe,
		CsrfError,
		loginLocal,
		oauthStartUrl,
		ServerUnavailableError,
		type AuthMethods
	} from '$lib/auth/api';
	import { announceSessionChanged } from '$lib/auth/session.svelte';
	import { brandingStore } from '$lib/branding/brandingStore.svelte';

	import ScreenView from '$lib/components/auth/ScreenView.svelte';
	import { themeStore } from '$lib/theme/themeStore.svelte';

	/** The sign-in screen the active theme ships, if any. */
	const screen = $derived(themeStore.theme.shell?.screens?.login ?? null);
	let methods = $state<AuthMethods | null>(null);
	let username = $state('');
	let password = $state('');
	let error = $state('');
	let loading = $state(false);
	let showPassword = $state(false);
	let csrf = $state<CsrfError['reason'] | null>(null);

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
			if (err instanceof CsrfError) csrf = err.reason;
			else error = err instanceof Error ? err.message : 'Login failed';
		} finally {
			loading = false;
		}
	}

	const localEnabled = $derived(methods?.enabled_methods?.includes('local') ?? true);
	const googleEnabled = $derived(methods?.enabled_methods?.includes('google') ?? false);
</script>

<CsrfErrorModal bind:reason={csrf} />

{#snippet loginForm()}
			{#if localEnabled}
				<form class="space-y-5" onsubmit={onSubmit}>
					<div class="space-y-2">
						<Label for="username" class="text-xs text-muted-foreground">Username or email</Label>
						<Input
							id="username"
							autocomplete="username"
							placeholder="you@example.com"
							aria-invalid={error ? true : undefined}
							bind:value={username}
							required
						/>
					</div>
					<div class="space-y-2">
						<Label for="password" class="text-xs text-muted-foreground">Password</Label>
						<div class="relative">
							<Input
								id="password"
								type={showPassword ? 'text' : 'password'}
								autocomplete="current-password"
								class="pr-12"
								aria-invalid={error ? true : undefined}
								bind:value={password}
								required
							/>
							<button
								type="button"
								class="absolute inset-y-0 right-0 grid w-10 place-items-center text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
								aria-label={showPassword ? 'Hide password' : 'Show password'}
								aria-pressed={showPassword}
								onclick={() => (showPassword = !showPassword)}
							>
								{#if showPassword}<AppIcon name="view-off" size={16} />{:else}<AppIcon
										name="view"
										size={16}
									/>{/if}
							</button>
						</div>
					</div>
					{#if error}
						<div
							class="flex items-start gap-2 border-l-4 border-destructive bg-field px-3 py-3 text-sm"
							role="alert"
						>
							<AppIcon name="error-filled" size={16} class="mt-0.5 shrink-0 text-destructive" />
							<span>{error}</span>
						</div>
					{/if}
					<a class="block text-sm underline" href={resolve('/forgot-password')}>Forgot your password?</a>
					<Button type="submit" size="lg" class="w-full" disabled={loading}>
						{loading ? 'Signing in…' : 'Sign in'}
						<AppIcon name="arrow-right" size={16} data-icon="inline-end" />
					</Button>
				</form>
			{/if}
{/snippet}

{#snippet oauthButtons()}
			{#if googleEnabled}
				<div class="space-y-4">
					{#if localEnabled}
						<div
							class="flex items-center gap-3 text-xs tracking-widest text-muted-foreground uppercase"
						>
							<span class="h-px flex-1 bg-border-strong/40"></span>
							or
							<span class="h-px flex-1 bg-border-strong/40"></span>
						</div>
					{/if}
					<Button
						variant="outline"
						size="lg"
						class="w-full"
						href={oauthStartUrl('google')}
						type="button"
					>
						Continue with Google
						<AppIcon name="arrow-right" size={16} data-icon="inline-end" />
					</Button>
				</div>
			{/if}
{/snippet}

{#if screen}
	<ScreenView
		def={screen}
		forms={{ 'login-form': loginForm, 'oauth-buttons': oauthButtons }}
	/>
{:else}

<div class="grid min-h-screen w-full bg-background lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
	<!-- Brand panel: desktop only -->
	<aside
		class="relative hidden flex-col justify-between overflow-hidden bg-[#161616] p-12 text-[#f4f4f4] lg:flex"
		aria-hidden="true"
	>
		<div
			class="absolute inset-0 opacity-[0.07]"
			style="background-image: linear-gradient(#fff 1px, transparent 1px), linear-gradient(90deg, #fff 1px, transparent 1px); background-size: 48px 48px;"
		></div>
		<div
			class="absolute -bottom-40 -left-40 size-[34rem] rounded-full bg-primary opacity-40 blur-[120px]"
		></div>
		<p class="relative text-sm font-semibold tracking-[0.2em] uppercase">
			{brandingStore.appName}
		</p>
		<div class="relative max-w-md space-y-4">
			<h2 class="text-5xl leading-[1.05] font-light tracking-tight">
				{brandingStore.loginHeadline}
			</h2>
			<p class="text-base text-[#c6c6c6]">{brandingStore.loginSubtitle}</p>
		</div>
	</aside>

	<main class="flex items-center justify-center px-6 py-12">
		<div class="w-full max-w-sm space-y-8">
			<div class="space-y-3">
				<p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase lg:hidden">
					{brandingStore.appName}
				</p>
				<h1 class="text-4xl font-light tracking-tight">Sign in</h1>
				<p class="text-sm text-muted-foreground">Use your account credentials to continue.</p>
			</div>

			{@render loginForm()}

			{@render oauthButtons()}

			{#if methods && !localEnabled && !googleEnabled}
				<p class="text-sm text-muted-foreground">No authentication methods are enabled.</p>
			{/if}
		</div>
	</main>
</div>
{/if}
