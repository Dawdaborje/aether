<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { themeStore } from '$lib/theme';

	let themes = $state<{ name: string; label: string }[]>([]);
	let loading = $state(true);
	let current = $derived(themeStore.theme.name);

	onMount(async () => {
		try {
			const res = await fetch('/api/ui/themes');
			if (res.ok) {
				const body = await res.json();
				themes = body.themes ?? [];
			}
		} finally {
			loading = false;
		}
	});

	async function selectTheme(_name: string) {
		await themeStore.loadFromApi('');
	}
</script>

<div class="mx-auto max-w-3xl p-8">
	<header class="mb-2 space-y-2 border-b border-border pb-6">
		<p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">Settings</p>
		<h1 class="text-3xl font-semibold tracking-tight">Appearance</h1>
		<p class="text-sm text-muted-foreground">
			Installed themes from the API. Reload applies the active theme tokens for this browser.
		</p>
	</header>

	{#if loading}
		<p class="py-8 text-sm text-muted-foreground">Loading themes…</p>
	{:else if themes.length === 0}
		<p class="py-8 text-sm text-muted-foreground">No themes available from the API.</p>
	{:else}
		<div class="grid gap-3 py-6 sm:grid-cols-2">
			{#each themes as theme (theme.name)}
				<button
					type="button"
					class="flex items-center justify-between border border-border px-4 py-3 text-left transition-colors hover:bg-muted {current ===
					theme.name
						? 'border-primary bg-muted'
						: ''}"
					onclick={() => selectTheme(theme.name)}
				>
					<div>
						<p class="text-sm font-semibold">{theme.label}</p>
						<p class="font-mono text-[11px] text-muted-foreground">{theme.name}</p>
					</div>
					{#if current === theme.name}
						<span class="text-[10px] font-semibold tracking-widest uppercase text-primary"
							>Active</span
						>
					{/if}
				</button>
			{/each}
		</div>
		<Button variant="outline" size="sm" onclick={() => themeStore.loadFromApi('')}>
			Reload theme
		</Button>
	{/if}
</div>
