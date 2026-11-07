<script lang="ts">
	import { resolve } from '$app/paths';
	import ScreenView from '$lib/components/auth/ScreenView.svelte';
	import { themeStore } from '$lib/theme/themeStore.svelte';

	const screen = $derived(themeStore.theme.shell?.screens?.register ?? null);
</script>

{#snippet notice()}
	<div class="space-y-4">
		<h1 class="text-2xl font-semibold">Registration closed</h1>
		<p class="text-sm text-muted-foreground">
			Self-registration is controlled by
			<code class="text-xs">auth.allow_registration</code> and is off by default.
		</p>
		<a class="text-sm underline" href={resolve('/login')}>Back to sign in</a>
	</div>
{/snippet}

{#if screen}
	<ScreenView def={screen} forms={{ 'registration-notice': notice }} />
{:else}
<div class="flex min-h-screen items-center justify-center px-6">
	<div class="max-w-md space-y-4 text-center">
		<h1 class="text-2xl font-semibold">Registration closed</h1>
		<p class="text-sm text-muted-foreground">
			Self-registration is controlled by
			<code class="text-xs">auth.allow_registration</code> and is off by default.
		</p>
		<a class="text-sm underline" href={resolve('/login')}>Back to sign in</a>
	</div>
</div>
{/if}
