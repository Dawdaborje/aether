<script lang="ts">
	import { DetailSkeleton } from '$lib/components/ui/skeleton';
	import { reportError } from '$lib/errors/report';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { apiFetch } from '$lib/api/client';
	import { authSession } from '$lib/auth/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import ThemePreview from '$lib/components/settings/ThemePreview.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { defaultTheme, themeStore } from '$lib/theme';
	import { onMount } from 'svelte';

	interface ThemeEntry {
		name: string;
		label: string;
		layout: string;
		preview: { background: string; foreground: string; primary: string; sidebar: string };
	}

	let themes = $state<ThemeEntry[]>([]);
	/** The organization's active theme; null means the built-in default. */
	let active = $state<string | null>(null);
	let loading = $state(true);
	let busy = $state<string | null>(null);
	let error = $state('');

	const builtIn = defaultTheme.tokens.light;

	/** The built-in default and the installed themes, with the one in use first. */
	const ordered = $derived.by(() => {
		const entries: {
			name: string | null;
			label: string;
			layout: string;
			preview: ThemeEntry['preview'];
		}[] = [
			{
				name: null,
				label: 'Enterprise',
				layout: 'default',
				preview: {
					background: builtIn.background,
					foreground: builtIn.foreground,
					primary: builtIn.primary,
					sidebar: builtIn.sidebar
				}
			},
			...themes
		];
		return [
			...entries.filter((e) => e.name === active),
			...entries.filter((e) => e.name !== active)
		];
	});

	async function load() {
		const res = await apiFetch('/api/ui/themes');
		if (!res.ok) throw new Error('The themes could not be loaded.');
		const body = await res.json();
		themes = body.themes ?? [];
		active = body.active ?? null;
	}

	// Picking the organization's theme is for developers; anyone else is sent back to Settings.
	$effect(() => {
		if (!authSession.isLoading && !authSession.isDeveloper) {
			void goto(resolve('/desk/settings'), { replaceState: true });
		}
	});

	// Reload when the organization changes.
	$effect(() => {
		void orgStore.current;
		if (orgStore.current) void load().catch(() => {});
	});

	onMount(async () => {
		try {
			await load();
		} catch (err) {
			error = reportError(err, 'The themes could not be loaded.');
		} finally {
			loading = false;
		}
	});

	/** Make `name` the organization's theme; `null` returns to the built-in default. */
	async function activate(name: string | null) {
		busy = name ?? '__default__';
		error = '';
		try {
			const res = await apiFetch('/api/ui/themes/active', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ name })
			});
			if (!res.ok) {
				const body = await res.json().catch(() => ({}));
				throw new Error(body.error ?? 'The theme could not be changed.');
			}
			await Promise.all([load(), themeStore.loadFromApi('')]);
		} catch (err) {
			error = reportError(err, 'The theme could not be changed.');
		} finally {
			busy = null;
		}
	}
</script>

<section class="space-y-5">
	<div class="space-y-1">
		<h2 class="text-lg font-semibold tracking-tight">Appearance</h2>
		<p class="text-sm text-muted-foreground">
			The theme sets the colours, layout and navigation for this organization. Install more themes
			as plugins.
		</p>
	</div>

	{#if loading}
		<DetailSkeleton fields={4} label="Loading themes…" />
	{:else if !orgStore.current}
		<div class="space-y-3 rounded-md border border-dashed border-border bg-card p-8 text-center">
			<p class="font-medium">Appearance is set per organization</p>
			<p class="text-sm text-muted-foreground">Enter an organization to choose its theme.</p>
			<Button onclick={() => orgStore.openSwitcher()}>Choose organization</Button>
		</div>
	{:else}
		{#if error}
			<p class="text-sm text-destructive" role="alert">{error}</p>
		{/if}

		<ul class="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
			{#each ordered as entry (entry.name ?? '__default__')}
				{@const isActive = entry.name === active}
				<li
					class="space-y-3 rounded-md border bg-card p-4 shadow-xs {isActive
						? 'border-primary ring-1 ring-primary'
						: 'border-border'}"
					aria-current={isActive ? 'true' : undefined}
				>
					<ThemePreview {...entry.preview} />
					<div class="flex items-start justify-between gap-2">
						<div>
							<p class="text-sm font-semibold">{entry.label}</p>
							<p class="text-xs text-muted-foreground">
								{#if entry.name === null}
									Built-in default
								{:else}
									<span class="font-mono">{entry.name}</span> · {entry.layout} layout
								{/if}
							</p>
						</div>
						{#if isActive}
							<span class="text-xs font-semibold tracking-widest text-primary uppercase"
								>Active</span
							>
						{:else}
							<Button
								size="sm"
								variant="outline"
								disabled={busy !== null}
								onclick={() => activate(entry.name)}
							>
								{busy === (entry.name ?? '__default__') ? 'Applying…' : 'Use this'}
							</Button>
						{/if}
					</div>
				</li>
			{/each}
		</ul>

		{#if themes.length === 0}
			<p class="text-sm text-muted-foreground">
				No theme plugins are installed for this organization, so the built-in default is in use.
			</p>
		{/if}
	{/if}
</section>
