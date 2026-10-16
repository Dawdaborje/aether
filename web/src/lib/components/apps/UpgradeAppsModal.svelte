<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import {
		fetchCatalog,
		installApps,
		type CatalogPlugin,
		type InstallOutcome
	} from '$lib/apps/catalogApi';
	import { groupCatalogByName } from '$lib/apps/catalogGroup';
	import OrgPicker from '$lib/components/apps/OrgPicker.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { open = $bindable(false) }: { open?: boolean } = $props();

	interface Candidate {
		name: string;
		label: string;
		from: string;
		to: string;
	}

	let catalog = $state<CatalogPlugin[] | null>(null);
	let loadError = $state<string | null>(null);
	/** Chosen `org|name@version` keys. */
	let chosen = $state<string[]>([]);
	/** Organizations in scope for this upgrade run. */
	let scope = $state<string[]>([]);
	let working = $state(false);
	let results = $state<InstallOutcome[] | null>(null);
	let failed = $state<string | null>(null);

	const orgName = (dbName: string) =>
		orgStore.organizations.find((org) => org.db_name === dbName)?.name ?? dbName;
	const keyOf = (org: string, plugin: Candidate) => `${org}|${plugin.name}@${plugin.to}`;

	/** For each organization, the installed plugins that have a newer catalogued version. */
	const byOrg = $derived.by(() => {
		const out = new Map<string, Candidate[]>();
		for (const group of groupCatalogByName(catalog ?? [])) {
			group.versions.forEach((plugin, index) => {
				if (index === 0) return;
				for (const org of plugin.installed_in) {
					const list = out.get(org) ?? [];
					list.push({
						name: group.name,
						label: group.latest.label,
						from: plugin.version,
						to: group.latest.version
					});
					out.set(org, list);
				}
			});
		}
		return [...out.entries()]
			.map(([org, plugins]) => ({
				org,
				plugins: plugins.sort((a, b) => a.label.localeCompare(b.label))
			}))
			.sort((a, b) => orgName(a.org).localeCompare(orgName(b.org)));
	});

	const scoped = $derived(byOrg.filter(({ org }) => scope.includes(org)));
	const scopedChosen = $derived(
		scoped.reduce(
			(n, { org, plugins }) =>
				n + plugins.filter((plugin) => chosen.includes(keyOf(org, plugin))).length,
			0
		)
	);
	const upgradeOrgs = $derived(byOrg.map(({ org }) => ({ db_name: org, name: orgName(org) })));
	const SECTION_CAP = 50;

	async function load() {
		catalog = null;
		loadError = null;
		try {
			catalog = await fetchCatalog();
		} catch (error) {
			loadError = error instanceof Error ? error.message : 'Could not load the catalog';
			catalog = [];
		}
		chosen = byOrg.flatMap(({ org, plugins }) => plugins.map((plugin) => keyOf(org, plugin)));
		scope = byOrg.map(({ org }) => org);
	}

	$effect(() => {
		if (!open) return;
		results = null;
		failed = null;
		void load();
	});

	function toggle(key: string) {
		chosen = chosen.includes(key) ? chosen.filter((item) => item !== key) : [...chosen, key];
	}

	function setOrg(org: string, plugins: Candidate[], on: boolean) {
		const keys = new Set(plugins.map((plugin) => keyOf(org, plugin)));
		chosen = [...chosen.filter((key) => !keys.has(key)), ...(on ? keys : [])];
	}

	async function upgrade(only?: string) {
		working = true;
		failed = null;
		try {
			const outcomes: InstallOutcome[] = [];
			for (const { org, plugins } of scoped) {
				if (only && org !== only) continue;
				const picks = plugins
					.filter((plugin) => chosen.includes(keyOf(org, plugin)))
					.map((plugin) => `${plugin.name}@${plugin.to}`);
				if (picks.length === 0) continue;
				// Installing a plugin an organization already has moves it to that version.
				outcomes.push(...(await installApps(picks, [org])));
			}
			results = outcomes;
			if (orgStore.current) await appsStore.load();
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not upgrade';
		} finally {
			working = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content
		class="flex h-[min(90vh,52rem)] w-[min(96rem,calc(100%-2rem))] max-w-[min(96rem,calc(100%-2rem))] flex-col gap-4 overflow-hidden sm:max-w-[min(96rem,calc(100%-2rem))]"
	>
		<Dialog.Header>
			<Dialog.Title>Upgrade modules</Dialog.Title>
			<Dialog.Description>
				Modules each organization has installed that have a newer loaded version.
			</Dialog.Description>
		</Dialog.Header>

		{#if results}
			<ul class="min-h-0 flex-1 space-y-2 overflow-auto text-sm">
				{#each results as outcome (outcome.organization)}
					<li class="rounded-md border border-border p-3">
						<p class="font-medium">{orgName(outcome.organization)}</p>
						{#if outcome.error}
							<p class="text-destructive" role="alert">{outcome.error}</p>
						{:else if outcome.upgraded.length > 0}
							<p class="text-muted-foreground">Upgraded: {outcome.upgraded.join(', ')}</p>
						{:else}
							<p class="text-muted-foreground">Nothing to upgrade.</p>
						{/if}
					</li>
				{/each}
			</ul>
			<Dialog.Footer>
				<Button onclick={() => (open = false)}>Done</Button>
			</Dialog.Footer>
		{:else if catalog === null}
			<p class="text-sm text-muted-foreground">Loading…</p>
		{:else if loadError}
			<p class="text-sm text-destructive" role="alert">{loadError}</p>
		{:else if byOrg.length === 0}
			<p class="text-sm text-muted-foreground">Every installed module is up to date.</p>
			<Dialog.Footer>
				<Button onclick={() => (open = false)}>Close</Button>
			</Dialog.Footer>
		{:else}
			<div class="grid min-h-0 flex-1 gap-6 md:grid-cols-[16rem_minmax(0,1fr)]">
			<OrgPicker
				orgs={upgradeOrgs}
				bind:selected={scope}
				heading="Organizations to upgrade"
				emptyText="No organization has upgrades."
			/>
			<div class="min-h-0 space-y-4 overflow-auto">
				{#if scoped.length === 0}
					<p class="text-sm text-muted-foreground">Select at least one organization.</p>
				{/if}
				{#each scoped.slice(0, SECTION_CAP) as { org, plugins } (org)}
					{@const picked = plugins.filter((plugin) => chosen.includes(keyOf(org, plugin))).length}
					<section class="rounded-md border border-border">
						<header class="flex items-center justify-between gap-2 border-b border-border p-3">
							<div>
								<h3 class="text-sm font-semibold">{orgName(org)}</h3>
								<p class="text-xs text-muted-foreground">
									{plugins.length} upgrade{plugins.length === 1 ? '' : 's'} available
								</p>
							</div>
							<div class="flex items-center gap-2">
								<Button
									size="sm"
									variant="ghost"
									onclick={() => setOrg(org, plugins, picked < plugins.length)}
								>
									{picked < plugins.length ? 'Select all' : 'Clear'}
								</Button>
								<Button size="sm" disabled={working || picked === 0} onclick={() => upgrade(org)}>
									{working ? 'Upgrading…' : `Upgrade ${picked}`}
								</Button>
							</div>
						</header>
						<ul class="divide-y divide-border text-sm">
							{#each plugins as plugin (plugin.name)}
								<li>
									<label class="flex cursor-pointer items-center gap-3 p-3">
										<input
											type="checkbox"
											checked={chosen.includes(keyOf(org, plugin))}
											onchange={() => toggle(keyOf(org, plugin))}
										/>
										<span class="min-w-0 flex-1 truncate font-medium">{plugin.label}</span>
										<span class="font-mono text-xs text-muted-foreground">
											{plugin.from} → {plugin.to}
										</span>
									</label>
								</li>
							{/each}
						</ul>
					</section>
				{/each}
				{#if scoped.length > SECTION_CAP}
					<p class="text-xs text-muted-foreground">
						Showing {SECTION_CAP} of {scoped.length} organizations; “Upgrade all selected” still
						applies to all of them.
					</p>
				{/if}
			</div>
			</div>
			{#if failed}
				<p class="text-sm text-destructive" role="alert">{failed}</p>
			{/if}
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
				<Button disabled={working || scopedChosen === 0} onclick={() => upgrade()}>
					{working ? 'Upgrading…' : `Upgrade all selected (${scopedChosen})`}
				</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
