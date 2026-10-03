<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { categoryLabel } from '$lib/apps/categories';
	import {
		fetchCatalog,
		installApps,
		type CatalogPlugin,
		type InstallOutcome
	} from '$lib/apps/catalogApi';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { open = $bindable(false) }: { open: boolean } = $props();

	let catalog = $state<CatalogPlugin[] | null>(null);
	let loadError = $state<string | null>(null);
	let chosenApps = $state<string[]>([]);
	let chosenOrgs = $state<string[]>([]);
	let working = $state(false);
	let results = $state<InstallOutcome[] | null>(null);
	let failed = $state<string | null>(null);

	const key = (plugin: CatalogPlugin) => `${plugin.name}@${plugin.version}`;
	const organizations = $derived(orgStore.organizations);
	const orgName = (dbName: string) =>
		organizations.find((org) => org.db_name === dbName)?.name ?? dbName;

	async function load() {
		catalog = null;
		loadError = null;
		try {
			catalog = await fetchCatalog();
		} catch (error) {
			loadError = error instanceof Error ? error.message : 'Could not load the catalog';
			catalog = [];
		}
	}

	// Start fresh each time the modal opens, with the current organization preselected.
	$effect(() => {
		if (!open) return;
		chosenApps = [];
		results = null;
		failed = null;
		chosenOrgs = orgStore.current ? [orgStore.current] : [];
		void load();
	});

	function toggle(list: string[], value: string): string[] {
		return list.includes(value) ? list.filter((item) => item !== value) : [...list, value];
	}

	const allApps = $derived((catalog ?? []).map(key));
	const allOrgs = $derived(organizations.map((org) => org.db_name));

	async function install() {
		working = true;
		failed = null;
		try {
			results = await installApps(chosenApps, chosenOrgs);
			await Promise.all([appsStore.load(), load()]);
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not install';
		} finally {
			working = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-w-3xl">
		<Dialog.Header>
			<Dialog.Title>Install apps</Dialog.Title>
			<Dialog.Description>
				Choose the apps and the organizations to install them in. You can pick several of each.
			</Dialog.Description>
		</Dialog.Header>

		{#if results}
			<ul class="max-h-80 space-y-2 overflow-auto text-sm">
				{#each results as outcome (outcome.organization)}
					<li class="rounded-md border border-border p-3">
						<p class="font-medium">{orgName(outcome.organization)}</p>
						{#if outcome.error}
							<p class="text-destructive" role="alert">{outcome.error}</p>
						{:else}
							{#if outcome.installed.length > 0}
								<p class="text-muted-foreground">Installed: {outcome.installed.join(', ')}</p>
							{/if}
							{#if outcome.already_installed.length > 0}
								<p class="text-muted-foreground">
									Already there: {outcome.already_installed.join(', ')}
								</p>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
			<Dialog.Footer>
				<Button onclick={() => (open = false)}>Done</Button>
			</Dialog.Footer>
		{:else}
			<div class="grid gap-6 sm:grid-cols-2">
				<section class="space-y-2">
					<div class="flex items-center justify-between">
						<h3 class="text-sm font-semibold">Apps</h3>
						{#if catalog && catalog.length > 0}
							<button
								type="button"
								class="text-xs text-primary hover:underline"
								onclick={() => (chosenApps = chosenApps.length === allApps.length ? [] : allApps)}
							>
								{chosenApps.length === allApps.length ? 'Clear' : 'Select all'}
							</button>
						{/if}
					</div>
					<ul class="max-h-72 space-y-1 overflow-auto rounded-md border border-border p-1.5">
						{#if catalog === null}
							<li class="p-2 text-sm text-muted-foreground">Loading…</li>
						{:else if loadError}
							<li class="p-2 text-sm text-destructive" role="alert">{loadError}</li>
						{:else if catalog.length === 0}
							<li class="p-2 text-sm text-muted-foreground">
								No apps are loaded in. Load one with <code>aether --load-plugin</code>.
							</li>
						{:else}
							{#each catalog as plugin (key(plugin))}
								<li>
									<label
										class="flex cursor-pointer items-start gap-2.5 rounded-md p-2 hover:bg-accent"
									>
										<input
											type="checkbox"
											class="mt-0.5 rounded-sm"
											checked={chosenApps.includes(key(plugin))}
											onchange={() => (chosenApps = toggle(chosenApps, key(plugin)))}
										/>
										<span class="min-w-0 text-sm">
											<span class="block font-medium">
												{plugin.label}
												<span class="font-normal text-muted-foreground">v{plugin.version}</span>
											</span>
											<span class="block text-xs text-muted-foreground">
												{categoryLabel(plugin.category)}{plugin.installed_in.length > 0
													? ` · in ${plugin.installed_in.length} organization${plugin.installed_in.length === 1 ? '' : 's'}`
													: ''}
											</span>
										</span>
									</label>
								</li>
							{/each}
						{/if}
					</ul>
				</section>

				<section class="space-y-2">
					<div class="flex items-center justify-between">
						<h3 class="text-sm font-semibold">Organizations</h3>
						{#if organizations.length > 1}
							<button
								type="button"
								class="text-xs text-primary hover:underline"
								onclick={() => (chosenOrgs = chosenOrgs.length === allOrgs.length ? [] : allOrgs)}
							>
								{chosenOrgs.length === allOrgs.length ? 'Clear' : 'Select all'}
							</button>
						{/if}
					</div>
					<ul class="max-h-72 space-y-1 overflow-auto rounded-md border border-border p-1.5">
						{#if organizations.length === 0}
							<li class="p-2 text-sm text-muted-foreground">
								There are no organizations to install into yet.
							</li>
						{:else}
							{#each organizations as org (org.db_name)}
								<li>
									<label
										class="flex cursor-pointer items-center gap-2.5 rounded-md p-2 hover:bg-accent"
									>
										<input
											type="checkbox"
											class="rounded-sm"
											checked={chosenOrgs.includes(org.db_name)}
											onchange={() => (chosenOrgs = toggle(chosenOrgs, org.db_name))}
										/>
										<span class="text-sm font-medium">{org.name}</span>
									</label>
								</li>
							{/each}
						{/if}
					</ul>
				</section>
			</div>

			{#if failed}
				<p class="text-sm text-destructive" role="alert">{failed}</p>
			{/if}
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
				<Button
					disabled={working || chosenApps.length === 0 || chosenOrgs.length === 0}
					onclick={install}
				>
					{working ? 'Installing…' : chosenApps.length > 0 ? `Install (${chosenApps.length})` : 'Install'}
				</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
