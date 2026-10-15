<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { consumeMagicLink } from '$lib/auth/api';
	import { announceSessionChanged, authSession } from '$lib/auth/session.svelte';

	const token = $derived(page.url.searchParams.get('token')?.trim() ?? '');

	let password = $state('');
	let confirm = $state('');
	let working = $state(false);
	let error = $state<string | null>(null);

	async function submit(event: Event) {
		event.preventDefault();
		if (!token) {
			error = 'This page needs a token from your email link.';
			return;
		}
		if (password.length < 8) {
			error = 'Use at least 8 characters.';
			return;
		}
		if (password !== confirm) {
			error = 'The passwords do not match.';
			return;
		}
		working = true;
		error = null;
		try {
			await consumeMagicLink(token, password);
			await authSession.load();
			announceSessionChanged();
			await goto(resolve('/apps'), { replaceState: true });
		} catch (err) {
			error = err instanceof Error ? err.message : 'Could not set the password.';
		} finally {
			working = false;
		}
	}
</script>

<div class="mx-auto flex min-h-[70vh] w-full max-w-md flex-col justify-center px-4 py-10">
	<h1 class="text-2xl font-semibold tracking-tight">Set your password</h1>
	<p class="mt-2 text-sm text-muted-foreground">
		Open the link from your email, then choose a password. This is how a new account is
		created and how an existing password is reset.
	</p>

	{#if !token}
		<p class="mt-6 text-sm text-destructive" role="alert">Missing token. Use the link from your email.</p>
	{:else}
		<form class="mt-6 space-y-4" onsubmit={submit}>
			{#if error}
				<p class="text-sm text-destructive" role="alert">{error}</p>
			{/if}
			<label class="block space-y-1 text-sm">
				<span class="text-muted-foreground">New password</span>
				<Input type="password" autocomplete="new-password" bind:value={password} />
			</label>
			<label class="block space-y-1 text-sm">
				<span class="text-muted-foreground">Confirm password</span>
				<Input type="password" autocomplete="new-password" bind:value={confirm} />
			</label>
			<Button type="submit" disabled={working} class="w-full">Save password and sign in</Button>
		</form>
	{/if}
</div>
