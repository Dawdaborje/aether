<script lang="ts">
	import { resolve } from '$app/paths';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { categoryLabel } from '$lib/apps/categories';
	import {
		fetchPluginDetails,
		fetchPluginFunctions,
		seedApps,
		type CatalogPlugin,
		type PluginDetails,
		type PluginFunctions
	} from '$lib/apps/catalogApi';
	import { formatCatalogDate, formatFileChangeSummary } from '$lib/apps/formatCatalogDate';
	import { appIcon } from './icons';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let {
		open = $bindable(false),
		versions = [],
		onInstall
	}: {
		open?: boolean;
		/** Every catalogued revision of this plugin (newest first). */
		versions?: CatalogPlugin[];
		onInstall?: (plugin: CatalogPlugin) => void;
	} = $props();

	let selectedVersion = $state<string | null>(null);

	$effect(() => {
		if (!open || versions.length === 0) {
			selectedVersion = null;
			return;
		}
		if (!selectedVersion || !versions.some((v) => v.version === selectedVersion)) {
			selectedVersion = versions[0]!.version;
		}
	});

	const plugin = $derived(
		versions.find((v) => v.version === selectedVersion) ?? versions[0] ?? null
	);
	const Icon = $derived(plugin ? appIcon(plugin.icon) : undefined);
	let logoFailed = $state(false);

	$effect(() => {
		void plugin?.logo_url;
		logoFailed = false;
	});

	// The functions the selected version offers, read when the version changes.
	let functionsInfo = $state<PluginFunctions | null>(null);
	let functionsError = $state<string | null>(null);
	let functionsLoading = $state(false);
	let functionsFor = '';

	$effect(() => {
		const current = open ? plugin : null;
		if (!current) {
			functionsInfo = null;
			functionsError = null;
			functionsFor = '';
			return;
		}
		const wanted = `${current.name}@${current.version}`;
		if (wanted === functionsFor) return;
		functionsFor = wanted;
		functionsInfo = null;
		functionsError = null;
		functionsLoading = true;
		fetchPluginFunctions(current.name, current.version)
			.then((info) => {
				if (functionsFor === wanted) functionsInfo = info;
			})
			.catch((error) => {
				if (functionsFor === wanted) {
					functionsError =
						error instanceof Error ? error.message : 'Could not read the plugin functions';
				}
			})
			.finally(() => {
				if (functionsFor === wanted) functionsLoading = false;
			});
	});

	// What the version asks for and offers: capabilities, model access, roles, pages, models.
	let details = $state<PluginDetails | null>(null);
	let detailsError = $state<string | null>(null);
	let detailsFor = '';

	$effect(() => {
		const current = open ? plugin : null;
		if (!current) {
			details = null;
			detailsError = null;
			detailsFor = '';
			return;
		}
		const wanted = `${current.name}@${current.version}`;
		if (wanted === detailsFor) return;
		detailsFor = wanted;
		details = null;
		detailsError = null;
		fetchPluginDetails(current.name, current.version)
			.then((info) => {
				if (detailsFor === wanted) details = info;
			})
			.catch((error) => {
				if (detailsFor === wanted) {
					detailsError =
						error instanceof Error ? error.message : 'Could not read the plugin details';
				}
			});
	});

	const createdLabel = $derived(plugin ? formatCatalogDate(plugin.date_created) : null);
	const changeSummary = $derived(plugin ? formatFileChangeSummary(plugin) : null);
	const hasFileDiff = $derived(
		!!plugin &&
			(plugin.files_added.length > 0 ||
				plugin.files_changed.length > 0 ||
				plugin.files_removed.length > 0)
	);

	const orgName = (dbName: string) =>
		orgStore.organizations.find((org) => org.db_name === dbName)?.name ?? dbName;

	const openHref = $derived.by(() => {
		if (!plugin?.app_route) return null;
		const inCurrent =
			!!orgStore.current && plugin.installed_in.includes(orgStore.current);
		if (!inCurrent) return null;
		return resolve(plugin.app_route as `/${string}`);
	});

	const canSeed = $derived(
		!!plugin && plugin.seed_commands.length > 0 && plugin.installed_in.length > 0
	);

	let seeding = $state(false);
	let seedMessage = $state<string | null>(null);
	let seedError = $state<string | null>(null);

	$effect(() => {
		void plugin?.name;
		seedMessage = null;
		seedError = null;
	});

	async function onSeed() {
		if (!plugin || !canSeed) return;
		seeding = true;
		seedMessage = null;
		seedError = null;
		try {
			const orgs =
				orgStore.current && plugin.installed_in.includes(orgStore.current)
					? [orgStore.current]
					: [...plugin.installed_in];
			const results = await seedApps([plugin.name], orgs);
			const parts: string[] = [];
			for (const row of results) {
				if (row.error) {
					parts.push(`${orgName(row.organization)}: ${row.error}`);
					continue;
				}
				if (row.seeded.length) {
					parts.push(`${orgName(row.organization)}: ${row.seeded.join(', ')}`);
				}
				if (row.failed.length) {
					parts.push(`${orgName(row.organization)} failed: ${row.failed.join('; ')}`);
				}
			}
			seedMessage = parts.join(' · ') || 'No seed commands ran.';
			if (results.some((row) => row.failed.length > 0 || row.error)) {
				seedError = 'Some seeds did not finish; see the message above.';
			}
		} catch (error) {
			seedError = error instanceof Error ? error.message : 'Could not seed data';
		} finally {
			seeding = false;
		}
	}
</script>

<Dialog.Root bind:open>
	{#if plugin}
		<Dialog.Content
			class="max-h-[min(90vh,40rem)] w-full max-w-[min(82.8rem,calc(100%-2rem))] overflow-y-auto sm:max-w-[min(82.8rem,calc(100%-2rem))]"
		>
			<Dialog.Header>
				<div class="flex items-start gap-3">
					<span
						class="flex size-14 shrink-0 items-center justify-center overflow-hidden bg-primary/10 text-primary ring-1 ring-primary/15"
						aria-hidden="true"
					>
						{#if plugin.logo_url && !logoFailed}
							<img
								src={plugin.logo_url}
								alt=""
								class="size-12 object-contain"
								onerror={() => (logoFailed = true)}
							/>
						{:else if Icon}
							<Icon class="size-7" />
						{:else}
							<span class="text-2xl font-semibold">{plugin.label.charAt(0).toUpperCase()}</span>
						{/if}
					</span>
					<div class="min-w-0 space-y-1">
						<Dialog.Title class="text-left">{plugin.label}</Dialog.Title>
						<Dialog.Description class="text-left">
							<code class="text-xs">{plugin.name}</code>
							{#if versions.length === 1}
								<span class="text-muted-foreground"> · v{plugin.version}</span>
							{:else}
								<span class="text-muted-foreground">
									· {versions.length} versions
								</span>
							{/if}
						</Dialog.Description>
					</div>
				</div>
			</Dialog.Header>

			{#if versions.length > 1}
				<div
					class="-mx-1 flex gap-1 overflow-x-auto border-b border-border pb-3"
					role="tablist"
					aria-label="Plugin versions"
				>
					{#each versions as version (version.version)}
						{@const date = formatCatalogDate(version.date_created)}
						{@const summary = formatFileChangeSummary(version)}
						<button
							type="button"
							role="tab"
							aria-selected={plugin.version === version.version}
							class="min-w-[10.5rem] shrink-0 rounded-md px-2.5 py-2 text-left {plugin.version ===
							version.version
								? 'bg-accent font-medium text-accent-foreground'
								: 'text-muted-foreground hover:bg-muted'}"
							onclick={() => (selectedVersion = version.version)}
						>
							<span class="block font-mono text-xs">v{version.version}</span>
							{#if date}
								<span class="mt-0.5 block text-xs leading-snug opacity-80">{date}</span>
							{/if}
							{#if summary}
								<span class="mt-0.5 block text-xs leading-snug opacity-70">{summary}</span>
							{/if}
						</button>
					{/each}
				</div>
			{/if}

			<div class="grid gap-6 text-sm lg:grid-cols-[minmax(0,1fr)_minmax(16rem,22rem)]">
				<div class="min-w-0 space-y-4">
					<dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
						<dt class="text-muted-foreground">Version</dt>
						<dd><code class="text-xs">{plugin.version}</code></dd>
						{#if createdLabel}
							<dt class="text-muted-foreground">Catalogued</dt>
							<dd>{createdLabel}</dd>
						{/if}
						<dt class="text-muted-foreground">Category</dt>
						<dd>{categoryLabel(plugin.category)}</dd>
						{#if plugin.kind}
							<dt class="text-muted-foreground">Kind</dt>
							<dd class="capitalize">{plugin.kind}</dd>
						{/if}
						<dt class="text-muted-foreground">Type</dt>
						<dd>
							{plugin.is_app ? 'App' : 'Library'}
							{#if plugin.is_builtin}
								<span class="text-muted-foreground"> · built-in</span>
							{/if}
						</dd>
						{#if plugin.app_route}
							<dt class="text-muted-foreground">Route</dt>
							<dd><code class="text-xs">{plugin.app_route}</code></dd>
						{/if}
						{#if plugin.website}
							<dt class="text-muted-foreground">Website</dt>
							<dd>
								<a
									href={plugin.website}
									class="underline underline-offset-2"
									target="_blank"
									rel="noreferrer">{plugin.website}</a
								>
							</dd>
						{/if}
					</dl>

					{#if plugin.description}
						<p>{plugin.description}</p>
					{/if}
					{#if plugin.app_description && plugin.app_description !== plugin.description}
						<p class="text-muted-foreground">{plugin.app_description}</p>
					{/if}
					{#if plugin.long_description}
						<p class="whitespace-pre-wrap text-muted-foreground">{plugin.long_description}</p>
					{/if}

					{#if plugin.authors.length > 0}
						<section class="space-y-1">
							<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
								Authors
							</h3>
							<ul class="space-y-0.5">
								{#each plugin.authors as author (author.name + (author.email ?? ''))}
									<li>
										{author.name}
										{#if author.email}
											<span class="text-muted-foreground"> · {author.email}</span>
										{/if}
									</li>
								{/each}
							</ul>
						</section>
					{/if}

					{#if plugin.dependencies.length > 0}
						<section class="space-y-1">
							<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
								Depends on
							</h3>
							<p class="flex flex-wrap gap-1.5">
								{#each plugin.dependencies as dep (dep)}
									<code class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">{dep}</code>
								{/each}
							</p>
						</section>
					{/if}

					<section class="space-y-1">
						<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Installed in
						</h3>
						{#if plugin.installed_in.length === 0}
							<p class="text-muted-foreground">Not installed in any organization yet.</p>
						{:else}
							<ul class="list-inside list-disc">
								{#each plugin.installed_in as db (db)}
									<li>{orgName(db)}</li>
								{/each}
							</ul>
						{/if}
					</section>

					{#if plugin.seed_commands.length > 0}
						<section class="space-y-1">
							<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
								Master data
							</h3>
							<p class="text-muted-foreground">
								Seed commands: {plugin.seed_commands.join(', ')}. Run them to create
								default records (currencies, countries, calendars, …) in an organization
								that has this plugin.
							</p>
						</section>
					{/if}

					<section class="space-y-3">
						<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Access &amp; contents
						</h3>
						{#if detailsError}
							<p class="text-destructive" role="alert">{detailsError}</p>
						{:else if !details}
							<p class="text-muted-foreground">Reading the plugin…</p>
						{:else}
							{#if !details.manifest_read}
								<p class="text-xs text-muted-foreground">
									The stored plugin.toml could not be read, so only what was loaded is listed.
								</p>
							{/if}
							{#if details.definition}
								{@const def = details.definition}
								<dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
									<dt class="text-muted-foreground">API</dt>
									<dd><code class="text-xs">{def.api_version}</code></dd>
									{#if def.code}
										<dt class="text-muted-foreground">Code</dt>
										<dd><code class="text-xs">{def.code}</code></dd>
									{/if}
									<dt class="text-muted-foreground">Capabilities</dt>
									<dd class="flex flex-wrap gap-1.5">
										{#each def.capabilities as capability (capability)}
											<code class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">{capability}</code>
										{:else}
											<span class="text-muted-foreground">none</span>
										{/each}
									</dd>
									<dt class="text-muted-foreground">Model access</dt>
									<dd class="flex flex-wrap gap-1.5">
										{#each def.access_models as grant (grant.name)}
											<span class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">
												<code>{grant.name}</code>
												<span class="text-muted-foreground">{grant.permissions.join(', ')}</span>
											</span>
										{:else}
											<span class="text-muted-foreground">none</span>
										{/each}
									</dd>
									{#if def.public_functions.length > 0 || def.public_capabilities.length > 0 || def.public_access_models.length > 0}
										<dt class="text-muted-foreground">For visitors</dt>
										<dd class="space-y-1">
											{#if def.public_functions.length > 0}
												<p class="text-xs">
													Functions: {def.public_functions.join(', ')}
												</p>
											{/if}
											{#if def.public_capabilities.length > 0}
												<p class="text-xs">Capabilities: {def.public_capabilities.join(', ')}</p>
											{/if}
											{#if def.public_access_models.length > 0}
												<p class="text-xs">
													Models: {def.public_access_models
														.map((g) => `${g.name} (${g.permissions.join(', ')})`)
														.join(', ')}
												</p>
											{/if}
										</dd>
									{/if}
									{#if def.http_hosts.length > 0}
										<dt class="text-muted-foreground">Calls out to</dt>
										<dd class="flex flex-wrap gap-1.5">
											{#each def.http_hosts as host (host)}
												<code class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">{host}</code>
											{/each}
										</dd>
									{/if}
									{#if def.bridges.length > 0}
										<dt class="text-muted-foreground">Bridges</dt>
										<dd>{def.bridges.join(', ')}</dd>
									{/if}
									{#if def.i18n_default}
										<dt class="text-muted-foreground">Text language</dt>
										<dd>{def.i18n_default}</dd>
									{/if}
								</dl>
							{/if}

							{#if details.roles.length > 0}
								<div class="space-y-1">
									<p class="text-xs font-medium text-muted-foreground">
										Roles ({details.roles.length})
									</p>
									<ul class="divide-y rounded-md border">
										{#each details.roles as role (role.name)}
											<li class="px-2.5 py-1.5">
												<code class="text-xs font-medium">{role.name}</code>
												{#if role.label}<span class="ml-1.5">{role.label}</span>{/if}
												{#if role.description}
													<p class="text-xs text-muted-foreground">{role.description}</p>
												{/if}
											</li>
										{/each}
									</ul>
								</div>
							{/if}

							{#if details.pages.length > 0}
								<div class="space-y-1">
									<p class="text-xs font-medium text-muted-foreground">
										Pages ({details.pages.length})
									</p>
									<ul class="divide-y rounded-md border">
										{#each details.pages as page (page.route)}
											<li class="flex flex-wrap items-center gap-1.5 px-2.5 py-1.5">
												{#if page.extends}
													<span class="rounded-sm bg-muted px-1.5 text-[10px]">tab</span>
													<span>{page.title}</span>
													<span class="text-xs text-muted-foreground">
														on <code>{page.extends}</code>
													</span>
												{:else}
													<code class="text-xs">{page.route}</code>
													<span>{page.title}</span>
												{/if}
												{#if page.public}
													<span class="rounded-sm bg-primary/10 px-1.5 text-[10px] text-primary"
														>public</span
													>
												{/if}
												{#if page.roles?.length}
													<span class="text-xs text-muted-foreground">
														roles: {page.roles.join(', ')}
													</span>
												{/if}
											</li>
										{/each}
									</ul>
								</div>
							{/if}

							{#if details.models.length > 0}
								<div class="space-y-1">
									<p class="text-xs font-medium text-muted-foreground">
										Models ({details.models.length})
									</p>
									<p class="flex flex-wrap gap-1.5">
										{#each details.models as model (model.name)}
											<span class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">
												<code>{model.name}</code>
												{#if model.fields !== null}
													<span class="text-muted-foreground">{model.fields} fields</span>
												{/if}
											</span>
										{/each}
									</p>
								</div>
							{/if}

							{#if details.components.length > 0}
								<div class="space-y-1">
									<p class="text-xs font-medium text-muted-foreground">
										Components ({details.components.length})
									</p>
									<p class="flex flex-wrap gap-1.5">
										{#each details.components as component (component)}
											<code class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">{component}</code>
										{/each}
									</p>
								</div>
							{/if}

							{#if details.events.length > 0}
								<div class="space-y-1">
									<p class="text-xs font-medium text-muted-foreground">Events</p>
									<p class="flex flex-wrap gap-1.5">
										{#each details.events as event (event.name + (event.direction ?? ''))}
											<span class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">
												<code>{event.name}</code>
												<span class="text-muted-foreground">{event.direction ?? 'emit'}</span>
											</span>
										{/each}
									</p>
								</div>
							{/if}

							{#if details.watches.length > 0}
								<div class="space-y-1">
									<p class="text-xs font-medium text-muted-foreground">Watched folders</p>
									<ul class="text-xs text-muted-foreground">
										{#each details.watches as watch (watch.name)}
											<li><code>{watch.name}</code> · {watch.path || '.'} → {watch.function}</li>
										{/each}
									</ul>
								</div>
							{/if}
						{/if}
					</section>

					<section class="space-y-2">
						<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Functions{functionsInfo ? ` (${functionsInfo.functions.length})` : ''}
						</h3>
						{#if functionsLoading}
							<p class="text-muted-foreground">Reading the plugin…</p>
						{:else if functionsError}
							<p class="text-destructive" role="alert">{functionsError}</p>
						{:else if functionsInfo}
							{#if !functionsInfo.inspected}
								<p class="text-xs text-muted-foreground">
									The plugin's code could not be read, so only functions its manifest names are
									listed.
								</p>
							{/if}
							{#if functionsInfo.functions.length === 0}
								<p class="text-muted-foreground">This plugin exposes no functions.</p>
							{:else}
								<ul class="divide-y rounded-md border">
									{#each functionsInfo.functions as fn (fn.name)}
										<li class="space-y-1 px-2.5 py-2">
											<div class="flex flex-wrap items-center gap-1.5">
												<code class="text-xs font-medium">{fn.name}</code>
												{#if fn.public}
													<span class="rounded-sm bg-primary/10 px-1.5 text-[10px] text-primary"
														>public</span
													>
												{/if}
												{#if fn.rule_variable}
													<span class="rounded-sm bg-muted px-1.5 text-[10px]">rule variable</span>
												{/if}
												{#if fn.commands.length > 0}
													<span class="rounded-sm bg-muted px-1.5 text-[10px]">command</span>
												{/if}
												{#if fn.schedules.length > 0}
													<span class="rounded-sm bg-muted px-1.5 text-[10px]">scheduled</span>
												{/if}
												{#if fn.listens_to.length > 0}
													<span class="rounded-sm bg-muted px-1.5 text-[10px]">event handler</span>
												{/if}
												{#if !fn.exported}
													<span class="rounded-sm bg-destructive/10 px-1.5 text-[10px] text-destructive"
														>not in the code</span
													>
												{/if}
											</div>
											{#each fn.commands as command (command.name)}
												<p class="text-xs text-muted-foreground">
													<code>aether --command {plugin.name}.{command.name}</code>
													{#if command.help}· {command.help}{/if}
													{#if command.args?.length}
														· args: {command.args.map((a) => a.name).join(', ')}
													{/if}
													{#if command.on_install || command.on_upgrade}
														· runs on {[command.on_install && 'install', command.on_upgrade && 'upgrade']
															.filter(Boolean)
															.join(' and ')}
													{/if}
												</p>
											{/each}
											{#each fn.schedules as task (task.name)}
												<p class="text-xs text-muted-foreground">
													Schedule <code>{task.name}</code>: {task.cron
														? `cron ${task.cron}`
														: `every ${task.every ?? '?'}`}
												</p>
											{/each}
											{#if fn.listens_to.length > 0}
												<p class="text-xs text-muted-foreground">
													Handles {fn.listens_to.join(', ')}
												</p>
											{/if}
										</li>
									{/each}
								</ul>
								<p class="text-xs text-muted-foreground">
									Called over HTTP as <code>/api/plugins/{plugin.name}/&lt;function&gt;</code>,
									from another plugin with <code>plugins::call</code>.
								</p>
							{/if}
						{/if}
					</section>

					{#if seedMessage}
						<p class="text-muted-foreground" role="status">{seedMessage}</p>
					{/if}
					{#if seedError}
						<p class="text-destructive" role="alert">{seedError}</p>
					{/if}
				</div>

				<section class="min-w-0 space-y-3 rounded-md border border-border bg-muted/20 p-3">
					<div>
						<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Files in this revision
						</h3>
						{#if changeSummary}
							<p class="mt-1 text-xs text-muted-foreground">{changeSummary}</p>
						{:else}
							<p class="mt-1 text-xs text-muted-foreground">
								No file differences recorded for this revision.
							</p>
						{/if}
					</div>

					{#if hasFileDiff}
						{#if plugin.files_changed.length > 0}
							<div class="space-y-1">
								<h4 class="text-xs font-medium text-warning-foreground dark:text-warning">Changed</h4>
								<ul class="max-h-40 space-y-0.5 overflow-y-auto font-mono text-xs">
									{#each plugin.files_changed as path (path)}
										<li class="truncate" title={path}>{path}</li>
									{/each}
								</ul>
							</div>
						{/if}
						{#if plugin.files_added.length > 0}
							<div class="space-y-1">
								<h4 class="text-xs font-medium text-success">
									{plugin.files_changed.length === 0 && plugin.files_removed.length === 0
										? 'Initial files'
										: 'Added'}
								</h4>
								<ul class="max-h-40 space-y-0.5 overflow-y-auto font-mono text-xs">
									{#each plugin.files_added as path (path)}
										<li class="truncate" title={path}>{path}</li>
									{/each}
								</ul>
							</div>
						{/if}
						{#if plugin.files_removed.length > 0}
							<div class="space-y-1">
								<h4 class="text-xs font-medium text-destructive">Removed</h4>
								<ul class="max-h-40 space-y-0.5 overflow-y-auto font-mono text-xs">
									{#each plugin.files_removed as path (path)}
										<li class="truncate" title={path}>{path}</li>
									{/each}
								</ul>
							</div>
						{/if}
					{/if}
				</section>
			</div>

			<Dialog.Footer class="flex-wrap gap-2 sm:justify-end">
				<Button variant="outline" onclick={() => (open = false)}>Close</Button>
				{#if canSeed}
					<Button variant="outline" disabled={seeding} onclick={() => void onSeed()}>
						{seeding ? 'Seeding…' : 'Seed data'}
					</Button>
				{/if}
				{#if openHref}
					<Button variant="secondary" href={openHref}>Open app</Button>
				{/if}
				{#if onInstall}
					<Button
						onclick={() => {
							onInstall(plugin);
							open = false;
						}}
					>
						Install into org
					</Button>
				{/if}
			</Dialog.Footer>
		</Dialog.Content>
	{/if}
</Dialog.Root>
