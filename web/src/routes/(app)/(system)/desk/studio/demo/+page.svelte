<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import BoxesIcon from '@lucide/svelte/icons/boxes';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import OrgSearchPicker from '$lib/components/org/OrgSearchPicker.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { SearchPicker } from '$lib/components/ui/search-picker';
	import { authSession } from '$lib/auth/session.svelte';
	import { landingPath } from '$lib/auth/landing';
	import {
		fetchDemoCatalog,
		generateDemo,
		type DemoPluginEntry,
		type GenerateDemoResult
	} from '$lib/demo/api';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let selectedDb = $state<string | null>(null);
	let plugins = $state<DemoPluginEntry[] | null>(null);
	let loadError = $state<string | null>(null);
	let selectedPlugin = $state<string | null>(null);
	let selectedModel = $state<string | null>(null);
	let count = $state(10);
	let running = $state(false);
	let runError = $state<string | null>(null);
	let lastResult = $state<GenerateDemoResult | null>(null);

	onMount(() => {
		if (!authSession.isDeveloper) {
			void goto(resolve(landingPath(authSession.me?.user)));
			return;
		}
		void orgStore.load();
	});

	$effect(() => {
		const current = orgStore.current;
		const orgs = orgStore.organizations;
		if (selectedDb && orgs.some((org) => org.db_name === selectedDb)) return;
		selectedDb =
			current && orgs.some((org) => org.db_name === current)
				? current
				: (orgs[0]?.db_name ?? null);
	});

	$effect(() => {
		const org = selectedDb;
		if (!org) {
			plugins = null;
			return;
		}
		let cancelled = false;
		plugins = null;
		loadError = null;
		selectedPlugin = null;
		selectedModel = null;
		lastResult = null;
		runError = null;
		void (async () => {
			try {
				const list = await fetchDemoCatalog(org);
				if (cancelled) return;
				plugins = list;
				selectedPlugin = list[0]?.name ?? null;
			} catch (error) {
				if (cancelled) return;
				loadError = error instanceof Error ? error.message : 'Could not load demo catalog';
				plugins = [];
			}
		})();
		return () => {
			cancelled = true;
		};
	});

	const activePlugin = $derived(plugins?.find((p) => p.name === selectedPlugin) ?? null);

	$effect(() => {
		const models = activePlugin?.models ?? [];
		if (!selectedModel || !models.some((m) => m.name === selectedModel)) {
			selectedModel = models[0]?.name ?? null;
		}
	});

	const activeModel = $derived(
		activePlugin?.models.find((m) => m.name === selectedModel) ?? null
	);

	const pluginItems = $derived(
		(plugins ?? []).map((p) => ({
			id: p.name,
			label: p.label || p.name,
			sublabel: `${p.name} · ${p.version}`
		}))
	);

	const modelItems = $derived(
		(activePlugin?.models ?? []).map((m) => ({
			id: m.name,
			label: m.label || m.name,
			sublabel: m.command
		}))
	);

	async function run() {
		if (!selectedDb || !selectedPlugin || !selectedModel) return;
		const n = Math.floor(Number(count));
		if (!Number.isFinite(n) || n < 1) {
			runError = 'Count must be at least 1';
			return;
		}
		running = true;
		runError = null;
		lastResult = null;
		try {
			lastResult = await generateDemo({
				org: selectedDb,
				plugin: selectedPlugin,
				model: selectedModel,
				count: n
			});
		} catch (error) {
			runError = error instanceof Error ? error.message : 'Generation failed';
		} finally {
			running = false;
		}
	}
</script>

{#if authSession.isDeveloper}
	<ControlPanel
		crumbs={[
			{ label: 'Developer tools', href: '/desk/studio' },
			{ label: 'Demo data' }
		]}
	/>

	<div class="flex w-full flex-1 flex-col gap-6 px-4 py-4 md:px-6">
		<p class="max-w-2xl text-sm text-muted-foreground">
			Generate sample records for models that a plugin opts into with
			<code class="rounded bg-muted px-1 py-0.5 text-xs">demo_model</code> on a command. Choose an
			organization where the plugin is installed, then the model and how many to create — the
			plugin does the work.
		</p>

		<div class="flex flex-wrap items-end gap-4">
			{#if orgStore.organizations.length > 0}
				<OrgSearchPicker
					organizations={orgStore.organizations}
					bind:value={selectedDb}
					currentDb={orgStore.current}
				/>
			{/if}
		</div>

		{#if !orgStore.loaded}
			<p class="text-sm text-muted-foreground">Loading organizations…</p>
		{:else if !selectedDb}
			<div class="rounded-md border border-dashed border-border px-4 py-8 text-center">
				<p class="text-sm text-muted-foreground">
					Choose an organization. Demo generators run against that org’s installed plugins.
				</p>
			</div>
		{:else if loadError}
			<p class="text-sm text-destructive">{loadError}</p>
		{:else if plugins === null}
			<p class="text-sm text-muted-foreground">Loading demo catalog…</p>
		{:else if plugins.length === 0}
			<div class="rounded-md border border-dashed border-border px-4 py-8 text-center">
				<p class="text-sm text-muted-foreground">
					No installed plugin in <span class="font-mono">{selectedDb}</span> offers demo generation.
					Add
					<code class="rounded bg-muted px-1 py-0.5 text-xs">demo_model</code>
					on a
					<code class="rounded bg-muted px-1 py-0.5 text-xs">[[command]]</code>
					in
					<code class="rounded bg-muted px-1 py-0.5 text-xs">plugin.toml</code>.
				</p>
			</div>
		{:else}
			<div class="grid max-w-3xl gap-4 sm:grid-cols-2">
				<SearchPicker
					items={pluginItems}
					bind:value={selectedPlugin}
					label="Plugin"
					placeholder="Select plugin"
					searchPlaceholder="Search plugins…"
					entityLabel="plugins"
					icon={BoxesIcon}
				/>
				<SearchPicker
					items={modelItems}
					bind:value={selectedModel}
					label="Model"
					placeholder="Select model"
					searchPlaceholder="Search models…"
					entityLabel="models"
					icon={DatabaseIcon}
				/>
			</div>

			{#if activeModel}
				<div class="flex max-w-3xl flex-wrap items-end gap-4">
					<div class="space-y-2">
						<Label for="demo-count">Count</Label>
						<Input
							id="demo-count"
							type="number"
							min="1"
							max="10000"
							class="w-32"
							bind:value={count}
						/>
						<p class="text-xs text-muted-foreground">
							Passed to <span class="font-mono">{activePlugin?.name}.{activeModel.command}</span>
						</p>
					</div>
					<Button disabled={running} onclick={() => void run()}>
						{running ? 'Generating…' : 'Generate'}
					</Button>
				</div>
			{/if}

			{#if runError}
				<p class="text-sm text-destructive">{runError}</p>
			{/if}
			{#if lastResult}
				<div class="max-w-3xl space-y-2">
					<p class="text-sm font-medium text-foreground">
						Generated via
						<span class="font-mono"
							>{lastResult.plugin}.{lastResult.command}</span
						>
						(count {lastResult.count})
					</p>
					<pre
						class="overflow-x-auto rounded-md border border-border bg-muted/40 p-3 text-xs">{JSON.stringify(
							lastResult.result,
							null,
							2
						)}</pre>
				</div>
			{/if}
		{/if}
	</div>
{/if}
