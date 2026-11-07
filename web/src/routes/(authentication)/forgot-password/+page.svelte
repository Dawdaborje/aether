<script lang="ts">
	import { resolve } from '$app/paths';
	import ScreenView from '$lib/components/auth/ScreenView.svelte';
	import { themeStore } from '$lib/theme/themeStore.svelte';

	const screen = $derived(themeStore.theme.shell?.screens?.['forgot-password'] ?? null);
</script>

{#snippet notice()}
	<div class="space-y-4">
		<h1 class="text-2xl font-semibold">Forgot your password?</h1>
		<p class="text-sm text-muted-foreground">
			An administrator of your organization can send you a link to choose a new one. Ask them,
			open the link from them, and you will be signed in once you save it.
		</p>
		<a class="text-sm underline" href={resolve('/login')}>Back to sign in</a>
	</div>
{/snippet}

{#if screen}
	<ScreenView def={screen} forms={{ 'forgot-password-notice': notice }} />
{:else}
	<div class="flex min-h-screen items-center justify-center px-6">
		<div class="max-w-md">{@render notice()}</div>
	</div>
{/if}
